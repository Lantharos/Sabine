#include "osr/handler.h"

#include <cmath>
#include <string>
#include <vector>

#include "include/cef_parser.h"
#include "include/wrapper/cef_helpers.h"

using namespace sabine_osr;

namespace {

constexpr char kPageZoomCommand[] = "sabine.window.pageZoom";

}  // namespace

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
  transport_.Send(kCursor, image ? size.width : 0, image ? size.height : 0,
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
  transport_.Send(kDraggableRegionsChanged, 0, 0, 0, 0, payload.data(),
                  static_cast<uint32_t>(payload.size()));
}

void SabineOsrHandler::OnMediaAccessChange(CefRefPtr<CefBrowser> browser,
                                           bool has_video_access,
                                           bool has_audio_access) {
  CEF_REQUIRE_UI_THREAD();
  media_.SetCapturing(browser->GetIdentifier(),
                      has_video_access || has_audio_access);
}

bool SabineOsrHandler::OnTooltip(CefRefPtr<CefBrowser> browser,
                                 CefString& text) {
  CEF_REQUIRE_UI_THREAD();
  const std::string value = text;
  transport_.Send(kTooltipChanged, 0, 0, 0, 0, value.data(),
                  static_cast<uint32_t>(value.size()));
  return true;
}

// The window's regions and guests are placed in the page's CSS pixels, so the
// window follows the page's zoom, whichever way it changed.
void SabineOsrHandler::ReportPageZoom() {
  CEF_REQUIRE_UI_THREAD();
  const double zoom = std::pow(1.2, browser_->GetHost()->GetZoomLevel());
  if (zoom == page_zoom_)
    return;
  page_zoom_ = zoom;
  guests_.FollowPageZoom(zoom);
  CefRefPtr<CefValue> payload = CefValue::Create();
  CefRefPtr<CefDictionaryValue> fields = CefDictionaryValue::Create();
  fields->SetDouble("factor", zoom);
  payload->SetDictionary(fields);
  const std::string line =
      "SABINE_BRIDGE_REQUEST\t" + std::to_string(browser_->GetIdentifier()) +
      "\t\t\t" + kPageZoomCommand + "\t" +
      CefWriteJSON(payload, JSON_WRITER_DEFAULT).ToString();
  transport_.Send(kBridgeRequest, {line});
}

bool SabineOsrHandler::GetScreenInfo(CefRefPtr<CefBrowser> browser,
                                     CefScreenInfo& screen_info) {
  screen_info.device_scale_factor = scale_;
  screen_info.depth = 32;
  screen_info.depth_per_component = 8;
  if (const GuestView* guest = guests_.ForBrowser(browser)) {
    screen_info.device_scale_factor =
        static_cast<float>(scale_ * guest->page_zoom);
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
  if (const GuestView* guest = guests_.ForBrowser(browser)) {
    rect = CefRect(0, 0, guest->bounds.width, guest->bounds.height);
    return;
  }
  rect = CefRect(0, 0, width_, height_);
}

void SabineOsrHandler::OnPopupShow(CefRefPtr<CefBrowser> browser, bool show) {
  if (GuestView* guest = guests_.ForBrowser(browser)) {
    if (!show) {
      guests_.HidePopup(*guest);
    }
    return;
  }
  if (!show) {
    transport_.Send(kPopupHidden, 0, 0, 0, 0, nullptr, 0);
  }
}

void SabineOsrHandler::OnPopupSize(CefRefPtr<CefBrowser> browser,
                                   const CefRect& rect) {
  if (GuestView* guest = guests_.ForBrowser(browser)) {
    guests_.ResizePopup(*guest, rect);
    return;
  }
  if (popup_rect_.x != rect.x || popup_rect_.y != rect.y ||
      popup_rect_.width != rect.width || popup_rect_.height != rect.height) {
    transport_.Send(kPopupHidden, 0, 0, 0, 0, nullptr, 0);
  }
  popup_rect_ = rect;
}
