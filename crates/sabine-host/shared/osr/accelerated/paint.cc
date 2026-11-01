#include "osr/accelerated/paint.h"

#include <cstdio>
#include <string>

#include "osr/accelerated/protocol.h"
#include "common/trace.h"
#include "osr/handler.h"

#if defined(OS_LINUX)
#include <thread>

#include "osr/accelerated/linux/dmabuf_copy.h"
#endif

namespace sabine_osr {

bool PreferSharedTexture(CefRefPtr<CefCommandLine> command_line) {
  const bool preferred =
      command_line && command_line->HasSwitch("sabine-shared-texture");
#if defined(OS_LINUX)
  if (preferred) {
    std::thread(AcceleratedDmabufAvailable).detach();
  }
#endif
  return preferred;
}

void ApplySharedTexture(CefWindowInfo* window_info, bool enabled) {
  if (!window_info) {
    return;
  }
  window_info->shared_texture_enabled = enabled ? 1 : 0;
}

}  // namespace sabine_osr

using namespace sabine_osr;

namespace {

CefRect VisibleWithin(const CefRect& visible, int width, int height) {
  const int64_t right = static_cast<int64_t>(visible.x) + visible.width;
  const int64_t bottom = static_cast<int64_t>(visible.y) + visible.height;
  if (visible.x < 0 || visible.y < 0 || visible.width <= 0 ||
      visible.height <= 0 || right > width || bottom > height) {
    return CefRect(0, 0, width, height);
  }
  return visible;
}

bool SupportedFormat(cef_color_type_t format) {
  return format == CEF_COLOR_TYPE_BGRA_8888 ||
         format == CEF_COLOR_TYPE_RGBA_8888;
}

}  // namespace

void SabineOsrHandler::RetireAcceleratedResources(
    const std::vector<uint64_t>& resource_ids) {
  std::vector<uint64_t> announced;
  for (const uint64_t resource_id : resource_ids) {
    if (announced_accelerated_resources_.erase(resource_id)) {
      announced.push_back(resource_id);
    }
  }
  if (!announced.empty()) {
    SendAcceleratedRetire(announced);
  }
}

void SabineOsrHandler::ReleaseAcceleratedFrame(uint64_t slot_token) {
  ReleaseAcceleratedSlot(slot_token);
  for (const AcceleratedSurfaceKey& surface : dropped_accelerated_paints_) {
    for (const CefRefPtr<CefBrowser>& browser : browsers_) {
      if (browser->GetIdentifier() == surface.browser_id) {
        browser->GetHost()->Invalidate(surface.popup ? PET_POPUP : PET_VIEW);
      }
    }
  }
  dropped_accelerated_paints_.clear();
}

void SabineOsrHandler::RetireAcceleratedBrowser(int browser_id) {
  dropped_accelerated_paints_.erase({browser_id, false});
  dropped_accelerated_paints_.erase({browser_id, true});
#if defined(OS_LINUX)
  unsettled_frames_.erase({browser_id, false});
#endif
  RetireAcceleratedSurfaces(browser_id);
}

bool SabineOsrHandler::FollowsCopiedCapture(
    const AcceleratedSurfaceKey& surface,
    const cef_accelerated_paint_info_common_t& frame) const {
  if (!frame.has_capture_counter) {
    return true;
  }
  const auto copied = copied_captures_.find(surface);
  return copied != copied_captures_.end() &&
         copied->second + 1 == frame.capture_counter;
}

void SabineOsrHandler::ForgetAcceleratedCaptures(int browser_id) {
  copied_captures_.erase({browser_id, false});
  copied_captures_.erase({browser_id, true});
}

void SabineOsrHandler::OnAcceleratedPaint(CefRefPtr<CefBrowser> browser,
                                          PaintElementType type,
                                          const RectList& dirtyRects,
                                          const CefAcceleratedPaintInfo& info) {
  if (!browser) {
    return;
  }
  static bool traced_first_callback = false;
  if (!traced_first_callback && TraceEnabled()) {
    traced_first_callback = true;
    std::fprintf(stderr,
                 "Sabine CEF: first accelerated paint format=%d coded=%dx%d "
                 "visible=%d,%d %dx%d\n",
                 static_cast<int>(info.format), info.extra.coded_size.width,
                 info.extra.coded_size.height, info.extra.visible_rect.x,
                 info.extra.visible_rect.y, info.extra.visible_rect.width,
                 info.extra.visible_rect.height);
    std::fflush(stderr);
  }
  if (!SupportedFormat(info.format)) {
    DropAcceleratedFrame(
        {AcceleratedDropKind::kColorFormat, static_cast<int>(info.format)});
    return;
  }
  const int frame_w = info.extra.coded_size.width;
  const int frame_h = info.extra.coded_size.height;
  if (frame_w <= 0 || frame_h <= 0) {
    return;
  }
  const CefRect& content_rect = info.extra.content_rect;
  const bool full_content = content_rect.x == 0 && content_rect.y == 0 &&
                            content_rect.width == frame_w &&
                            content_rect.height == frame_h;
  const bool source_matches =
      !info.extra.has_source_size || (info.extra.source_size.width == frame_w &&
                                      info.extra.source_size.height == frame_h);
  if (!full_content || !source_matches) {
    DropAcceleratedFrame({AcceleratedDropKind::kPartialFrame, frame_w, frame_h,
                          content_rect.width, content_rect.height});
    return;
  }
  const CefRect visible =
      VisibleWithin(info.extra.visible_rect, frame_w, frame_h);
  if (type == PET_VIEW && !QualifyResizeFrame(visible.width, visible.height)) {
    DropAcceleratedFrame(
        {AcceleratedDropKind::kWrongSize, visible.width, visible.height});
    browser->GetHost()->Invalidate(PET_VIEW);
    return;
  }
  const std::optional<AcceleratedTarget> target =
      AcceleratedTargetFor(browser, type);
  if (!target) {
    return;
  }
  const AcceleratedSurfaceKey surface{browser->GetIdentifier(),
                                      type == PET_POPUP};
  PixelRegion damage;
  if (FollowsCopiedCapture(surface, info.extra)) {
    for (const CefRect& rect : dirtyRects) {
      damage.Unite(PixelRegion{rect.x, rect.y, rect.width, rect.height});
    }
  }
  if (damage.empty()) {
    damage = PixelRegion::Whole(frame_w, frame_h);
  }
  if (info.extra.has_capture_counter) {
    copied_captures_[surface] = info.extra.capture_counter;
  }
  CopiedAccelFrame copied{};
  if (!CopyAcceleratedFrame(surface, damage, info, &copied)) {
    DropAcceleratedFrame({AcceleratedDropKind::kHandOver});
    dropped_accelerated_paints_.insert(surface);
    return;
  }
  if (static_cast<int>(copied.width) != frame_w ||
      static_cast<int>(copied.height) != frame_h) {
    DropAcceleratedFrame({AcceleratedDropKind::kTextureSize,
                          static_cast<int>(copied.width),
                          static_cast<int>(copied.height), frame_w, frame_h});
    DiscardAcceleratedFrame(copied);
    return;
  }
  const AcceleratedContent content{static_cast<uint32_t>(info.format), visible};
  const bool sent = SendAcceleratedFrame(*target, copied, content);
#if defined(OS_LINUX)
  if (sent && type == PET_VIEW) {
    SettleWhenPaused(surface, browser, content);
  }
#endif
  if (GuestView* guest = GuestForBrowser(browser)) {
    if (sent && type == PET_VIEW && !guest->painted) {
      guest->painted = true;
      if (guest->id == kSabinePopupGuestId) {
        EmitBridgeEvent("\"popup.open\"", "{}");
      }
    }
    return;
  }
  accelerated_paint_seen_ |= sent && type == PET_VIEW;
  if (type == PET_VIEW) {
    CompleteResizeFrame(visible.width, visible.height);
  }
}

std::optional<SabineOsrHandler::AcceleratedTarget>
SabineOsrHandler::AcceleratedTargetFor(CefRefPtr<CefBrowser> browser,
                                       PaintElementType type) {
  if (GuestView* guest = GuestForBrowser(browser)) {
    if (type == PET_POPUP) {
      return AcceleratedTarget{kGuestAccel, guest->id + "/popup",
                               guest->bounds.x + guest_popup_rect_.x,
                               guest->bounds.y + guest_popup_rect_.y};
    }
    return AcceleratedTarget{kGuestAccel, guest->id, guest->bounds.x,
                             guest->bounds.y};
  }
  if (view_hidden_ || !browser_ || !browser_->IsSame(browser)) {
    return std::nullopt;
  }
  if (type == PET_POPUP) {
    return AcceleratedTarget{kPopupAccel, std::string(), popup_rect_.x,
                             popup_rect_.y};
  }
  return AcceleratedTarget{kMainAccel, std::string(), 0, 0};
}

bool SabineOsrHandler::SendAcceleratedFrame(const AcceleratedTarget& target,
                                            const CopiedAccelFrame& copied,
                                            const AcceleratedContent& content) {
  AccelPaintMeta meta;
  meta.format = content.format;
  meta.visible_x = content.visible.x;
  meta.visible_y = content.visible.y;
  meta.visible_width = static_cast<uint32_t>(content.visible.width);
  meta.visible_height = static_cast<uint32_t>(content.visible.height);
  meta.resource_id = copied.resource_id;
  meta.resource_slot = copied.resource_slot;
  meta.shared_handle = copied.shared_handle;
  meta.slot_token = copied.slot_token;
  BuildAccelPayload(target.guest_id, meta, &accel_payload_);
  const bool sent = SendMessage(target.kind, copied.width, copied.height,
                                target.x, target.y, accel_payload_.data(),
                                static_cast<uint32_t>(accel_payload_.size()));
  if (!sent) {
    DiscardAcceleratedFrame(copied);
  }
  return sent;
}
