#include "osr/handler.h"

#include <algorithm>
#include <limits>

#include "guest/manager.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/transport/wire.h"

using namespace sabine_osr;

bool SabineOsrHandler::SendPaintBatch(PaintSurface surface,
                                      const std::string& guest_id,
                                      int32_t origin_x,
                                      int32_t origin_y,
                                      const void* buffer,
                                      int buffer_width,
                                      int buffer_height,
                                      const RectList& dirty_rects) {
  if (buffer_width <= 0 || buffer_height <= 0 || !buffer) {
    return false;
  }

  std::vector<CefRect> source_rects;
  if (dirty_rects.empty()) {
    source_rects.push_back(CefRect(0, 0, buffer_width, buffer_height));
  } else {
    source_rects.assign(dirty_rects.begin(), dirty_rects.end());
  }

  std::vector<PaintRectBytes> rects;
  uint64_t total_bytes = 0;
  for (const auto& rect : source_rects) {
    const int left = std::max(0, rect.x);
    const int top = std::max(0, rect.y);
    const int right = std::min(buffer_width, rect.x + rect.width);
    const int bottom = std::min(buffer_height, rect.y + rect.height);
    const int width = right - left;
    const int height = bottom - top;
    if (width <= 0 || height <= 0) {
      continue;
    }
    const uint64_t len = static_cast<uint64_t>(width) * height * 4;
    if (len > std::numeric_limits<uint32_t>::max()) {
      return false;
    }
    rects.push_back(PaintRectBytes{
        left,
        top,
        width,
        height,
        total_bytes,
        static_cast<uint32_t>(len),
    });
    total_bytes += len;
  }
  if (rects.empty()) {
    return true;
  }

  const std::string prefix = surface == PaintSurface::kGuest
                                 ? GuestPayloadPrefix(guest_id)
                                 : std::string();
#ifndef _WIN32
  if (total_bytes >= kSharedPaintThreshold) {
    const int index = shared_paint_.Acquire(static_cast<size_t>(total_bytes));
    if (index >= 0) {
      SharedPaintSlot& slot = shared_paint_.Slot(index);
      for (const auto& rect : rects) {
        CopyPaintRect(slot.data, buffer, buffer_width, rect);
      }
      std::vector<char> metadata = PaintMetadata(
          prefix, rects, static_cast<uint32_t>(index), slot.generation);
      const uint32_t shared_kind = SharedBatchKind(surface);
      const uint32_t metadata_len = static_cast<uint32_t>(metadata.size());
      const bool sent =
          slot.announced
              ? SendMessage(shared_kind, static_cast<uint32_t>(buffer_width),
                            static_cast<uint32_t>(buffer_height), origin_x,
                            origin_y, metadata.data(), metadata_len)
              : SendMessageWithFd(
                    shared_kind, static_cast<uint32_t>(buffer_width),
                    static_cast<uint32_t>(buffer_height), origin_x, origin_y,
                    metadata.data(), metadata_len, slot.fd);
      if (!sent) {
        shared_paint_.Release(static_cast<uint32_t>(index), slot.generation);
        return false;
      }
      slot.announced = true;
      return true;
    }
  }
#endif

  std::vector<char> payload = PaintMetadata(prefix, rects);
  const size_t metadata_len = payload.size();
  if (metadata_len + total_bytes > std::numeric_limits<uint32_t>::max()) {
    return false;
  }
  payload.resize(metadata_len + static_cast<size_t>(total_bytes));
  for (const auto& rect : rects) {
    CopyPaintRect(payload.data() + metadata_len, buffer, buffer_width, rect);
  }
  return SendMessage(BatchKind(surface), static_cast<uint32_t>(buffer_width),
                     static_cast<uint32_t>(buffer_height), origin_x, origin_y,
                     payload.data(), static_cast<uint32_t>(payload.size()));
}

#ifndef _WIN32
void SabineOsrHandler::ReleaseSharedPaint(uint32_t slot, uint32_t generation) {
  CEF_REQUIRE_UI_THREAD();
  shared_paint_.Release(slot, generation);
}
#endif
