// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Every handle a frame names must already be open in the window host that
// owns the browser, so each texture is duplicated into that process once, and
// closed there again when the frame carrying it never reaches it.

#include <windows.h>

#include "osr/accelerated/protocol.h"
#include "osr/accelerated/share.h"
#include "osr/accelerated/windows/d3d11_copy.h"

using namespace sabine_osr;

void SabineOsrHandler::UseParentProcess(int pid) {
  parent_process_.Open(pid);
}

bool SabineOsrHandler::CopyAcceleratedFrame(
    const AcceleratedSurfaceKey& surface,
    const PixelRegion& damage,
    const CefAcceleratedPaintInfo& info,
    CopiedAccelFrame* out) {
  AccelD3d11CopiedFrame copied{};
  const bool copied_frame = CopyAcceleratedD3d11Frame(
      surface, info.shared_texture_handle, damage, &copied);
  RetireEverywhere(copied.retired_resource_ids);
  if (!copied_frame) {
    return false;
  }
  return ShareCopiedSlot(
      copied,
      [&](uint64_t* shared_handle) {
        *shared_handle = parent_process_.Duplicate(copied.shared_handle);
        return *shared_handle != 0;
      },
      out);
}

void SabineOsrHandler::DiscardAcceleratedFrame(const CopiedAccelFrame& frame) {
  if (frame.shared_handle != 0) {
    announced_accelerated_resources_.erase(frame.resource_id);
    parent_process_.Close(frame.shared_handle);
  }
  ReleaseAcceleratedD3d11Frame(frame.slot_token);
}

void SabineOsrHandler::ReleaseAcceleratedSlot(uint64_t slot_token) {
  ReleaseAcceleratedD3d11Frame(slot_token);
}

void SabineOsrHandler::RetireAcceleratedSurfaces(int browser_id) {
  RetireAcceleratedResources(RetireAcceleratedD3d11Browser(browser_id));
}

void SabineOsrHandler::SendAcceleratedRetire(
    const std::vector<uint64_t>& resource_ids) {
  const std::vector<char> payload = BuildAccelRetirePayload(resource_ids);
  SendMessage(kAccelRetire, 0, 0, 0, 0, payload.data(),
              static_cast<uint32_t>(payload.size()));
}
