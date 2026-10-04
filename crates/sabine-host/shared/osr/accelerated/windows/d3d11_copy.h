#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_WINDOWS_D3D11_COPY_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_WINDOWS_D3D11_COPY_H_

#include <cstdint>
#include <vector>
#include <windows.h>

#include "osr/accelerated/damage.h"

namespace sabine_osr {

struct AccelD3d11CopiedFrame {
  HANDLE shared_handle = nullptr;
  uint64_t resource_id = 0;
  uint32_t slot_index = 0;
  uint64_t slot_token = 0;
  uint32_t width = 0;
  uint32_t height = 0;
  /// Resources this call replaced, whose compositor handles can be closed.
  std::vector<uint64_t> retired_resource_ids;
};

/// Copy CEF's pooled shared texture into a Sabine-owned shared texture before
/// `OnAcceleratedPaint` returns. CEF recycles the source handle when the
/// callback returns; the compositor must only ever receive handles to our copy.
/// Only the pixels that changed since the chosen slot last received a frame
/// are copied.
bool CopyAcceleratedD3d11Frame(const AcceleratedSurfaceKey& surface,
                               HANDLE cef_shared_handle,
                               const PixelRegion& damage,
                               uint32_t cef_format,
                               AccelD3d11CopiedFrame* out);

/// Allow a copied texture slot to be reused after the compositor stops sampling
/// it.
void ReleaseAcceleratedD3d11Frame(uint64_t slot_token);
std::vector<uint64_t> RetireAcceleratedD3d11Browser(int browser_id);

}  // namespace sabine_osr

#endif
