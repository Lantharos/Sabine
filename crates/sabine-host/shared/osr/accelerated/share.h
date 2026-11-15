#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_SHARE_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_SHARE_H_

#include <cstdint>
#include <vector>

#include "osr/accelerated/slot_ring.h"
#include "osr/common/registry.h"
#include "osr/handler.h"

namespace sabine_osr {

// The copy resources are shared by every window in this process, so every
// window drops the ones a copy replaced, and paints again when `repaint`.
inline void RetireEverywhere(const std::vector<uint64_t>& resource_ids,
                             bool repaint = false) {
  if (resource_ids.empty() && !repaint) {
    return;
  }
  for (SabineOsrHandler* handler : RegisteredHandlers()) {
    handler->RetireAcceleratedResources(resource_ids);
    if (repaint) {
      handler->RepaintAll();
    }
  }
}

}  // namespace sabine_osr

template <typename Announce>
bool SabineOsrHandler::ShareCopiedSlot(const sabine_osr::CopiedSlot& copied,
                                       Announce announce,
                                       CopiedAccelFrame* out) {
  uint64_t shared_handle = 0;
  if (!announced_accelerated_resources_.count(copied.resource_id)) {
    if (!announce(&shared_handle)) {
      ReleaseAcceleratedSlot(copied.slot_token);
      return false;
    }
    announced_accelerated_resources_.insert(copied.resource_id);
  }
  *out = {copied.width,      copied.height,  copied.resource_id,
          copied.slot_index, shared_handle,  copied.slot_token,
          copied.damage,     copied.sequence};
  return true;
}

#endif
