#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_WINDOWS_D3D11_COPY_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_WINDOWS_D3D11_COPY_H_

#include <cstdint>
#include <vector>
#include <windows.h>

#include "osr/accelerated/slot_ring.h"

namespace sabine_osr {

struct AccelD3d11CopiedFrame : CopiedSlot {
  HANDLE shared_handle = nullptr;
};

/// Copy CEF's pooled shared texture into a Sabine-owned shared texture before
/// `OnAcceleratedPaint` returns. CEF recycles the source handle when the
/// callback returns; the compositor must only ever receive handles to our copy.
/// Only the pixels that changed since the chosen slot last received a frame
/// are copied, and the copy has finished when this returns.
bool CopyAcceleratedD3d11Frame(const AcceleratedSurfaceKey& surface,
                               HANDLE cef_shared_handle,
                               const PixelRegion& damage,
                               AccelD3d11CopiedFrame* out);

/// Allow a copied texture slot to be reused after the compositor stops sampling
/// it.
void ReleaseAcceleratedD3d11Frame(uint64_t slot_token);
std::vector<uint64_t> RetireAcceleratedD3d11Browser(int browser_id);

}  // namespace sabine_osr

#endif
