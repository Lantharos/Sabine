#ifndef SABINE_CEF_HOST_OSR_TRANSPORT_WIRE_H_
#define SABINE_CEF_HOST_OSR_TRANSPORT_WIRE_H_

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

enum class PaintSurface { kMain, kPopup, kGuest };

namespace sabine_osr {

constexpr size_t kSharedPaintThreshold = 256 * 1024;

struct PaintRectBytes {
  int x = 0;
  int y = 0;
  int width = 0;
  int height = 0;
  uint64_t offset = 0;
  uint32_t len = 0;
};

// The shared-memory slot a paint's pixels were written to. `handle` is the
// slot's mapping handle in the window's process on Windows the first time the
// slot is sent, and 0 everywhere else.
struct SharedPaintRef {
  uint32_t slot = 0;
  uint32_t generation = 0;
  uint64_t handle = 0;
};

// Buffers a paint is assembled in, kept from paint to paint.
struct PaintScratch {
  std::vector<PaintRectBytes> rects;
  std::vector<char> bytes;
};

void PutU32(std::vector<char>* buffer, size_t offset, uint32_t value);
void PutI32(std::vector<char>* buffer, size_t offset, int32_t value);
void PutU64(std::vector<char>* buffer, size_t offset, uint64_t value);
// Writes the length-prefixed guest id that precedes every guest payload, or
// nothing for the page and its popup. Returns where the rest begins.
size_t PutGuestPrefix(std::vector<char>* buffer, const std::string& guest_id);
// Writes the batch metadata into `out`, followed by the shared slot when the
// pixels travel in shared memory.
void PaintMetadata(const std::string& guest_id,
                   const std::vector<PaintRectBytes>& rects,
                   const SharedPaintRef* shared,
                   std::vector<char>* out);
void CopyPaintRect(char* destination,
                   const void* buffer,
                   int buffer_width,
                   const PaintRectBytes& rect);
uint32_t BatchKind(PaintSurface surface);
uint32_t SharedBatchKind(PaintSurface surface);

}  // namespace sabine_osr

#endif
