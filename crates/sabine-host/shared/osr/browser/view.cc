#include "osr/handler.h"

#include <cstdio>
#include <string>
#include <vector>

#include "common/trace.h"
#include "include/wrapper/cef_helpers.h"

using namespace sabine_osr;

bool SabineOsrHandler::OnCursorChange(CefRefPtr<CefBrowser> browser,
                                      CefCursorHandle cursor,
                                      cef_cursor_type_t type,
                                      const CefCursorInfo& custom_cursor_info) {
  std::vector<char> payload(4);
  PutU32(&payload, 0, static_cast<uint32_t>(type));
  const CefSize& size = custom_cursor_info.size;
  const bool image = type == CT_CUSTOM && custom_cursor_info.buffer &&
                     size.width > 0 && size.height > 0;
  if (image) {
    const char* pixels = static_cast<const char*>(custom_cursor_info.buffer);
    payload.insert(payload.end(), pixels,
                   pixels + static_cast<size_t>(size.width) * size.height * 4);
  }
  SendMessage(kCursor, image ? size.width : 0, image ? size.height : 0,
              custom_cursor_info.hotspot.x, custom_cursor_info.hotspot.y,
              payload.data(), static_cast<uint32_t>(payload.size()));
  return true;
}

void SabineOsrHandler::OnDraggableRegionsChanged(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    const std::vector<CefDraggableRegion>& regions) {
  CEF_REQUIRE_UI_THREAD();
  if (!browser_ || !browser_->IsSame(browser)) {
    return;
  }
  constexpr size_t kEntryLen = 20;
  std::vector<char> payload(4 + regions.size() * kEntryLen, 0);
  PutU32(&payload, 0, static_cast<uint32_t>(regions.size()));
  for (size_t index = 0; index < regions.size(); ++index) {
    const auto& region = regions[index];
    const size_t offset = 4 + index * kEntryLen;
    PutI32(&payload, offset, region.bounds.x);
    PutI32(&payload, offset + 4, region.bounds.y);
    PutI32(&payload, offset + 8, region.bounds.width);
    PutI32(&payload, offset + 12, region.bounds.height);
    PutU32(&payload, offset + 16, region.draggable ? 1 : 0);
  }
  SendMessage(kDraggableRegionsChanged, 0, 0, 0, 0, payload.data(),
              static_cast<uint32_t>(payload.size()));
}

bool SabineOsrHandler::OnTooltip(CefRefPtr<CefBrowser> browser,
                                 CefString& text) {
  CEF_REQUIRE_UI_THREAD();
  const std::string value = text;
  SendMessage(kTooltipChanged, 0, 0, 0, 0, value.data(),
              static_cast<uint32_t>(value.size()));
  return true;
}

bool SabineOsrHandler::GetScreenInfo(CefRefPtr<CefBrowser> browser,
                                     CefScreenInfo& screen_info) {
  screen_info.device_scale_factor = scale_;
  screen_info.depth = 32;
  screen_info.depth_per_component = 8;
  if (const GuestView* guest = GuestForBrowser(browser)) {
    screen_info.rect = CefRect(0, 0, guest->bounds.width, guest->bounds.height);
    screen_info.available_rect = screen_info.rect;
    return true;
  }
  screen_info.rect = CefRect(0, 0, width_, height_);
  screen_info.available_rect = CefRect(0, 0, width_, height_);
  return true;
}

void SabineOsrHandler::GetViewRect(CefRefPtr<CefBrowser> browser,
                                   CefRect& rect) {
  if (const GuestView* guest = GuestForBrowser(browser)) {
    rect = CefRect(0, 0, guest->bounds.width, guest->bounds.height);
    return;
  }
  rect = CefRect(0, 0, width_, height_);
}

void SabineOsrHandler::OnPopupShow(CefRefPtr<CefBrowser> browser, bool show) {
  if (GuestView* guest = GuestForBrowser(browser)) {
    if (!show) {
      const std::string popup_id = guest->id + "/popup";
      SendMessage(kGuestHidden, 0, 0, guest->bounds.x + guest_popup_rect_.x,
                  guest->bounds.y + guest_popup_rect_.y, popup_id.data(),
                  static_cast<uint32_t>(popup_id.size()));
      guest_popup_rect_ = CefRect();
    }
    return;
  }
  if (!show) {
    SendMessage(kPopupHidden, 0, 0, 0, 0, nullptr, 0);
  }
}

void SabineOsrHandler::OnPopupSize(CefRefPtr<CefBrowser> browser,
                                   const CefRect& rect) {
  if (GuestForBrowser(browser)) {
    if (guest_popup_rect_.x != rect.x || guest_popup_rect_.y != rect.y ||
        guest_popup_rect_.width != rect.width ||
        guest_popup_rect_.height != rect.height) {
      if (GuestView* guest = GuestForBrowser(browser)) {
        const std::string popup_id = guest->id + "/popup";
        SendMessage(kGuestHidden, 0, 0, guest->bounds.x + guest_popup_rect_.x,
                    guest->bounds.y + guest_popup_rect_.y, popup_id.data(),
                    static_cast<uint32_t>(popup_id.size()));
      }
    }
    guest_popup_rect_ = rect;
    return;
  }
  if (popup_rect_.x != rect.x || popup_rect_.y != rect.y ||
      popup_rect_.width != rect.width || popup_rect_.height != rect.height) {
    SendMessage(kPopupHidden, 0, 0, 0, 0, nullptr, 0);
  }
  popup_rect_ = rect;
}

void SabineOsrHandler::OnPaint(CefRefPtr<CefBrowser> browser,
                               PaintElementType type,
                               const RectList& dirtyRects,
                               const void* buffer,
                               int width,
                               int height) {
  static bool traced_first_software_paint = false;
  if (!traced_first_software_paint && TraceEnabled()) {
    traced_first_software_paint = true;
    std::fprintf(stderr, "Sabine CEF: first software paint size=%dx%d\n", width,
                 height);
    std::fflush(stderr);
  }
  if (GuestView* guest = GuestForBrowser(browser)) {
    if (type == PET_POPUP) {
      SendPaintBatch(PaintSurface::kGuest, guest->id + "/popup",
                     guest->bounds.x + guest_popup_rect_.x,
                     guest->bounds.y + guest_popup_rect_.y, buffer, width,
                     height, dirtyRects);
      return;
    }
    if (!guest->painted) {
      guest->painted = true;
      if (guest->id == kSabinePopupGuestId) {
        EmitBridgeEvent("\"popup.open\"", "{}");
      }
    }
    if (guest->visible) {
      SendGuestPaint(*guest, buffer, width, height, dirtyRects);
    }
    return;
  }
  if (view_hidden_ || !browser_ || !browser_->IsSame(browser)) {
    return;
  }
#if defined(OS_LINUX)
  // Chromium hands over shared textures only while it composites on the GPU.
  // Software frames mean its GPU process is gone or was never usable, so no
  // shared frame will come and the window has to take these instead.
  if (accelerated_paint_ && type == PET_VIEW) {
    accelerated_drop_reason_ = "Chromium paints without the GPU";
    ReportAcceleratedPaintUnavailable();
  }
#endif
  const PaintSurface surface =
      type == PET_POPUP ? PaintSurface::kPopup : PaintSurface::kMain;
  const int32_t x = type == PET_POPUP ? popup_rect_.x : 0;
  const int32_t y = type == PET_POPUP ? popup_rect_.y : 0;
  if (type == PET_VIEW && !QualifyResizeFrame(width, height)) {
    browser->GetHost()->Invalidate(PET_VIEW);
    return;
  }
  RectList paint_rects = dirtyRects;
  if (type == PET_VIEW &&
      (width != last_main_paint_width_ || height != last_main_paint_height_)) {
    paint_rects = {CefRect(0, 0, width, height)};
    last_main_paint_width_ = width;
    last_main_paint_height_ = height;
  }
  const bool sent = SendPaintBatch(surface, std::string(), x, y, buffer, width,
                                   height, paint_rects);
  if (type == PET_VIEW) {
    CompleteResizeFrame(width, height);
  }
  if (sent && type == PET_VIEW && pending_guest_cover_ && guests_.Covered()) {
    pending_guest_cover_ = false;
    for (GuestView* guest : guests_.InZOrder()) {
      if (guest->visible) {
        SendGuestHidden(*guest);
      }
    }
  }
}
