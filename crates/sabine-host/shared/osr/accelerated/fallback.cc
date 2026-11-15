#include <cstdio>

#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "osr/accelerated/protocol.h"
#include "common/trace.h"
#include "osr/handler.h"

namespace sabine_osr {

namespace {

constexpr int64_t kAcceleratedPaintGraceMs = 2000;

}  // namespace

void AcceleratedPaint::Drop(AcceleratedDrop drop) {
  const bool changed = drop.kind != drop_.kind;
  drop_ = drop;
  if (changed && TraceEnabled()) {
    std::fprintf(stderr, "Sabine CEF: dropped an accelerated frame: %s\n",
                 drop.Describe().c_str());
    std::fflush(stderr);
  }
}

void AcceleratedPaint::Watch() {
  if (!enabled_ || shown_) {
    return;
  }
  CefPostDelayedTask(TID_UI,
                     CefCreateClosureTask(base::BindOnce(
                         [](CefRefPtr<SabineOsrHandler> owner) {
                           if (!owner->accelerated_.shown_ &&
                               !owner->view_hidden_ && owner->browser_) {
                             owner->accelerated_.ReportMissing();
                           }
                         },
                         CefRefPtr<SabineOsrHandler>(owner_))),
                     kAcceleratedPaintGraceMs);
}

void AcceleratedPaint::ReportMissing() {
  std::fprintf(
      stderr,
      "Sabine CEF: no accelerated frame reached the window %lld ms "
      "after loading: %s; the window needs a %dx%d page at scale %.2f\n",
      static_cast<long long>(kAcceleratedPaintGraceMs),
      drop_.Describe().c_str(), owner_->width_, owner_->height_,
      owner_->scale_);
  std::fflush(stderr);
  // A loaded page that still has no shared frame won't get one, such as when
  // Chromium can't allocate the buffers it shares frames in, so the window
  // takes software frames instead of waiting.
  if (!failed_) {
    failed_ = true;
    transport_.Send(kAccelUnavailable, 0, 0, 0, 0, nullptr, 0);
  }
}

void AcceleratedPaint::ReportUnavailable(AcceleratedDrop reason) {
  drop_ = reason;
  if (failed_) {
    return;
  }
  failed_ = true;
  std::fprintf(stderr,
               "Sabine CEF: no accelerated frame reached the window: %s\n",
               drop_.Describe().c_str());
  std::fflush(stderr);
  transport_.Send(kAccelUnavailable, 0, 0, 0, 0, nullptr, 0);
}

}  // namespace sabine_osr
