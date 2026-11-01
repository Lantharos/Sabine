#include "osr/paint/shared_pool.h"

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

}  // namespace

SharedPaintPool::~SharedPaintPool() {
  for (SharedPaintSlot& slot : slots_) {
    ReleaseSharedMemory(&slot.memory);
  }
}

int SharedPaintPool::Acquire(size_t bytes) {
  int replaceable = -1;
  for (size_t index = 0; index < slots_.size(); ++index) {
    SharedPaintSlot& slot = slots_[index];
    if (slot.in_flight) {
      continue;
    }
    if (slot.memory.capacity >= bytes) {
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
  ReleaseSharedMemory(&slot->memory);
  if (!CreateSharedMemory(SlotCapacity(bytes), &slot->memory)) {
    return false;
  }
  slot->generation += 1;
  slot->announced = false;
  return true;
}

}  // namespace sabine_osr
