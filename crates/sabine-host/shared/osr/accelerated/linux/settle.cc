#include <algorithm>

#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "osr/handler.h"

// Stopgap until CEF hands Chromium's fence over with each accelerated frame
// (chromiumembedded/cef#4313). Without it, drivers that attach no implicit
// fence to the dma-buf, such as NVIDIA's, can have a frame copied while
// Chromium is still drawing it. Chromium also changes some pixels without
// reporting them as damage, such as the edges of icons and rounded corners
// when an animated layer comes or goes. Once a page stops painting, its last
// frame is copied again whole from the dma-buf it was drawn into, so the
// window settles on the page as drawn.

using namespace sabine_osr;

namespace {

constexpr std::chrono::milliseconds kSettleDelay{100};

}  // namespace

void SabineOsrHandler::SettleWhenPaused(const AcceleratedSurfaceKey& surface,
                                        CefRefPtr<CefBrowser> browser,
                                        const AcceleratedContent& content) {
  unsettled_frames_[surface] = {browser, content,
                                std::chrono::steady_clock::now()};
  if (!settle_pending_) {
    ScheduleSettle(kSettleDelay);
  }
}

void SabineOsrHandler::ScheduleSettle(std::chrono::milliseconds delay) {
  settle_pending_ = true;
  CefRefPtr<SabineOsrHandler> self(this);
  CefPostDelayedTask(TID_UI,
                     CefCreateClosureTask(base::BindOnce(
                         [](CefRefPtr<SabineOsrHandler> handler) {
                           handler->SettlePausedFrames();
                         },
                         self)),
                     delay.count());
}

void SabineOsrHandler::SettlePausedFrames() {
  settle_pending_ = false;
  const auto now = std::chrono::steady_clock::now();
  auto next = kSettleDelay;
  for (auto it = unsettled_frames_.begin(); it != unsettled_frames_.end();) {
    const auto paused = now - it->second.painted_at;
    if (paused < kSettleDelay) {
      next = std::min(next, std::chrono::ceil<std::chrono::milliseconds>(
                                kSettleDelay - paused));
      ++it;
      continue;
    }
    SettleFrame(it->first, it->second);
    it = unsettled_frames_.erase(it);
  }
  if (!unsettled_frames_.empty()) {
    ScheduleSettle(next);
  }
}

void SabineOsrHandler::SettleFrame(const AcceleratedSurfaceKey& surface,
                                   const UnsettledFrame& frame) {
  if (accelerated_paint_failed_) {
    return;
  }
  if (GuestView* guest = GuestForBrowser(frame.browser)) {
    if (!guest->visible) {
      return;
    }
  } else if (!FitsView(frame.content.visible.width,
                       frame.content.visible.height)) {
    return;
  }
  const std::optional<AcceleratedTarget> target =
      AcceleratedTargetFor(frame.browser, PET_VIEW);
  if (!target) {
    return;
  }
  AccelDmabufCopiedFrame copied_dmabuf{};
  const DmabufCopy result = CopyFinishedDmabufFrame(surface, &copied_dmabuf);
  CopiedAccelFrame copied{};
  if (ShareDmabufCopy(result, copied_dmabuf, &copied)) {
    SendAcceleratedFrame(*target, copied, frame.content);
  }
}
