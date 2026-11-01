#include "osr/accelerated/macos/iosurface_copy.h"
#include "osr/common/registry.h"
#include "osr/handler.h"

using namespace sabine_osr;

namespace {

void RetireEverywhere(const std::vector<uint64_t>& resource_ids) {
  if (resource_ids.empty()) {
    return;
  }
  for (SabineOsrHandler* handler : RegisteredHandlers()) {
    handler->RetireAcceleratedResources(resource_ids);
  }
}

}  // namespace

void SabineOsrHandler::UseSurfaceService(const std::string& service_name) {
  if (!service_name.empty()) {
    surface_broker_ =
        std::make_unique<SurfaceBroker>(service_name, authentication_token_);
  }
}

bool SabineOsrHandler::CopyAcceleratedFrame(
    const AcceleratedSurfaceKey& surface,
    const PixelRegion& damage,
    const CefAcceleratedPaintInfo& info,
    CopiedAccelFrame* out) {
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
  if (!announced_accelerated_resources_.count(copied.resource_id)) {
    if (!surface_broker_->Announce(copied.resource_id, copied.surface)) {
      ReleaseAcceleratedIOSurfaceFrame(copied.slot_token);
      return false;
    }
    announced_accelerated_resources_.insert(copied.resource_id);
  }
  *out = {
      copied.width,     copied.height, copied.resource_id, copied.slot_index, 0,
      copied.slot_token};
  return true;
}

void SabineOsrHandler::DiscardAcceleratedFrame(const CopiedAccelFrame& frame) {
  ReleaseAcceleratedIOSurfaceFrame(frame.slot_token);
}

void SabineOsrHandler::ReleaseAcceleratedSlot(uint64_t slot_token) {
  ReleaseAcceleratedIOSurfaceFrame(slot_token);
}

void SabineOsrHandler::RetireAcceleratedSurfaces(int browser_id) {
  RetireAcceleratedResources(RetireAcceleratedIOSurfaceBrowser(browser_id));
}

void SabineOsrHandler::SendAcceleratedRetire(
    const std::vector<uint64_t>& resource_ids) {
  for (const uint64_t resource_id : resource_ids) {
    surface_broker_->Retire(resource_id);
  }
}
