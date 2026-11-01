#ifndef SABINE_CEF_HOST_OSR_TRANSPORT_SOCKET_H_
#define SABINE_CEF_HOST_OSR_TRANSPORT_SOCKET_H_

#include <cstdint>
#include <initializer_list>
#include <mutex>
#include <string>
#include <string_view>

#include "include/base/cef_build.h"

namespace sabine_osr {

// The window's socket. Every message is one header followed by its payload
// parts, written in a single gathered send so messages from different threads
// never interleave.
class Socket {
 public:
  Socket() = default;
  Socket(const Socket&) = delete;
  Socket& operator=(const Socket&) = delete;
  ~Socket();

  bool Connect(const std::string& endpoint, const std::string& token);
  bool connected() const { return fd_ >= 0; }
  int Receive(char* buffer, int size);
  void Shutdown();

  bool Send(uint32_t kind,
            uint32_t width,
            uint32_t height,
            int32_t x,
            int32_t y,
            std::initializer_list<std::string_view> payload);
#if !defined(OS_WIN)
  bool SendWithFd(uint32_t kind,
                  uint32_t width,
                  uint32_t height,
                  int32_t x,
                  int32_t y,
                  std::string_view payload,
                  int fd);
#endif

 private:
  intptr_t fd_ = -1;
  std::mutex send_mutex_;
};

}  // namespace sabine_osr

#endif
