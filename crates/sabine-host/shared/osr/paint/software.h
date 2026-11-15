#ifndef SABINE_CEF_HOST_OSR_PAINT_SOFTWARE_H_
#define SABINE_CEF_HOST_OSR_PAINT_SOFTWARE_H_

#include <cstddef>
#include <cstdint>
#include <string>

#include "include/cef_render_handler.h"
#include "osr/accelerated/damage.h"
#include "osr/paint/dropped.h"
#include "osr/paint/shared_pool.h"
#include "osr/transport/transport.h"
#include "osr/transport/wire.h"

namespace sabine_osr {

// The window's software frames, sent as the rects Chromium redrew.
class SoftwarePaint {
 public:
  SoftwarePaint(Transport& transport, DroppedPaints& dropped)
      : transport_(transport), dropped_(dropped) {}

  // Paints too large for the socket go in shared memory; while every slot
  // is in flight they are dropped and `source` painted again once one frees.
  bool Send(const AcceleratedSurfaceKey& source,
            PaintSurface surface,
            const std::string& guest_id,
            int32_t origin_x,
            int32_t origin_y,
            const void* buffer,
            int buffer_width,
            int buffer_height,
            const CefRenderHandler::RectList& dirty_rects);
  void Release(uint32_t slot, uint32_t generation) {
    shared_.Release(slot, generation);
  }

 private:
  bool SendShared(PaintSurface surface,
                  const std::string& guest_id,
                  int32_t origin_x,
                  int32_t origin_y,
                  const void* buffer,
                  int buffer_width,
                  int buffer_height,
                  size_t total_bytes);

  Transport& transport_;
  DroppedPaints& dropped_;
  SharedPaintPool shared_;
  PaintScratch scratch_;
};

}  // namespace sabine_osr

#endif
