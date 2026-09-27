#ifndef SABINE_CEF_HOST_OSR_PAINT_SHARED_POOL_H_
#define SABINE_CEF_HOST_OSR_PAINT_SHARED_POOL_H_

#include <cstddef>
#include <cstdint>
#include <vector>

namespace sabine_osr {

struct SharedPaintSlot {
  int fd = -1;
  char* data = nullptr;
  size_t capacity = 0;
  uint32_t generation = 0;
  bool in_flight = false;
  bool announced = false;
};

// Reusable shared-memory paint buffers. A slot stays mapped in both processes
// and returns to the pool when the native host releases it after copying.
class SharedPaintPool {
 public:
  SharedPaintPool() = default;
  SharedPaintPool(const SharedPaintPool&) = delete;
  SharedPaintPool& operator=(const SharedPaintPool&) = delete;
  ~SharedPaintPool();

  int Acquire(size_t bytes);
  SharedPaintSlot& Slot(int index) {
    return slots_[static_cast<size_t>(index)];
  }
  void Release(uint32_t index, uint32_t generation);

 private:
  bool Allocate(SharedPaintSlot* slot, size_t bytes);

  std::vector<SharedPaintSlot> slots_;
};

}  // namespace sabine_osr

#endif
