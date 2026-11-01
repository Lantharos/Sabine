#include "osr/accelerated/protocol.h"

#include "osr/transport/wire.h"

namespace sabine_osr {

namespace {

constexpr size_t kMetaLen = 4 + 4 + 4 + 4 + 4 + 8 + 4 + 8 + 8;

}  // namespace

void BuildAccelPayload(const std::string& guest_id,
                       const AccelPaintMeta& meta,
                       std::vector<char>* payload) {
  const size_t prefix_len = guest_id.empty() ? 0 : 2 + guest_id.size();
  payload->resize(prefix_len + kMetaLen);
  const size_t at = PutGuestPrefix(payload, guest_id);
  PutU32(payload, at, meta.format);
  PutI32(payload, at + 4, meta.visible_x);
  PutI32(payload, at + 8, meta.visible_y);
  PutU32(payload, at + 12, meta.visible_width);
  PutU32(payload, at + 16, meta.visible_height);
  PutU64(payload, at + 20, meta.resource_id);
  PutU32(payload, at + 28, meta.resource_slot);
  PutU64(payload, at + 32, meta.shared_handle);
  PutU64(payload, at + 40, meta.slot_token);
}

std::vector<char> BuildAccelRetirePayload(
    const std::vector<uint64_t>& resource_ids) {
  std::vector<char> payload(resource_ids.size() * 8, 0);
  for (size_t index = 0; index < resource_ids.size(); ++index) {
    PutU64(&payload, index * 8, resource_ids[index]);
  }
  return payload;
}

std::vector<char> BuildDmabufAnnouncePayload(uint64_t resource_id,
                                             uint64_t modifier,
                                             uint32_t stride,
                                             uint32_t offset) {
  std::vector<char> payload(8 + 8 + 4 + 4, 0);
  PutU64(&payload, 0, resource_id);
  PutU64(&payload, 8, modifier);
  PutU32(&payload, 16, stride);
  PutU32(&payload, 20, offset);
  return payload;
}

}  // namespace sabine_osr
