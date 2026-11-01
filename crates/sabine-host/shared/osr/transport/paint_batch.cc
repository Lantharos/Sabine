#include "osr/handler.h"

#include <algorithm>
#include <limits>

#include "osr/guest/registry.h"
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

  std::vector<PaintRectBytes>& rects = paint_scratch_.rects;
  rects.clear();
  uint64_t total_bytes = 0;
  const auto add = [&](const CefRect& rect) {
    const int left = std::max(0, rect.x);
    const int top = std::max(0, rect.y);
    const int right = std::min(buffer_width, rect.x + rect.width);
    const int bottom = std::min(buffer_height, rect.y + rect.height);
    const int width = right - left;
    const int height = bottom - top;
    if (width <= 0 || height <= 0) {
      return;
    }
    const uint64_t len = static_cast<uint64_t>(width) * height * 4;
    rects.push_back(PaintRectBytes{left, top, width, height, total_bytes,
                                   static_cast<uint32_t>(len)});
    total_bytes += len;
  };
  if (dirty_rects.empty()) {
    add(CefRect(0, 0, buffer_width, buffer_height));
  }
  for (const CefRect& rect : dirty_rects) {
    add(rect);
  }
  if (rects.empty()) {
    return true;
  }
  if (total_bytes >= kSharedPaintThreshold &&
      SendSharedPaint(surface, guest_id, origin_x, origin_y, buffer,
                      buffer_width, buffer_height,
                      static_cast<size_t>(total_bytes))) {
    return true;
  }

  std::vector<char>& payload = paint_scratch_.bytes;
  PaintMetadata(guest_id, rects, nullptr, &payload);
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

// Large paints travel in shared memory, which the window maps once per slot:
// on Linux and macOS its file descriptor goes along with the slot's first
// paint, on Windows its handle is duplicated into the window's process.
bool SabineOsrHandler::SendSharedPaint(PaintSurface surface,
                                       const std::string& guest_id,
                                       int32_t origin_x,
                                       int32_t origin_y,
                                       const void* buffer,
                                       int buffer_width,
                                       int buffer_height,
                                       size_t total_bytes) {
  const int index = shared_paint_.Acquire(total_bytes);
  if (index < 0) {
    return false;
  }
  SharedPaintSlot& slot = shared_paint_.Slot(index);
  for (const auto& rect : paint_scratch_.rects) {
    CopyPaintRect(slot.memory.data, buffer, buffer_width, rect);
  }
  SharedPaintRef shared{static_cast<uint32_t>(index), slot.generation, 0};
#ifdef _WIN32
  if (!slot.announced) {
    shared.handle =
        parent_process_.Duplicate(slot.memory.mapping, FILE_MAP_READ);
    if (shared.handle == 0) {
      shared_paint_.Release(shared.slot, shared.generation);
      return false;
    }
  }
#endif
  std::vector<char>& metadata = paint_scratch_.bytes;
  PaintMetadata(guest_id, paint_scratch_.rects, &shared, &metadata);
  const uint32_t kind = SharedBatchKind(surface);
  const uint32_t width = static_cast<uint32_t>(buffer_width);
  const uint32_t height = static_cast<uint32_t>(buffer_height);
  const uint32_t metadata_len = static_cast<uint32_t>(metadata.size());
#ifdef _WIN32
  const bool sent = SendMessage(kind, width, height, origin_x, origin_y,
                                metadata.data(), metadata_len);
  if (!sent && shared.handle != 0) {
    parent_process_.Close(shared.handle);
  }
#else
  const bool sent =
      slot.announced
          ? SendMessage(kind, width, height, origin_x, origin_y,
                        metadata.data(), metadata_len)
          : SendMessageWithFd(kind, width, height, origin_x, origin_y,
                              metadata.data(), metadata_len, slot.memory.fd);
#endif
  if (!sent) {
    shared_paint_.Release(shared.slot, shared.generation);
    return false;
  }
  slot.announced = true;
  return true;
}

void SabineOsrHandler::ReleaseSharedPaint(uint32_t slot, uint32_t generation) {
  CEF_REQUIRE_UI_THREAD();
  shared_paint_.Release(slot, generation);
}

void SabineOsrHandler::SendGuestOrder() {
  std::vector<char>& payload = paint_scratch_.bytes;
  payload.clear();
  for (const GuestView* guest : guests_.InZOrder()) {
    payload.insert(payload.end(), guest->id.begin(), guest->id.end());
    payload.push_back('\0');
  }
  SendMessage(kGuestOrder, 0, 0, 0, 0, payload.data(),
              static_cast<uint32_t>(payload.size()));
}
