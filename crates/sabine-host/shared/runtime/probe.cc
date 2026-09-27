#include "runtime/probe.h"

#include <cstdint>
#include <iostream>

#include "include/cef_app.h"
#include "include/base/cef_callback.h"
#include "include/cef_browser.h"
#include "include/cef_client.h"
#include "include/cef_render_handler.h"
#include "include/wrapper/cef_closure_task.h"

#if defined(OS_MAC)
#include <CoreVideo/CVPixelBuffer.h>
#include <IOSurface/IOSurfaceRef.h>

#include <cstdlib>

#include "osr/accelerated/macos/iosurface_copy.h"
#endif

namespace {
int result = 1;

#if defined(OS_MAC)
constexpr int kProbeTolerance = 24;

uint32_t ReadPixel(IOSurfaceRef surface, int x, int y) {
  IOSurfaceLock(surface, kIOSurfaceLockReadOnly, nullptr);
  const auto* row =
      static_cast<const uint8_t*>(IOSurfaceGetBaseAddress(surface)) +
      static_cast<size_t>(y) * IOSurfaceGetBytesPerRow(surface);
  const uint8_t* pixel = row + static_cast<size_t>(x) * 4;
  const bool bgra =
      IOSurfaceGetPixelFormat(surface) == kCVPixelFormatType_32BGRA;
  const uint32_t red = bgra ? pixel[2] : pixel[0];
  const uint32_t blue = bgra ? pixel[0] : pixel[2];
  const uint32_t argb = (static_cast<uint32_t>(pixel[3]) << 24) | (red << 16) |
                        (static_cast<uint32_t>(pixel[1]) << 8) | blue;
  IOSurfaceUnlock(surface, kIOSurfaceLockReadOnly, nullptr);
  return argb;
}

bool NearProbeColor(uint32_t argb) {
  const int channels[] = {static_cast<int>((argb >> 16) & 0xff) - 0x33,
                          static_cast<int>((argb >> 8) & 0xff) - 0x66,
                          static_cast<int>(argb & 0xff) - 0x99};
  for (const int delta : channels) {
    if (std::abs(delta) > kProbeTolerance) {
      return false;
    }
  }
  return (argb >> 24) == 0xff;
}
#endif

class RuntimeProbe : public CefClient,
                     public CefRenderHandler,
                     public CefLifeSpanHandler {
 public:
  CefRefPtr<CefRenderHandler> GetRenderHandler() override { return this; }
  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }

  void GetViewRect(CefRefPtr<CefBrowser>, CefRect& rect) override {
    rect = CefRect(0, 0, 64, 64);
  }

  void OnAfterCreated(CefRefPtr<CefBrowser> browser) override {
    browser_ = browser;
    CefRefPtr<RuntimeProbe> self(this);
    CefPostDelayedTask(
        TID_UI,
        CefCreateClosureTask(base::BindOnce(&RuntimeProbe::Timeout, self)),
        25000);
  }

  void OnPaint(CefRefPtr<CefBrowser> browser,
               PaintElementType type,
               const RectList&,
               const void* buffer,
               int width,
               int height) override {
    if (type != PET_VIEW || width != 64 || height != 64 || closing_)
      return;
    const auto* pixels = static_cast<const uint32_t*>(buffer);
    if (pixels[0] != 0xff336699 || pixels[32 * width + 32] != 0xff336699)
      return;
    result = 0;
    closing_ = true;
    browser->GetHost()->CloseBrowser(true);
  }

#if defined(OS_MAC)
  void OnAcceleratedPaint(CefRefPtr<CefBrowser> browser,
                          PaintElementType type,
                          const RectList&,
                          const CefAcceleratedPaintInfo& info) override {
    if (type != PET_VIEW || closing_) {
      return;
    }
    auto* source = static_cast<IOSurfaceRef>(info.shared_texture_io_surface);
    sabine_osr::AccelIOSurfaceCopiedFrame copied{};
    if (!sabine_osr::CopyAcceleratedIOSurfaceFrame(
            "probe", info.shared_texture_io_surface, &copied)) {
      std::cerr << "Chromium's shared paint surface could not be copied"
                << std::endl;
      closing_ = true;
      browser->GetHost()->CloseBrowser(true);
      return;
    }
    const int x = static_cast<int>(copied.width / 2);
    const int y = static_cast<int>(copied.height / 2);
    const uint32_t rendered = ReadPixel(source, x, y);
    const bool copied_exactly =
        ReadPixel(copied.surface, 0, 0) == ReadPixel(source, 0, 0) &&
        ReadPixel(copied.surface, x, y) == rendered;
    sabine_osr::ReleaseAcceleratedIOSurfaceFrame(copied.slot_token);
    if (!NearProbeColor(rendered)) {
      return;
    }
    if (!copied_exactly) {
      std::cerr << "Copied shared paint pixels differ from Chromium's frame"
                << std::endl;
      closing_ = true;
      browser->GetHost()->CloseBrowser(true);
      return;
    }
    result = 0;
    closing_ = true;
    browser->GetHost()->CloseBrowser(true);
  }
#endif

  void OnBeforeClose(CefRefPtr<CefBrowser>) override {
    browser_ = nullptr;
    CefQuitMessageLoop();
  }

 private:
  void Timeout() {
    if (!browser_ || closing_)
      return;
    std::cerr
        << "Chromium did not render its runtime probe page within 25 seconds"
        << std::endl;
    closing_ = true;
    browser_->GetHost()->CloseBrowser(true);
  }

  CefRefPtr<CefBrowser> browser_;
  bool closing_ = false;
  IMPLEMENT_REFCOUNTING(RuntimeProbe);
};
}  // namespace

void StartRuntimeProbe() {
  CefWindowInfo window;
  window.SetAsWindowless(0);
#if defined(OS_MAC)
  window.shared_texture_enabled = true;
#endif
  CefBrowserSettings settings;
  settings.windowless_frame_rate = 60;
  if (!CefBrowserHost::CreateBrowser(
          window, new RuntimeProbe(),
          "data:text/html,<html style='background:%23336699'></html>", settings,
          nullptr, nullptr)) {
    std::cerr << "Chromium could not create an off-screen browser" << std::endl;
    CefQuitMessageLoop();
  }
}

int RuntimeProbeResult() {
  return result;
}
