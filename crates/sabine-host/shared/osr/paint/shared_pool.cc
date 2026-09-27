#include "osr/paint/shared_pool.h"

#include <cerrno>
#include <cstdio>
#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>

namespace sabine_osr {
namespace {

constexpr size_t kMaxSlots = 4;
constexpr size_t kMinCapacity = 1024 * 1024;
constexpr size_t kMaxCapacity = 256 * 1024 * 1024;

size_t SlotCapacity(size_t bytes) {
  size_t capacity = kMinCapacity;
  while (capacity < bytes && capacity < kMaxCapacity) {
    capacity *= 2;
  }
  return capacity < bytes ? bytes : capacity;
}

int CreateSharedMemory() {
#if defined(__APPLE__)
  static uint32_t sequence = 0;
  char name[32];
  for (int attempt = 0; attempt < 16; ++attempt) {
    std::snprintf(name, sizeof(name), "/sabine-%d-%u", static_cast<int>(getpid()),
                  sequence++);
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
  return memfd_create("sabine-osr-paint", MFD_CLOEXEC);
#endif
}

void Unmap(SharedPaintSlot* slot) {
  if (slot->data) {
    munmap(slot->data, slot->capacity);
  }
  if (slot->fd >= 0) {
    close(slot->fd);
  }
  slot->fd = -1;
  slot->data = nullptr;
  slot->capacity = 0;
}

}  // namespace

SharedPaintPool::~SharedPaintPool() {
  for (SharedPaintSlot& slot : slots_) {
    Unmap(&slot);
  }
}

int SharedPaintPool::Acquire(size_t bytes) {
  int replaceable = -1;
  for (size_t index = 0; index < slots_.size(); ++index) {
    SharedPaintSlot& slot = slots_[index];
    if (slot.in_flight) {
      continue;
    }
    if (slot.capacity >= bytes) {
      slot.in_flight = true;
      return static_cast<int>(index);
    }
    replaceable = static_cast<int>(index);
  }
  if (replaceable < 0) {
    if (slots_.size() >= kMaxSlots) {
      return -1;
    }
    slots_.emplace_back();
    replaceable = static_cast<int>(slots_.size() - 1);
  }
  SharedPaintSlot& slot = Slot(replaceable);
  if (!Allocate(&slot, bytes)) {
    return -1;
  }
  slot.in_flight = true;
  return replaceable;
}

void SharedPaintPool::Release(uint32_t index, uint32_t generation) {
  if (index < slots_.size() && slots_[index].generation == generation) {
    slots_[index].in_flight = false;
  }
}

bool SharedPaintPool::Allocate(SharedPaintSlot* slot, size_t bytes) {
  Unmap(slot);
  const size_t capacity = SlotCapacity(bytes);
  const int fd = CreateSharedMemory();
  if (fd < 0) {
    return false;
  }
  if (ftruncate(fd, static_cast<off_t>(capacity)) != 0) {
    close(fd);
    return false;
  }
  void* data = mmap(nullptr, capacity, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
  if (data == MAP_FAILED) {
    close(fd);
    return false;
  }
  slot->fd = fd;
  slot->data = static_cast<char*>(data);
  slot->capacity = capacity;
  slot->generation += 1;
  slot->announced = false;
  return true;
}

}  // namespace sabine_osr
