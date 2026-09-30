#include "osr/accelerated/paint.h"

#include <algorithm>
#include <cstdio>
#include <cstdlib>

#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "osr/handler.h"
#include "osr/accelerated/protocol.h"
#include "osr/utilities.h"

#if defined(OS_WIN)
#include "osr/accelerated/windows/d3d11_copy.h"
#include <windows.h>
#elif defined(OS_MAC)
#include "osr/accelerated/macos/iosurface_copy.h"
#elif defined(OS_LINUX)
#include <thread>

#include "osr/accelerated/linux/dmabuf_copy.h"

constexpr int64_t kAcceleratedPaintGraceMs = 2000;
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

#if defined(OS_WIN)
HANDLE OpenParentForHandleDuplication() {
  CefRefPtr<CefCommandLine> command_line =
      CefCommandLine::GetGlobalCommandLine();
  const int parent_pid = SwitchInt(command_line, "sabine-parent-pid", 0);
  if (parent_pid <= 0) {
    return nullptr;
  }
  return OpenProcess(PROCESS_DUP_HANDLE, FALSE, static_cast<DWORD>(parent_pid));
}

uint64_t DuplicateHandleToParent(HANDLE shared) {
  if (!shared) {
    return 0;
  }
  HANDLE parent = OpenParentForHandleDuplication();
  if (!parent) {
    return 0;
  }
  // Windows process handles are table-local values. The numeric handle sent
  // over IPC must be created directly in the compositor process; sending this
  // process's value would intermittently open an unrelated object instead.
  HANDLE remote = nullptr;
  const BOOL ok = DuplicateHandle(GetCurrentProcess(), shared, parent, &remote,
                                  0, FALSE, DUPLICATE_SAME_ACCESS);
  CloseHandle(parent);
  if (!ok || !remote) {
    return 0;
  }
  return reinterpret_cast<uint64_t>(remote);
}

void CloseHandleInParent(uint64_t remote_value) {
  if (remote_value == 0) {
    return;
  }
  HANDLE parent = OpenParentForHandleDuplication();
  if (!parent) {
    return;
  }
  HANDLE local = nullptr;
  DuplicateHandle(parent, reinterpret_cast<HANDLE>(remote_value),
                  GetCurrentProcess(), &local, 0, FALSE,
                  DUPLICATE_SAME_ACCESS | DUPLICATE_CLOSE_SOURCE);
  if (local) {
    CloseHandle(local);
  }
  CloseHandle(parent);
}
#endif

}  // namespace sabine_osr

using namespace sabine_osr;

#if defined(OS_WIN)
bool SabineOsrHandler::CopyAcceleratedFrame(const std::string& slot_key,
                                            const CefAcceleratedPaintInfo& info,
                                            int width,
                                            int height,
                                            CopiedAccelFrame* out) {
  AccelD3d11CopiedFrame copied{};
  const bool copied_frame = CopyAcceleratedD3d11Frame(
      slot_key, info.shared_texture_handle, width, height,
      static_cast<uint32_t>(info.format), &copied);
  RetireAcceleratedResources(copied.retired_resource_ids);
  if (!copied_frame) {
    return false;
  }
  uint64_t shared_handle = 0;
  if (!announced_accelerated_resources_.count(copied.resource_id)) {
    shared_handle = DuplicateHandleToParent(copied.shared_handle);
    if (shared_handle == 0) {
      ReleaseAcceleratedD3d11Frame(copied.slot_token);
      EmitBridgeEvent("\"osr.accel_handle_failed\"", "{}");
      return false;
    }
    announced_accelerated_resources_.insert(copied.resource_id);
  }
  *out = {copied.width,      copied.height, copied.resource_id,
          copied.slot_index, shared_handle, copied.slot_token};
  return true;
}

void SabineOsrHandler::DiscardAcceleratedFrame(const CopiedAccelFrame& frame) {
  if (frame.shared_handle != 0) {
    announced_accelerated_resources_.erase(frame.resource_id);
    CloseHandleInParent(frame.shared_handle);
  }
  ReleaseAcceleratedD3d11Frame(frame.slot_token);
}

void SabineOsrHandler::ReleaseAcceleratedSlot(uint64_t slot_token) {
  ReleaseAcceleratedD3d11Frame(slot_token);
}

void SabineOsrHandler::RetireAcceleratedBrowser(int browser_id) {
  RetireAcceleratedResources(RetireAcceleratedD3d11Browser(browser_id));
}
#elif defined(OS_MAC)
void SabineOsrHandler::UseSurfaceService(const std::string& service_name) {
  if (!service_name.empty()) {
    surface_broker_ =
        std::make_unique<SurfaceBroker>(service_name, authentication_token_);
  }
}

bool SabineOsrHandler::CopyAcceleratedFrame(const std::string& slot_key,
                                            const CefAcceleratedPaintInfo& info,
                                            int width,
                                            int height,
                                            CopiedAccelFrame* out) {
  (void)width;
  (void)height;
  if (!surface_broker_) {
    return false;
  }
  AccelIOSurfaceCopiedFrame copied{};
  const bool copied_frame = CopyAcceleratedIOSurfaceFrame(
      slot_key, info.shared_texture_io_surface, &copied);
  RetireAcceleratedResources(copied.retired_surface_ids);
  if (!copied_frame) {
    return false;
  }
  if (!surface_broker_->Announce(copied.surface_id, copied.surface)) {
    ReleaseAcceleratedIOSurfaceFrame(copied.slot_token);
    EmitBridgeEvent("\"osr.accel_handle_failed\"", "{}");
    return false;
  }
  *out = {
      copied.width,     copied.height, copied.surface_id, copied.slot_index, 0,
      copied.slot_token};
  return true;
}

void SabineOsrHandler::DiscardAcceleratedFrame(const CopiedAccelFrame& frame) {
  ReleaseAcceleratedIOSurfaceFrame(frame.slot_token);
}

void SabineOsrHandler::ReleaseAcceleratedSlot(uint64_t slot_token) {
  ReleaseAcceleratedIOSurfaceFrame(slot_token);
}

void SabineOsrHandler::RetireAcceleratedResources(
    const std::vector<uint64_t>& resource_ids) {
  if (!surface_broker_) {
    return;
  }
  for (const uint64_t surface_id : resource_ids) {
    surface_broker_->Retire(surface_id);
  }
}

void SabineOsrHandler::RetireAcceleratedBrowser(int browser_id) {
  RetireAcceleratedResources(RetireAcceleratedIOSurfaceBrowser(browser_id));
}
#elif defined(OS_LINUX)
bool SabineOsrHandler::CopyAcceleratedFrame(const std::string& slot_key,
                                            const CefAcceleratedPaintInfo& info,
                                            int width,
                                            int height,
                                            CopiedAccelFrame* out) {
  (void)width;
  (void)height;
  AccelDmabufCopiedFrame copied{};
  const DmabufCopy result = CopyAcceleratedDmabufFrame(slot_key, info, &copied);
  RetireAcceleratedResources(copied.retired_resource_ids);
  if (result == DmabufCopy::kFailed) {
    ReportAcceleratedPaintUnavailable();
  }
  if (result != DmabufCopy::kCopied) {
    return false;
  }
  if (!announced_accelerated_resources_.count(copied.resource_id)) {
    const std::string payload = BuildDmabufAnnouncePayload(
        copied.resource_id, copied.modifier, copied.stride, copied.offset);
    if (!SendMessageWithFd(kAccelDmabuf, copied.width, copied.height, 0, 0,
                           payload.data(),
                           static_cast<uint32_t>(payload.size()), copied.fd)) {
      ReleaseAcceleratedDmabufFrame(copied.slot_token);
      return false;
    }
    announced_accelerated_resources_.insert(copied.resource_id);
  }
  *out = {
      copied.width,     copied.height, copied.resource_id, copied.slot_index, 0,
      copied.slot_token};
  return true;
}

void SabineOsrHandler::DiscardAcceleratedFrame(const CopiedAccelFrame& frame) {
  ReleaseAcceleratedDmabufFrame(frame.slot_token);
}

void SabineOsrHandler::ReleaseAcceleratedSlot(uint64_t slot_token) {
  ReleaseAcceleratedDmabufFrame(slot_token);
}

void SabineOsrHandler::RetireAcceleratedBrowser(int browser_id) {
  RetireAcceleratedResources(RetireAcceleratedDmabufBrowser(browser_id));
}

void SabineOsrHandler::UseAcceleratedPaint(bool enabled) {
  accelerated_paint_ = enabled;
}

void SabineOsrHandler::WatchAcceleratedPaint() {
  if (!accelerated_paint_ || accelerated_paint_seen_) {
    return;
  }
  CefRefPtr<SabineOsrHandler> self(this);
  CefPostDelayedTask(TID_UI,
                     CefCreateClosureTask(base::BindOnce(
                         [](CefRefPtr<SabineOsrHandler> handler) {
                           if (!handler->accelerated_paint_seen_ &&
                               !handler->view_hidden_ && handler->browser_) {
                             handler->ReportAcceleratedPaintUnavailable();
                           }
                         },
                         self)),
                     kAcceleratedPaintGraceMs);
}

void SabineOsrHandler::ReportAcceleratedPaintUnavailable() {
  if (accelerated_paint_failed_) {
    return;
  }
  accelerated_paint_failed_ = true;
  SendMessage(kAccelUnavailable, 0, 0, 0, 0, nullptr, 0);
}
#endif

#if defined(OS_WIN) || defined(OS_LINUX)
void SabineOsrHandler::RetireAcceleratedResources(
    const std::vector<uint64_t>& resource_ids) {
  std::vector<uint64_t> announced;
  for (const uint64_t resource_id : resource_ids) {
    if (announced_accelerated_resources_.erase(resource_id)) {
      announced.push_back(resource_id);
    }
  }
  if (announced.empty()) {
    return;
  }
  const std::string payload = BuildAccelRetirePayload(announced);
  SendMessage(kAccelRetire, 0, 0, 0, 0, payload.data(),
              static_cast<uint32_t>(payload.size()));
}
#endif

void SabineOsrHandler::ReleaseAcceleratedFrame(uint64_t slot_token) {
  ReleaseAcceleratedSlot(slot_token);
  for (auto& [browser, type] : dropped_accelerated_paints_) {
    browser->GetHost()->Invalidate(type);
  }
  dropped_accelerated_paints_.clear();
}

void SabineOsrHandler::OnAcceleratedPaint(CefRefPtr<CefBrowser> browser,
                                          PaintElementType type,
                                          const RectList& dirtyRects,
                                          const CefAcceleratedPaintInfo& info) {
  (void)dirtyRects;
  if (!browser) {
    return;
  }
#if defined(OS_LINUX)
  accelerated_paint_seen_ = true;
#endif

  static bool traced_first_callback = false;
  if (!traced_first_callback && std::getenv("SABINE_TRACE")) {
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

#if defined(OS_WIN)
  const bool supported_format = info.format == CEF_COLOR_TYPE_BGRA_8888;
#else
  const bool supported_format = info.format == CEF_COLOR_TYPE_BGRA_8888 ||
                                info.format == CEF_COLOR_TYPE_RGBA_8888;
#endif
  if (!supported_format) {
    EmitBridgeEvent("\"osr.accel_unsupported\"", "{}");
    return;
  }
  const int width = type == PET_POPUP ? popup_rect_.width : width_;
  const int height = type == PET_POPUP ? popup_rect_.height : height_;
  const int frame_w =
      info.extra.coded_size.width > 0 ? info.extra.coded_size.width : width;
  const int frame_h =
      info.extra.coded_size.height > 0 ? info.extra.coded_size.height : height;
  if (frame_w <= 0 || frame_h <= 0) {
    return;
  }
  const bool full_content = info.extra.content_rect.x == 0 &&
                            info.extra.content_rect.y == 0 &&
                            info.extra.content_rect.width == frame_w &&
                            info.extra.content_rect.height == frame_h;
  const bool source_matches =
      !info.extra.has_source_size || (info.extra.source_size.width == frame_w &&
                                      info.extra.source_size.height == frame_h);
  if (!full_content || !source_matches) {
    return;
  }
  CefRect reported_visible = info.extra.visible_rect;
  const int64_t reported_right =
      static_cast<int64_t>(reported_visible.x) + reported_visible.width;
  const int64_t reported_bottom =
      static_cast<int64_t>(reported_visible.y) + reported_visible.height;
  if (reported_visible.x < 0 || reported_visible.y < 0 ||
      reported_visible.width <= 0 || reported_visible.height <= 0 ||
      reported_right > frame_w || reported_bottom > frame_h) {
    reported_visible = CefRect(0, 0, frame_w, frame_h);
  }
  if (type == PET_VIEW &&
      !QualifyResizeFrame(reported_visible.width, reported_visible.height)) {
    browser->GetHost()->Invalidate(PET_VIEW);
    return;
  }
  auto send_accel = [&](uint32_t accel_kind, const std::string& guest_id,
                        int32_t x, int32_t y) -> bool {
    const std::string slot_key = std::to_string(browser->GetIdentifier()) +
                                 (type == PET_POPUP ? "/popup" : "/view");
    CopiedAccelFrame copied{};
    if (!CopyAcceleratedFrame(slot_key, info, frame_w, frame_h, &copied)) {
      dropped_accelerated_paints_.emplace_back(browser, type);
      EmitBridgeEvent("\"osr.accel_copy_dropped\"", "{}");
      return false;
    }
    const int coded_width = static_cast<int>(copied.width);
    const int coded_height = static_cast<int>(copied.height);
    CefRect visible = info.extra.visible_rect;
    const int64_t visible_right =
        static_cast<int64_t>(visible.x) + visible.width;
    const int64_t visible_bottom =
        static_cast<int64_t>(visible.y) + visible.height;
    if (visible.x < 0 || visible.y < 0 || visible.width <= 0 ||
        visible.height <= 0 || visible_right > coded_width ||
        visible_bottom > coded_height) {
      visible = CefRect(0, 0, coded_width, coded_height);
    }
    if (std::getenv("SABINE_TRACE") &&
        (frame_w != coded_width || frame_h != coded_height)) {
      std::fprintf(stderr,
                   "Sabine CEF: accelerated metadata mismatch reported=%dx%d "
                   "resource=%dx%d\n",
                   frame_w, frame_h, coded_width, coded_height);
      std::fflush(stderr);
    }

    AccelPaintMeta meta;
    meta.format = static_cast<uint32_t>(info.format);
    meta.visible_x = visible.x;
    meta.visible_y = visible.y;
    meta.visible_width = static_cast<uint32_t>(visible.width);
    meta.visible_height = static_cast<uint32_t>(visible.height);
    meta.resource_id = copied.resource_id;
    meta.resource_slot = copied.resource_slot;
    meta.shared_handle = copied.shared_handle;
    meta.slot_token = copied.slot_token;

    const std::string payload = BuildAccelPayload(guest_id, meta);
    const bool sent =
        !payload.empty() &&
        SendMessage(accel_kind, copied.width, copied.height, x, y,
                    payload.data(), static_cast<uint32_t>(payload.size()));
    if (!sent) {
      DiscardAcceleratedFrame(copied);
    }
    return sent;
  };

  if (GuestView* guest = GuestForBrowser(browser)) {
    if (type == PET_POPUP) {
      send_accel(kGuestAccel, guest->id + "/popup",
                 guest->bounds.x + guest_popup_rect_.x,
                 guest->bounds.y + guest_popup_rect_.y);
      return;
    }
    if (send_accel(kGuestAccel, guest->id, guest->bounds.x, guest->bounds.y) &&
        !guest->painted) {
      guest->painted = true;
      if (guest->id == kSabinePopupGuestId) {
        EmitBridgeEvent("\"popup.open\"", "{}");
      }
    }
    return;
  }
  if (view_hidden_ || !browser_ || !browser_->IsSame(browser)) {
    return;
  }
  const uint32_t kind = type == PET_POPUP ? kPopupAccel : kMainAccel;
  const int32_t x = type == PET_POPUP ? popup_rect_.x : 0;
  const int32_t y = type == PET_POPUP ? popup_rect_.y : 0;
  send_accel(kind, std::string(), x, y);
  if (type == PET_VIEW) {
    CompleteResizeFrame(reported_visible.width, reported_visible.height);
  }
}
