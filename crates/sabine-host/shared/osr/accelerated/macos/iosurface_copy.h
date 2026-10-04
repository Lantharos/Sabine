#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_MACOS_IOSURFACE_COPY_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_MACOS_IOSURFACE_COPY_H_

#include <IOSurface/IOSurfaceRef.h>

#include <cstdint>
#include <vector>

#include "osr/accelerated/damage.h"

namespace sabine_osr {

struct AccelIOSurfaceCopiedFrame {
  IOSurfaceRef surface = nullptr;
  uint64_t surface_id = 0;
  uint32_t slot_index = 0;
  uint64_t slot_token = 0;
  uint32_t width = 0;
  uint32_t height = 0;
  // Surfaces this call replaced, whose compositor references can be dropped.
  std::vector<uint64_t> retired_surface_ids;
};

// CEF recycles its IOSurface as soon as OnAcceleratedPaint returns, so each
// frame is copied on the GPU into a Sabine-owned surface that stays valid until
// the compositor releases its slot. Only the pixels that changed since the
// chosen slot last received a frame are copied.
bool CopyAcceleratedIOSurfaceFrame(const AcceleratedSurfaceKey& surface,
                                   void* cef_io_surface,
                                   const PixelRegion& damage,
                                   AccelIOSurfaceCopiedFrame* out);
void ReleaseAcceleratedIOSurfaceFrame(uint64_t slot_token);
std::vector<uint64_t> RetireAcceleratedIOSurfaceBrowser(int browser_id);

}  // namespace sabine_osr

#endif
