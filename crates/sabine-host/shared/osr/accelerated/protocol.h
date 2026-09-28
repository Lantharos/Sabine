#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_PROTOCOL_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_PROTOCOL_H_

#include <cstdint>
#include <string>
#include <vector>

#include "include/cef_render_handler.h"

constexpr uint32_t kMainAccel = 24;
constexpr uint32_t kPopupAccel = 25;
constexpr uint32_t kGuestAccel = 26;
constexpr uint32_t kAccelRetire = 39;

namespace sabine_osr {

struct AccelPaintMeta {
  uint32_t format = 0;
  int32_t visible_x = 0;
  int32_t visible_y = 0;
  uint32_t visible_width = 0;
  uint32_t visible_height = 0;
  /// Stable identity of the Sabine-owned texture the frame was copied into.
  uint64_t resource_id = 0;
  /// Producer slot index, so the compositor can keep one import per slot.
  uint32_t resource_slot = 0;
  /// Windows: NT HANDLE duplicated into the compositor process the first time
  /// a resource is sent, otherwise 0.
  uint64_t shared_handle = 0;
  /// Identifies the producer slot released after the compositor is done
  /// sampling.
  uint64_t slot_token = 0;
};

std::string BuildAccelPayload(const std::string& guest_id,
                              const AccelPaintMeta& meta);
std::string BuildAccelRetirePayload(const std::vector<uint64_t>& resource_ids);

}  // namespace sabine_osr

#endif
