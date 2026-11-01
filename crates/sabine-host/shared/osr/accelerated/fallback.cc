#include <cstdio>

#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "osr/accelerated/protocol.h"
#include "common/trace.h"
#include "osr/handler.h"

using namespace sabine_osr;

namespace {

constexpr int64_t kAcceleratedPaintGraceMs = 2000;

}  // namespace

void SabineOsrHandler::UseAcceleratedPaint(bool enabled) {
  accelerated_paint_ = enabled;
}

void SabineOsrHandler::DropAcceleratedFrame(AcceleratedDrop drop) {
  const bool changed = drop.kind != accelerated_drop_.kind;
  accelerated_drop_ = drop;
  if (changed && TraceEnabled()) {
    std::fprintf(stderr, "Sabine CEF: dropped an accelerated frame: %s\n",
                 drop.Describe().c_str());
    std::fflush(stderr);
  }
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
                             handler->ReportMissingAcceleratedPaint();
                           }
                         },
                         self)),
                     kAcceleratedPaintGraceMs);
}

void SabineOsrHandler::ReportMissingAcceleratedPaint() {
  std::fprintf(
      stderr,
      "Sabine CEF: no accelerated frame reached the window %lld ms "
      "after loading: %s; the window needs a %dx%d page at scale %.2f\n",
      static_cast<long long>(kAcceleratedPaintGraceMs),
      accelerated_drop_.Describe().c_str(), width_, height_, scale_);
  std::fflush(stderr);
  // A loaded page that still has no shared frame won't get one, such as when
  // Chromium can't allocate the buffers it shares frames in, so the window
  // takes software frames instead of waiting.
  if (!accelerated_paint_failed_) {
    accelerated_paint_failed_ = true;
    SendMessage(kAccelUnavailable, 0, 0, 0, 0, nullptr, 0);
  }
}

void SabineOsrHandler::ReportAcceleratedPaintUnavailable() {
  if (accelerated_paint_failed_) {
    return;
  }
  accelerated_paint_failed_ = true;
  std::fprintf(stderr,
               "Sabine CEF: no accelerated frame reached the window: %s\n",
               accelerated_drop_.Describe().c_str());
  std::fflush(stderr);
  SendMessage(kAccelUnavailable, 0, 0, 0, 0, nullptr, 0);
}
