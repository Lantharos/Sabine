#include "osr/paint/shared_memory.h"

#include <windows.h>

#include <cstdint>

namespace sabine_osr {

bool CreateSharedMemory(size_t capacity, SharedMemory* out) {
  const uint64_t size = capacity;
  HANDLE mapping = CreateFileMappingW(
      INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE,
      static_cast<DWORD>(size >> 32), static_cast<DWORD>(size), nullptr);
  if (!mapping) {
    return false;
  }
  void* data = MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, capacity);
  if (!data) {
    CloseHandle(mapping);
    return false;
  }
  out->mapping = mapping;
  out->data = static_cast<char*>(data);
  out->capacity = capacity;
  return true;
}

void ReleaseSharedMemory(SharedMemory* memory) {
  if (memory->data) {
    UnmapViewOfFile(memory->data);
  }
  if (memory->mapping) {
    CloseHandle(memory->mapping);
  }
  *memory = SharedMemory{};
}

}  // namespace sabine_osr
