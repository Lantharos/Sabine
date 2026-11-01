#include "osr/transport/wire.h"

#include <algorithm>
#include <cstring>

#include "osr/transport/message_kinds.h"

namespace sabine_osr {

namespace {

constexpr size_t kBatchEntryLen = 28;

void PutPaintEntry(std::vector<char>* payload,
                   size_t offset,
                   const PaintRectBytes& rect) {
  PutI32(payload, offset + 0, rect.x);
  PutI32(payload, offset + 4, rect.y);
  PutU32(payload, offset + 8, static_cast<uint32_t>(rect.width));
  PutU32(payload, offset + 12, static_cast<uint32_t>(rect.height));
  PutU64(payload, offset + 16, rect.offset);
  PutU32(payload, offset + 24, rect.len);
}

constexpr size_t kSharedSlotLen = 16;

}  // namespace

size_t PutGuestPrefix(std::vector<char>* buffer, const std::string& guest_id) {
  if (guest_id.empty()) {
    return 0;
  }
  const size_t len = guest_id.size();
  buffer->resize(std::max(buffer->size(), 2 + len));
  (*buffer)[0] = static_cast<char>(len & 0xff);
  (*buffer)[1] = static_cast<char>((len >> 8) & 0xff);
  std::memcpy(buffer->data() + 2, guest_id.data(), len);
  return 2 + len;
}

void PaintMetadata(const std::string& guest_id,
                   const std::vector<PaintRectBytes>& rects,
                   const SharedPaintRef* shared,
                   std::vector<char>* out) {
  const size_t prefix_len = guest_id.empty() ? 0 : 2 + guest_id.size();
  const size_t slot_len = shared ? kSharedSlotLen : 0;
  const size_t entries_start = prefix_len + slot_len + 4;
  out->resize(entries_start + rects.size() * kBatchEntryLen);
  PutGuestPrefix(out, guest_id);
  if (shared) {
    PutU32(out, prefix_len, shared->slot);
    PutU32(out, prefix_len + 4, shared->generation);
    PutU64(out, prefix_len + 8, shared->handle);
  }
  PutU32(out, prefix_len + slot_len, static_cast<uint32_t>(rects.size()));
  for (size_t i = 0; i < rects.size(); ++i) {
    PutPaintEntry(out, entries_start + i * kBatchEntryLen, rects[i]);
  }
}

void PutU32(std::vector<char>* buffer, size_t offset, uint32_t value) {
  (*buffer)[offset + 0] = static_cast<char>(value & 0xff);
  (*buffer)[offset + 1] = static_cast<char>((value >> 8) & 0xff);
  (*buffer)[offset + 2] = static_cast<char>((value >> 16) & 0xff);
  (*buffer)[offset + 3] = static_cast<char>((value >> 24) & 0xff);
}

void PutI32(std::vector<char>* buffer, size_t offset, int32_t value) {
  PutU32(buffer, offset, static_cast<uint32_t>(value));
}

void PutU64(std::vector<char>* buffer, size_t offset, uint64_t value) {
  for (size_t i = 0; i < 8; ++i) {
    (*buffer)[offset + i] = static_cast<char>((value >> (i * 8)) & 0xff);
  }
}

void CopyPaintRect(char* destination,
                   const void* buffer,
                   int buffer_width,
                   const PaintRectBytes& rect) {
  const char* source = static_cast<const char*>(buffer);
  const int source_stride = buffer_width * 4;
  const int row_bytes = rect.width * 4;
  for (int row = 0; row < rect.height; ++row) {
    std::memcpy(
        destination + rect.offset + static_cast<size_t>(row * row_bytes),
        source + (rect.y + row) * source_stride + rect.x * 4, row_bytes);
  }
}

uint32_t BatchKind(PaintSurface surface) {
  switch (surface) {
    case PaintSurface::kMain:
      return kMainBatch;
    case PaintSurface::kPopup:
      return kPopupBatch;
    case PaintSurface::kGuest:
      return kGuestBatch;
  }
  return kMainBatch;
}

uint32_t SharedBatchKind(PaintSurface surface) {
  switch (surface) {
    case PaintSurface::kMain:
      return kMainSharedBatch;
    case PaintSurface::kPopup:
      return kPopupSharedBatch;
    case PaintSurface::kGuest:
      return kGuestSharedBatch;
  }
  return kMainSharedBatch;
}

}  // namespace sabine_osr
