#include "osr/accelerated/paint.h"

#include <cstdio>
#include <string>

#include "osr/accelerated/protocol.h"
#include "common/trace.h"

#if defined(OS_LINUX)
#include <thread>
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

void AcceleratedPaint::RetireResources(
    const std::vector<uint64_t>& resource_ids) {
  std::vector<uint64_t> announced;
  for (const uint64_t resource_id : resource_ids) {
    if (announced_resources_.erase(resource_id)) {
      announced.push_back(resource_id);
    }
  }
  if (!announced.empty()) {
    SendRetire(announced);
  }
}

void AcceleratedPaint::RetireBrowser(int browser_id) {
  dropped_.Forget(browser_id);
#if defined(OS_LINUX)
  unsettled_frames_.erase({browser_id, false});
#endif
  RetireSurfaces(browser_id);
  copied_captures_.erase({browser_id, false});
  copied_captures_.erase({browser_id, true});
}

bool AcceleratedPaint::FollowsCopiedCapture(
    const AcceleratedSurfaceKey& surface,
    const cef_accelerated_paint_info_common_t& frame) const {
  if (!frame.has_capture_counter) {
    return true;
  }
  const auto copied = copied_captures_.find(surface);
  return copied != copied_captures_.end() &&
         copied->second + 1 == frame.capture_counter;
}

std::optional<CefRect> AcceleratedPaint::Accept(
    const CefAcceleratedPaintInfo& info) {
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
    Drop({AcceleratedDropKind::kColorFormat, static_cast<int>(info.format)});
    return std::nullopt;
  }
  const int frame_w = info.extra.coded_size.width;
  const int frame_h = info.extra.coded_size.height;
  if (frame_w <= 0 || frame_h <= 0) {
    return std::nullopt;
  }
  const CefRect& content_rect = info.extra.content_rect;
  const bool full_content = content_rect.x == 0 && content_rect.y == 0 &&
                            content_rect.width == frame_w &&
                            content_rect.height == frame_h;
  const bool source_matches =
      !info.extra.has_source_size || (info.extra.source_size.width == frame_w &&
                                      info.extra.source_size.height == frame_h);
  if (!full_content || !source_matches) {
    Drop({AcceleratedDropKind::kPartialFrame, frame_w, frame_h,
          content_rect.width, content_rect.height});
    return std::nullopt;
  }
  return VisibleWithin(info.extra.visible_rect, frame_w, frame_h);
}

AcceleratedPaint::Presented AcceleratedPaint::Present(
    CefRefPtr<CefBrowser> browser,
    cef_paint_element_type_t type,
    const CefRenderHandler::RectList& dirty_rects,
    const CefAcceleratedPaintInfo& info,
    const AcceleratedTarget& target,
    const CefRect& visible) {
  const int frame_w = info.extra.coded_size.width;
  const int frame_h = info.extra.coded_size.height;
  const AcceleratedSurfaceKey surface{browser->GetIdentifier(),
                                      type == PET_POPUP};
  PixelRegion damage;
  if (FollowsCopiedCapture(surface, info.extra)) {
    for (const CefRect& rect : dirty_rects) {
      damage.Unite(PixelRegion{rect.x, rect.y, rect.width, rect.height});
    }
  }
  if (damage.empty()) {
    damage = PixelRegion::Whole(frame_w, frame_h);
  }
  if (info.extra.has_capture_counter) {
    copied_captures_[surface] = info.extra.capture_counter;
  }
  CopiedFrame copied{};
  if (!Copy(surface, damage, info, &copied)) {
    Drop({AcceleratedDropKind::kHandOver});
    dropped_.Add(surface);
    return Presented::kDropped;
  }
  if (static_cast<int>(copied.width) != frame_w ||
      static_cast<int>(copied.height) != frame_h) {
    Drop({AcceleratedDropKind::kTextureSize, static_cast<int>(copied.width),
          static_cast<int>(copied.height), frame_w, frame_h});
    Discard(copied);
    return Presented::kDropped;
  }
  const Content content{static_cast<uint32_t>(info.format), visible};
  const bool sent = Send(target, copied, content);
#if defined(OS_LINUX)
  if (sent && type == PET_VIEW) {
    SettleWhenPaused(surface, browser, content);
  }
#endif
  return sent ? Presented::kSent : Presented::kUnsent;
}

bool AcceleratedPaint::Send(const AcceleratedTarget& target,
                            const CopiedFrame& copied,
                            const Content& content) {
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
  meta.damage = copied.damage;
  meta.sequence = copied.sequence;
  BuildAccelPayload(target.guest_id, meta, &payload_);
  const bool sent = transport_.Send(target.kind, copied.width, copied.height,
                                    target.x, target.y, payload_.data(),
                                    static_cast<uint32_t>(payload_.size()));
  if (!sent) {
    Discard(copied);
  }
  return sent;
}

}  // namespace sabine_osr
