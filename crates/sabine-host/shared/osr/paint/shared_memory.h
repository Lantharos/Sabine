#ifndef SABINE_CEF_HOST_OSR_PAINT_SHARED_MEMORY_H_
#define SABINE_CEF_HOST_OSR_PAINT_SHARED_MEMORY_H_

#include <cstddef>

namespace sabine_osr {

// Anonymous memory this process maps read-write and the window maps
// read-only: a file descriptor on Linux and macOS, a file mapping handle on
// Windows.
struct SharedMemory {
#ifdef _WIN32
  void* mapping = nullptr;
#else
  int fd = -1;
#endif
  char* data = nullptr;
  size_t capacity = 0;
};

bool CreateSharedMemory(size_t capacity, SharedMemory* out);
void ReleaseSharedMemory(SharedMemory* memory);

}  // namespace sabine_osr

#endif
