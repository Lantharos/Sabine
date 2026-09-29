#include "common/bytes_message.h"

#include <cstdint>
#include <cstring>

#include "include/cef_shared_process_message_builder.h"

namespace sabine_bytes {

CefRefPtr<CefProcessMessage> Create(const std::string& name,
                                    const std::vector<std::string>& fields,
                                    const char* body,
                                    size_t body_size) {
  size_t size = 2 * sizeof(uint32_t) + body_size;
  for (const auto& field : fields)
    size += sizeof(uint32_t) + field.size();
  auto builder = CefSharedProcessMessageBuilder::Create(name, size);
  if (!builder || !builder->IsValid())
    return nullptr;
  auto* cursor = static_cast<char*>(builder->Memory());
  const auto put = [&cursor](const void* data, size_t length) {
    std::memcpy(cursor, data, length);
    cursor += length;
  };
  const uint32_t count = static_cast<uint32_t>(fields.size());
  put(&count, sizeof(count));
  for (const auto& field : fields) {
    const uint32_t length = static_cast<uint32_t>(field.size());
    put(&length, sizeof(length));
    put(field.data(), field.size());
  }
  const uint32_t length = static_cast<uint32_t>(body_size);
  put(&length, sizeof(length));
  if (body_size)
    put(body, body_size);
  return builder->Build();
}

bool Read(CefRefPtr<CefProcessMessage> message, Message* read) {
  auto region = message->GetSharedMemoryRegion();
  if (!region || !region->IsValid())
    return false;
  const char* cursor = static_cast<const char*>(region->Memory());
  const char* end = cursor + region->Size();
  const auto take = [&cursor, end](void* out, size_t length) {
    if (static_cast<size_t>(end - cursor) < length)
      return false;
    std::memcpy(out, cursor, length);
    cursor += length;
    return true;
  };
  uint32_t count = 0;
  if (!take(&count, sizeof(count)) || count > 16)
    return false;
  read->fields.clear();
  for (uint32_t index = 0; index < count; ++index) {
    uint32_t length = 0;
    if (!take(&length, sizeof(length)) ||
        static_cast<size_t>(end - cursor) < length)
      return false;
    read->fields.emplace_back(cursor, length);
    cursor += length;
  }
  uint32_t body_size = 0;
  if (!take(&body_size, sizeof(body_size)) || body_size > kMaxBodyBytes ||
      static_cast<size_t>(end - cursor) < body_size)
    return false;
  read->body = cursor;
  read->body_size = body_size;
  read->region = region;
  return true;
}

}  // namespace sabine_bytes
