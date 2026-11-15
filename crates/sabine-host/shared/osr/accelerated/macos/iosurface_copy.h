#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_MACOS_IOSURFACE_COPY_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_MACOS_IOSURFACE_COPY_H_

#include <IOSurface/IOSurfaceRef.h>

#include <cstdint>
#include <vector>

#include "osr/accelerated/slot_ring.h"

namespace sabine_osr {

struct AccelIOSurfaceCopiedFrame : CopiedSlot {
  IOSurfaceRef surface = nullptr;
};

// CEF recycles its IOSurface as soon as OnAcceleratedPaint returns, so each
// frame is copied on the GPU into a Sabine-owned surface that stays valid until
// the compositor releases its slot. Only the pixels that changed since the
// chosen slot last received a frame are copied, and the copy has finished
// when this returns.
bool CopyAcceleratedIOSurfaceFrame(const AcceleratedSurfaceKey& surface,
                                   void* cef_io_surface,
                                   const PixelRegion& damage,
                                   AccelIOSurfaceCopiedFrame* out);
void ReleaseAcceleratedIOSurfaceFrame(uint64_t slot_token);
std::vector<uint64_t> RetireAcceleratedIOSurfaceBrowser(int browser_id);

}  // namespace sabine_osr

#endif
