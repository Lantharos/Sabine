#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_LINUX_DMABUF_COPY_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_LINUX_DMABUF_COPY_H_

#include <cstdint>
#include <vector>

#include "include/internal/cef_types_wrappers.h"
#include "osr/accelerated/damage.h"

namespace sabine_osr {

struct AccelDmabufCopiedFrame {
  // Owned by the slot; valid until the resource is retired.
  int fd = -1;
  uint64_t resource_id = 0;
  uint32_t slot_index = 0;
  uint64_t slot_token = 0;
  uint32_t width = 0;
  uint32_t height = 0;
  uint64_t modifier = 0;
  uint32_t stride = 0;
  uint32_t offset = 0;
  // Resources this call replaced, whose compositor imports can be dropped.
  std::vector<uint64_t> retired_resource_ids;
};

enum class DmabufCopy { kCopied, kSlotsBusy, kFailed };

bool AcceleratedDmabufAvailable();

// CEF returns its dma-bufs to Chromium's pool when OnAcceleratedPaint returns,
// so each frame is copied on the GPU into a Sabine-owned dma-buf that stays
// valid until the compositor releases its slot. Only the pixels that changed
// since the chosen slot last received a frame are copied.
DmabufCopy CopyAcceleratedDmabufFrame(const AcceleratedSurfaceKey& surface,
                                      const CefAcceleratedPaintInfo& info,
                                      const PixelRegion& damage,
                                      AccelDmabufCopiedFrame* out);
void ReleaseAcceleratedDmabufFrame(uint64_t slot_token);
std::vector<uint64_t> RetireAcceleratedDmabufBrowser(int browser_id);

}  // namespace sabine_osr

#endif
