#include "osr/accelerated/macos/iosurface_copy.h"
#include "osr/accelerated/share.h"

namespace sabine_osr {

void AcceleratedPaint::UseSurfaceService(const std::string& service_name) {
  if (!service_name.empty()) {
    surface_broker_ = std::make_unique<SurfaceBroker>(
        service_name, transport_.authentication_token());
  }
}

bool AcceleratedPaint::Copy(const AcceleratedSurfaceKey& surface,
                            const PixelRegion& damage,
                            const CefAcceleratedPaintInfo& info,
                            CopiedFrame* out) {
  if (!surface_broker_) {
    return false;
  }
  AccelIOSurfaceCopiedFrame copied{};
  const bool copied_frame = CopyAcceleratedIOSurfaceFrame(
      surface, info.shared_texture_io_surface, damage, &copied);
  RetireEverywhere(copied.retired_resource_ids);
  if (!copied_frame) {
    return false;
  }
  return ShareCopiedSlot(
      copied,
      [&](uint64_t*) {
        return surface_broker_->Announce(copied.resource_id, copied.surface);
      },
      out);
}

void AcceleratedPaint::Discard(const CopiedFrame& frame) {
  ReleaseAcceleratedIOSurfaceFrame(frame.slot_token);
}

void AcceleratedPaint::ReleaseSlot(uint64_t slot_token) {
  ReleaseAcceleratedIOSurfaceFrame(slot_token);
}

void AcceleratedPaint::RetireSurfaces(int browser_id) {
  RetireResources(RetireAcceleratedIOSurfaceBrowser(browser_id));
}

void AcceleratedPaint::SendRetire(const std::vector<uint64_t>& resource_ids) {
  for (const uint64_t resource_id : resource_ids) {
    surface_broker_->Retire(resource_id);
  }
}

}  // namespace sabine_osr
