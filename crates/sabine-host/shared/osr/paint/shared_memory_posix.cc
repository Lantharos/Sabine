#include "osr/paint/shared_memory.h"

#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>

namespace sabine_osr {
namespace {

int CreateFd() {
#if defined(__APPLE__)
  static uint32_t sequence = 0;
  char name[32];
  for (int attempt = 0; attempt < 16; ++attempt) {
    std::snprintf(name, sizeof(name), "/sabine-%d-%u",
                  static_cast<int>(getpid()), sequence++);
    const int fd = shm_open(name, O_RDWR | O_CREAT | O_EXCL, 0600);
    if (fd >= 0) {
      shm_unlink(name);
      return fd;
    }
    if (errno != EEXIST) {
      return -1;
    }
  }
  return -1;
#else
  return memfd_create("sabine-osr-paint", MFD_CLOEXEC | MFD_ALLOW_SEALING);
#endif
}

// The window maps the memory read-only and refuses it unless it can never
// shrink under that mapping. A macOS shared memory object keeps the size it
// is first given.
bool FixSize(int fd, size_t capacity) {
  if (ftruncate(fd, static_cast<off_t>(capacity)) != 0) {
    return false;
  }
#if defined(__APPLE__)
  return true;
#else
  return fcntl(fd, F_ADD_SEALS, F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_SEAL) == 0;
#endif
}

}  // namespace

bool CreateSharedMemory(size_t capacity, SharedMemory* out) {
  const int fd = CreateFd();
  if (fd < 0) {
    return false;
  }
  if (!FixSize(fd, capacity)) {
    close(fd);
    return false;
  }
  void* data =
      mmap(nullptr, capacity, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
  if (data == MAP_FAILED) {
    close(fd);
    return false;
  }
  out->fd = fd;
  out->data = static_cast<char*>(data);
  out->capacity = capacity;
  return true;
}

void ReleaseSharedMemory(SharedMemory* memory) {
  if (memory->data) {
    munmap(memory->data, memory->capacity);
  }
  if (memory->fd >= 0) {
    close(memory->fd);
  }
  *memory = SharedMemory{};
}

}  // namespace sabine_osr
