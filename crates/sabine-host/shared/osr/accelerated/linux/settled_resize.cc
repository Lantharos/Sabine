#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "osr/handler.h"

// Stopgap until CEF hands Chromium's fence over with each accelerated frame
// (chromiumembedded/cef#4313). Without it, drivers that attach no implicit
// fence to the dma-buf can be copied from mid-draw right after a resize.

namespace {

constexpr int64_t kResizeSettleMs = 100;

}  // namespace

void SabineOsrHandler::RepaintWholeOnceResizeSettles() {
  if (!accelerated_paint_ || accelerated_paint_failed_) {
    return;
  }
  CefRefPtr<SabineOsrHandler> self(this);
  CefPostDelayedTask(
      TID_UI,
      CefCreateClosureTask(base::BindOnce(
          [](CefRefPtr<SabineOsrHandler> handler, uint64_t generation) {
            if (generation != handler->resize_generation_ ||
                !handler->browser_) {
              return;
            }
            handler->settled_resize_frame_pending_ = true;
            handler->browser_->GetHost()->Invalidate(PET_VIEW);
          },
          self, ++resize_generation_)),
      kResizeSettleMs);
}

bool SabineOsrHandler::TakeSettledResizeFrame(CefRefPtr<CefBrowser> browser,
                                              PaintElementType type) {
  if (type != PET_VIEW || !settled_resize_frame_pending_ || view_hidden_ ||
      !browser_ || !browser_->IsSame(browser)) {
    return false;
  }
  settled_resize_frame_pending_ = false;
  return true;
}
