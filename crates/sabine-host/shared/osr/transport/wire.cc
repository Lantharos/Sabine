#include "osr/transport/wire.h"

#include <cstring>

#ifdef _WIN32
#include <winsock2.h>
#else
#include <sys/socket.h>
#endif

#include "osr/handler.h"

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

std::vector<char> BuildPaintMetadata(const std::string& prefix,
                                     const std::vector<PaintRectBytes>& rects,
                                     size_t slot_len) {
  const size_t entries_start = prefix.size() + slot_len + 4;
  std::vector<char> metadata(entries_start + rects.size() * kBatchEntryLen, 0);
  std::memcpy(metadata.data(), prefix.data(), prefix.size());
  PutU32(&metadata, prefix.size() + slot_len,
         static_cast<uint32_t>(rects.size()));
  for (size_t i = 0; i < rects.size(); ++i) {
    PutPaintEntry(&metadata, entries_start + i * kBatchEntryLen, rects[i]);
  }
  return metadata;
}

}  // namespace

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

bool SendAll(intptr_t fd, const char* bytes, size_t len) {
  size_t sent = 0;
  while (sent < len) {
    const int result = send(
#ifdef _WIN32
        static_cast<SOCKET>(fd),
#else
        static_cast<int>(fd),
#endif
        bytes + sent, static_cast<int>(len - sent),
#ifdef _WIN32
        0
#else
        MSG_NOSIGNAL
#endif
    );
    if (result <= 0) {
      return false;
    }
    sent += static_cast<size_t>(result);
  }
  return true;
}

std::vector<char> PaintMetadata(const std::string& prefix,
                                const std::vector<PaintRectBytes>& rects) {
  return BuildPaintMetadata(prefix, rects, 0);
}

std::vector<char> PaintMetadata(const std::string& prefix,
                                const std::vector<PaintRectBytes>& rects,
                                uint32_t slot,
                                uint32_t generation) {
  std::vector<char> metadata = BuildPaintMetadata(prefix, rects, 8);
  PutU32(&metadata, prefix.size(), slot);
  PutU32(&metadata, prefix.size() + 4, generation);
  return metadata;
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
