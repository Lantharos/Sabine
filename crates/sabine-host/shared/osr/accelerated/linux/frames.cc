#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "osr/accelerated/linux/dmabuf_copy.h"
#include "osr/accelerated/protocol.h"
#include "osr/accelerated/share.h"

using namespace sabine_osr;

namespace {

constexpr int64_t kReopenRetryMs = 250;

}  // namespace

bool SabineOsrHandler::CopyAcceleratedFrame(
    const AcceleratedSurfaceKey& surface,
    const PixelRegion& damage,
    const CefAcceleratedPaintInfo& info,
    CopiedAccelFrame* out) {
  AccelDmabufCopiedFrame copied{};
  const DmabufCopy result =
      CopyAcceleratedDmabufFrame(surface, info, damage, &copied);
  if (result == DmabufCopy::kReopening) {
    RepaintAllSoon();
  }
  if (result == DmabufCopy::kFailed) {
    accelerated_drop_ = {AcceleratedDropKind::kCopyFailed};
    ReportAcceleratedPaintUnavailable();
  }
  return ShareDmabufCopy(result, copied, out);
}

bool SabineOsrHandler::ShareDmabufCopy(DmabufCopy result,
                                       const AccelDmabufCopiedFrame& copied,
                                       CopiedAccelFrame* out) {
  RetireEverywhere(copied.retired_resource_ids, copied.device_reopened);
  if (result != DmabufCopy::kCopied) {
    return false;
  }
  return ShareCopiedSlot(
      copied,
      [&](uint64_t*) {
        const std::vector<char> payload = BuildDmabufAnnouncePayload(
            copied.resource_id, copied.modifier, copied.stride, copied.offset);
        return SendMessageWithFd(
            kAccelDmabuf, copied.width, copied.height, 0, 0, payload.data(),
            static_cast<uint32_t>(payload.size()), copied.fd);
      },
      out);
}

void SabineOsrHandler::DiscardAcceleratedFrame(const CopiedAccelFrame& frame) {
  ReleaseAcceleratedDmabufFrame(frame.slot_token);
}

void SabineOsrHandler::ReleaseAcceleratedSlot(uint64_t slot_token) {
  ReleaseAcceleratedDmabufFrame(slot_token);
}

void SabineOsrHandler::RetireAcceleratedSurfaces(int browser_id) {
  RetireAcceleratedResources(RetireAcceleratedDmabufBrowser(browser_id));
}

void SabineOsrHandler::SendAcceleratedRetire(
    const std::vector<uint64_t>& resource_ids) {
  const std::vector<char> payload = BuildAccelRetirePayload(resource_ids);
  SendMessage(kAccelRetire, 0, 0, 0, 0, payload.data(),
              static_cast<uint32_t>(payload.size()));
}

void SabineOsrHandler::RepaintAllSoon() {
  if (repaint_all_pending_) {
    return;
  }
  repaint_all_pending_ = true;
  CefRefPtr<SabineOsrHandler> self(this);
  CefPostDelayedTask(TID_UI,
                     CefCreateClosureTask(base::BindOnce(
                         [](CefRefPtr<SabineOsrHandler> handler) {
                           handler->repaint_all_pending_ = false;
                           handler->RepaintAll();
                         },
                         self)),
                     kReopenRetryMs);
}
