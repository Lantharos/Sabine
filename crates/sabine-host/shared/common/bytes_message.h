#ifndef SABINE_COMMON_BYTES_MESSAGE_H_
#define SABINE_COMMON_BYTES_MESSAGE_H_

#include <string>
#include <vector>

#include "include/cef_process_message.h"
#include "include/cef_shared_memory_region.h"

namespace sabine_bytes {

// A bridge message that carries bytes between the browser and a renderer:
// a few text fields followed by the bytes, in shared memory.
struct Message {
  std::vector<std::string> fields;
  const char* body = nullptr;
  size_t body_size = 0;
  CefRefPtr<CefSharedMemoryRegion> region;
};

constexpr char kPrefix[] = "SABINE_BRIDGE_BYTES\t";
constexpr size_t kMaxBodyBytes = 32 * 1024 * 1024;

CefRefPtr<CefProcessMessage> Create(const std::string& name,
                                    const std::vector<std::string>& fields,
                                    const char* body,
                                    size_t body_size);
bool Read(CefRefPtr<CefProcessMessage> message, Message* read);

}  // namespace sabine_bytes

#endif
