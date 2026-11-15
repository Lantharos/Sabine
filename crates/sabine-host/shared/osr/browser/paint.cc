#include "osr/handler.h"

#include <cmath>
#include <cstdio>
#include <string>

#include "common/trace.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/accelerated/protocol.h"

using namespace sabine_osr;

namespace {

// Chromium sizes the view's pixels with gfx::ScaleToCeiledSize, rounding up
// in single precision.
int ViewPixels(int dips, float scale) {
  const float pixels = static_cast<float>(dips) * scale;
  return static_cast<int>(std::ceil(pixels));
}

}  // namespace

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
  if (GuestView* guest = guests_.ForBrowser(browser)) {
    if (type == PET_POPUP) {
      const CefRect popup = guests_.PopupInView(*guest);
      software_paint_.Send({browser->GetIdentifier(), true},
                           PaintSurface::kGuest, guest->id + "/popup", popup.x,
                           popup.y, buffer, width, height, dirtyRects);
      return;
    }
    guests_.MarkPainted(*guest);
    if (guest->visible) {
      guests_.SendPaint(*guest, buffer, width, height, dirtyRects);
    }
    return;
  }
  if (view_hidden_ || !browser_ || !browser_->IsSame(browser)) {
    return;
  }
  // Chromium hands over shared textures only while it composites on the GPU.
  // Software frames mean its GPU process is gone or was never usable, so no
  // shared frame will come and the window has to take these instead.
  if (accelerated_.enabled() && type == PET_VIEW) {
    accelerated_.ReportUnavailable({AcceleratedDropKind::kSoftwareFrames});
  }
  const PaintSurface surface =
      type == PET_POPUP ? PaintSurface::kPopup : PaintSurface::kMain;
  const int32_t x = type == PET_POPUP ? popup_rect_.x : 0;
  const int32_t y = type == PET_POPUP ? popup_rect_.y : 0;
  if (type == PET_VIEW && !QualifyResizeFrame(width, height)) {
    browser->GetHost()->Invalidate(PET_VIEW);
    return;
  }
  static const RectList kWholeFrame;
  const bool resized = type == PET_VIEW && (width != last_main_paint_width_ ||
                                            height != last_main_paint_height_);
  if (resized) {
    last_main_paint_width_ = width;
    last_main_paint_height_ = height;
  }
  const bool sent = software_paint_.Send(
      {browser->GetIdentifier(), type == PET_POPUP}, surface, std::string(), x,
      y, buffer, width, height, resized ? kWholeFrame : dirtyRects);
  if (type == PET_VIEW) {
    CompleteResizeFrame(width, height);
  }
  if (sent && type == PET_VIEW) {
    guests_.HideCovered();
  }
}

void SabineOsrHandler::OnAcceleratedPaint(CefRefPtr<CefBrowser> browser,
                                          PaintElementType type,
                                          const RectList& dirtyRects,
                                          const CefAcceleratedPaintInfo& info) {
  if (!browser || accelerated_.failed()) {
    return;
  }
  const std::optional<CefRect> visible = accelerated_.Accept(info);
  if (!visible) {
    return;
  }
  if (type == PET_VIEW &&
      !QualifyResizeFrame(visible->width, visible->height)) {
    accelerated_.Drop(
        {AcceleratedDropKind::kWrongSize, visible->width, visible->height});
    browser->GetHost()->Invalidate(PET_VIEW);
    return;
  }
  const std::optional<AcceleratedTarget> target =
      AcceleratedTargetFor(browser, type);
  if (!target) {
    return;
  }
  const AcceleratedPaint::Presented presented =
      accelerated_.Present(browser, type, dirtyRects, info, *target, *visible);
  if (presented == AcceleratedPaint::Presented::kDropped) {
    return;
  }
  const bool shown =
      presented == AcceleratedPaint::Presented::kSent && type == PET_VIEW;
  if (GuestView* guest = guests_.ForBrowser(browser)) {
    if (shown) {
      guests_.MarkPainted(*guest);
    }
    return;
  }
  if (shown) {
    accelerated_.Shown();
  }
  if (type == PET_VIEW) {
    CompleteResizeFrame(visible->width, visible->height);
  }
}

std::optional<AcceleratedTarget> SabineOsrHandler::AcceleratedTargetFor(
    CefRefPtr<CefBrowser> browser,
    PaintElementType type) {
  if (GuestView* guest = guests_.ForBrowser(browser)) {
    if (type == PET_POPUP) {
      const CefRect popup = guests_.PopupInView(*guest);
      return AcceleratedTarget{kGuestAccel, guest->id + "/popup", popup.x,
                               popup.y};
    }
    const CefRect view = guest->ViewBounds();
    return AcceleratedTarget{kGuestAccel, guest->id, view.x, view.y};
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

bool SabineOsrHandler::FitsView(int pixel_width, int pixel_height) const {
  return pixel_width == ViewPixels(width_, scale_) &&
         pixel_height == ViewPixels(height_, scale_);
}

bool SabineOsrHandler::QualifyResizeFrame(int pixel_width, int pixel_height) {
  CEF_REQUIRE_UI_THREAD();
  return !transport_.ResizeInFlight() || FitsView(pixel_width, pixel_height);
}

void SabineOsrHandler::CompleteResizeFrame(int pixel_width, int pixel_height) {
  CEF_REQUIRE_UI_THREAD();
  if (!FitsView(pixel_width, pixel_height)) {
    return;
  }
  transport_.ResizeFramePainted();
}

void SabineOsrHandler::ReleaseSharedPaint(uint32_t slot, uint32_t generation) {
  CEF_REQUIRE_UI_THREAD();
  software_paint_.Release(slot, generation);
  dropped_paints_.Repaint(browsers_);
}

void SabineOsrHandler::ReleaseAcceleratedFrame(uint64_t slot_token) {
  accelerated_.ReleaseSlot(slot_token);
  dropped_paints_.Repaint(browsers_);
}
