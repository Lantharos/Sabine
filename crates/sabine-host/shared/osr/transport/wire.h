#ifndef SABINE_CEF_HOST_OSR_TRANSPORT_WIRE_H_
#define SABINE_CEF_HOST_OSR_TRANSPORT_WIRE_H_

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

enum class PaintSurface;

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

void PutU32(std::vector<char>* buffer, size_t offset, uint32_t value);
void PutI32(std::vector<char>* buffer, size_t offset, int32_t value);
void PutU64(std::vector<char>* buffer, size_t offset, uint64_t value);
bool SendAll(intptr_t fd, const char* bytes, size_t len);
std::vector<char> PaintMetadata(const std::string& prefix,
                                const std::vector<PaintRectBytes>& rects);
std::vector<char> PaintMetadata(const std::string& prefix,
                                const std::vector<PaintRectBytes>& rects,
                                uint32_t slot,
                                uint32_t generation);
void CopyPaintRect(char* destination,
                   const void* buffer,
                   int buffer_width,
                   const PaintRectBytes& rect);
uint32_t BatchKind(PaintSurface surface);
uint32_t SharedBatchKind(PaintSurface surface);

}  // namespace sabine_osr

#endif
