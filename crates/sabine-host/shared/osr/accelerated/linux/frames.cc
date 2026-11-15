#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "osr/accelerated/linux/dmabuf_copy.h"
#include "osr/accelerated/protocol.h"
#include "osr/accelerated/share.h"
#include "osr/handler.h"

namespace sabine_osr {

namespace {

constexpr int64_t kReopenRetryMs = 250;

}  // namespace

bool AcceleratedPaint::Copy(const AcceleratedSurfaceKey& surface,
                            const PixelRegion& damage,
                            const CefAcceleratedPaintInfo& info,
                            CopiedFrame* out) {
  AccelDmabufCopiedFrame copied{};
  const DmabufCopy result =
      CopyAcceleratedDmabufFrame(surface, info, damage, &copied);
  if (result == DmabufCopy::kReopening) {
    RepaintAllSoon();
  }
  if (result == DmabufCopy::kFailed) {
    ReportUnavailable({AcceleratedDropKind::kCopyFailed});
  }
  return ShareDmabufCopy(result, copied, out);
}

bool AcceleratedPaint::ShareDmabufCopy(DmabufCopy result,
                                       const AccelDmabufCopiedFrame& copied,
                                       CopiedFrame* out) {
  RetireEverywhere(copied.retired_resource_ids, copied.device_reopened);
  if (result != DmabufCopy::kCopied) {
    return false;
  }
  return ShareCopiedSlot(
      copied,
      [&](uint64_t*) {
        const std::vector<char> payload = BuildDmabufAnnouncePayload(
            copied.resource_id, copied.modifier, copied.stride, copied.offset);
        return transport_.SendWithFd(
            kAccelDmabuf, copied.width, copied.height, 0, 0, payload.data(),
            static_cast<uint32_t>(payload.size()), copied.fd);
      },
      out);
}

void AcceleratedPaint::Discard(const CopiedFrame& frame) {
  ReleaseAcceleratedDmabufFrame(frame.slot_token);
}

void AcceleratedPaint::ReleaseSlot(uint64_t slot_token) {
  ReleaseAcceleratedDmabufFrame(slot_token);
}

void AcceleratedPaint::RetireSurfaces(int browser_id) {
  RetireResources(RetireAcceleratedDmabufBrowser(browser_id));
}

void AcceleratedPaint::SendRetire(const std::vector<uint64_t>& resource_ids) {
  const std::vector<char> payload = BuildAccelRetirePayload(resource_ids);
  transport_.Send(kAccelRetire, 0, 0, 0, 0, payload.data(),
                  static_cast<uint32_t>(payload.size()));
}

void AcceleratedPaint::RepaintAllSoon() {
  if (repaint_all_pending_) {
    return;
  }
  repaint_all_pending_ = true;
  CefPostDelayedTask(TID_UI,
                     CefCreateClosureTask(base::BindOnce(
                         [](CefRefPtr<SabineOsrHandler> owner) {
                           owner->accelerated_.repaint_all_pending_ = false;
                           owner->RepaintAll();
                         },
                         CefRefPtr<SabineOsrHandler>(owner_))),
                     kReopenRetryMs);
}

}  // namespace sabine_osr
