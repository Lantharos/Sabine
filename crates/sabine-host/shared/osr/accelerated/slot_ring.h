#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_SLOT_RING_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_SLOT_RING_H_

#include <array>
#include <cstdint>
#include <map>
#include <optional>
#include <vector>

#include "osr/accelerated/damage.h"

namespace sabine_osr {

constexpr uint32_t kSlotsPerSurface = 4;

// What every platform's Sabine-owned frame texture tracks: the identity the
// compositor imports it by, whether the compositor still samples it, and the
// pixels it is missing from frames it skipped.
struct RingSlot {
  uint64_t resource_id = 0;
  bool in_use = false;
  uint64_t token = 0;
  PixelRegion stale;
};

template <typename Slot, typename Extra>
struct SurfaceRing {
  std::array<Slot, kSlotsPerSurface> slots;
  uint32_t next = 0;
  Extra extra;

  void Damage(const PixelRegion& damage) {
    for (Slot& slot : slots) {
      slot.stale.Unite(damage);
    }
  }

  // The next slot the compositor is not sampling, taken in turn so a slot
  // just released is reused last.
  std::optional<uint32_t> FreeSlot() {
    const uint32_t first = next++ % kSlotsPerSurface;
    for (uint32_t offset = 0; offset < kSlotsPerSurface; ++offset) {
      const uint32_t index = (first + offset) % kSlotsPerSurface;
      if (!slots[index].in_use) {
        return index;
      }
    }
    return std::nullopt;
  }
};

struct NoExtra {};

// The slots of every accelerated surface in this process. `Destroy` releases
// a surface's platform resources before the surface is forgotten.
template <typename Slot, typename Extra = NoExtra>
class SlotRings {
 public:
  using Surface = SurfaceRing<Slot, Extra>;

  Surface& operator[](const AcceleratedSurfaceKey& key) { return rings_[key]; }

  Surface* Find(const AcceleratedSurfaceKey& key) {
    const auto found = rings_.find(key);
    return found == rings_.end() ? nullptr : &found->second;
  }

  bool empty() const { return rings_.empty(); }

  uint64_t NewResourceId() { return next_resource_id_++; }

  // Hands `slot` to the compositor until it releases the returned token.
  uint64_t Claim(Slot& slot) {
    slot.stale = {};
    slot.in_use = true;
    slot.token = next_token_++;
    return slot.token;
  }

  void Release(uint64_t token) {
    for (auto& [key, ring] : rings_) {
      for (Slot& slot : ring.slots) {
        if (slot.in_use && slot.token == token) {
          slot.in_use = false;
          slot.token = 0;
          return;
        }
      }
    }
  }

  template <typename Destroy>
  std::vector<uint64_t> RetireBrowser(int browser_id, Destroy destroy) {
    std::vector<uint64_t> retired;
    for (auto it = rings_.begin(); it != rings_.end();) {
      if (it->first.browser_id != browser_id) {
        ++it;
        continue;
      }
      Retire(it->second, destroy, &retired);
      it = rings_.erase(it);
    }
    return retired;
  }

  template <typename Destroy>
  std::vector<uint64_t> RetireAll(Destroy destroy) {
    std::vector<uint64_t> retired;
    for (auto& [key, ring] : rings_) {
      Retire(ring, destroy, &retired);
    }
    rings_.clear();
    return retired;
  }

 private:
  template <typename Destroy>
  static void Retire(Surface& ring,
                     Destroy& destroy,
                     std::vector<uint64_t>* retired) {
    for (const Slot& slot : ring.slots) {
      if (slot.resource_id != 0) {
        retired->push_back(slot.resource_id);
      }
    }
    destroy(ring);
  }

  std::map<AcceleratedSurfaceKey, Surface> rings_;
  uint64_t next_token_ = 1;
  uint64_t next_resource_id_ = 1;
};

}  // namespace sabine_osr

#endif
