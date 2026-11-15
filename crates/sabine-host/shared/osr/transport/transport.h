#ifndef SABINE_CEF_HOST_OSR_TRANSPORT_TRANSPORT_H_
#define SABINE_CEF_HOST_OSR_TRANSPORT_TRANSPORT_H_

#include <cstdint>
#include <initializer_list>
#include <optional>
#include <string>
#include <string_view>

#include "include/base/cef_build.h"
#include "include/cef_base.h"
#include "osr/transport/control_queue.h"
#include "osr/transport/socket.h"
#if defined(OS_WIN)
#include "osr/accelerated/windows/parent_process.h"
#endif

class SabineOsrHandler;

namespace sabine_osr {

// The window's connection: the socket, the thread reading controls from it,
// and the controls read but not yet handled.
class Transport {
 public:
  Transport(SabineOsrHandler* owner,
            std::string endpoint,
            std::string authentication_token);
  Transport(const Transport&) = delete;
  Transport& operator=(const Transport&) = delete;

  bool Connect();
  const std::string& authentication_token() const {
    return authentication_token_;
  }
  void StartReading();
  void Close();
  bool closed() { return controls_.closed(); }
  void CompleteQueued(size_t bytes) { controls_.Release(bytes); }

  // Takes the resize the browser applies next, whose frame it then waits for.
  std::string TakeResize();
  bool ResizeInFlight();
  // The frame of the last resize was painted, so the next one can apply.
  void ResizeFramePainted();

  bool Send(uint32_t kind,
            uint32_t width,
            uint32_t height,
            int32_t x,
            int32_t y,
            const void* payload,
            uint32_t payload_len);
  bool Send(uint32_t kind, std::initializer_list<std::string_view> parts);
#if defined(OS_WIN)
  ParentProcess& parent_process() { return parent_process_; }
#else
  bool SendWithFd(uint32_t kind,
                  uint32_t width,
                  uint32_t height,
                  int32_t x,
                  int32_t y,
                  const void* payload,
                  uint32_t payload_len,
                  int fd);
#endif

 private:
  void ReaderStopped();
  void ReadControls();
  bool DispatchControl(std::string line,
                       std::optional<std::string> body,
                       CefRefPtr<LatestControl>* move);
  void QueueResize(std::string line);
  void PostResize();

  SabineOsrHandler* const owner_;
  const std::string endpoint_;
  const std::string authentication_token_;
  Socket socket_;
  // Keeps the handler while the reader runs, so the reader thread never lets
  // go of the last reference.
  CefRefPtr<SabineOsrHandler> reader_;
  ControlQueue controls_;
  PendingResize resize_;
#if defined(OS_WIN)
  ParentProcess parent_process_;
#endif
};

}  // namespace sabine_osr

#endif
