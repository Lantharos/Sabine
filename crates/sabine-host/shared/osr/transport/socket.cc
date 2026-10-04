#include "osr/handler.h"

#include <array>
#include <cstring>

#ifdef _WIN32
#include <winsock2.h>
#include <afunix.h>
#else
#include <sys/socket.h>
#include <sys/uio.h>
#include <sys/un.h>
#include <unistd.h>
#endif

#include "osr/transport/wire.h"

using namespace sabine_osr;

namespace {

using MessageHeader = std::array<char, 28>;

void PutHeaderU32(MessageHeader* header, size_t offset, uint32_t value) {
  for (size_t i = 0; i < 4; ++i) {
    (*header)[offset + i] = static_cast<char>((value >> (i * 8)) & 0xff);
  }
}

MessageHeader Header(uint32_t kind,
                     uint32_t width,
                     uint32_t height,
                     int32_t x,
                     int32_t y,
                     uint32_t payload_len) {
  MessageHeader header{'S', 'A', 'B', '1'};
  PutHeaderU32(&header, 4, kind);
  PutHeaderU32(&header, 8, width);
  PutHeaderU32(&header, 12, height);
  PutHeaderU32(&header, 16, static_cast<uint32_t>(x));
  PutHeaderU32(&header, 20, static_cast<uint32_t>(y));
  PutHeaderU32(&header, 24, payload_len);
  return header;
}

}  // namespace

bool SabineOsrHandler::ConnectSocket() {
#ifdef _WIN32
  WSADATA data{};
  if (WSAStartup(MAKEWORD(2, 2), &data) != 0) {
    return false;
  }
  using NativeSocket = SOCKET;
  constexpr NativeSocket kInvalidSocket = INVALID_SOCKET;
  const auto close_socket = [](NativeSocket socket) { closesocket(socket); };
#else
  using NativeSocket = int;
  constexpr NativeSocket kInvalidSocket = -1;
  const auto close_socket = [](NativeSocket socket) { close(socket); };
#endif
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  if (endpoint_.size() >= sizeof(address.sun_path)) {
    return false;
  }
  std::memcpy(address.sun_path, endpoint_.data(), endpoint_.size());
  const NativeSocket connection = socket(AF_UNIX, SOCK_STREAM, 0);
  if (connection == kInvalidSocket) {
    return false;
  }
  const std::string authentication = authentication_token_ + "\n";
  if (connect(connection, reinterpret_cast<sockaddr*>(&address),
              sizeof(address)) != 0 ||
      !SendAll(static_cast<intptr_t>(connection), authentication.data(),
               authentication.size())) {
    close_socket(connection);
    return false;
  }
  socket_fd_ = static_cast<intptr_t>(connection);
  return true;
}

void SabineOsrHandler::CloseTransport() {
  {
    std::lock_guard<std::mutex> lock(control_mutex_);
    controls_closed_ = true;
  }
  control_space_.notify_all();
  if (socket_fd_ < 0)
    return;
#ifdef _WIN32
  shutdown(static_cast<SOCKET>(socket_fd_), SD_BOTH);
#else
  shutdown(static_cast<int>(socket_fd_), SHUT_RDWR);
#endif
}

bool SabineOsrHandler::SendMessage(uint32_t kind,
                                   uint32_t width,
                                   uint32_t height,
                                   int32_t x,
                                   int32_t y,
                                   const void* payload,
                                   uint32_t payload_len) {
  if (socket_fd_ < 0) {
    return false;
  }
  std::lock_guard<std::mutex> lock(socket_mutex_);
  MessageHeader header = Header(kind, width, height, x, y, payload_len);
  return SendAll(socket_fd_, header.data(), header.size()) &&
         (payload_len == 0 ||
          SendAll(socket_fd_, static_cast<const char*>(payload), payload_len));
}

bool SabineOsrHandler::SendMessageWithFd(uint32_t kind,
                                         uint32_t width,
                                         uint32_t height,
                                         int32_t x,
                                         int32_t y,
                                         const void* payload,
                                         uint32_t payload_len,
                                         int fd) {
#ifdef _WIN32
  return false;
#else
  if (socket_fd_ < 0 || fd < 0) {
    return false;
  }
  std::lock_guard<std::mutex> lock(socket_mutex_);
  MessageHeader header = Header(kind, width, height, x, y, payload_len);

  iovec iov{};
  iov.iov_base = header.data();
  iov.iov_len = header.size();
  alignas(cmsghdr) char control[CMSG_SPACE(sizeof(int))] = {};
  msghdr message{};
  message.msg_iov = &iov;
  message.msg_iovlen = 1;
  message.msg_control = control;
  message.msg_controllen = sizeof(control);
  cmsghdr* cmsg = CMSG_FIRSTHDR(&message);
  cmsg->cmsg_level = SOL_SOCKET;
  cmsg->cmsg_type = SCM_RIGHTS;
  cmsg->cmsg_len = CMSG_LEN(sizeof(int));
  std::memcpy(CMSG_DATA(cmsg), &fd, sizeof(int));

  const ssize_t sent = sendmsg(socket_fd_, &message, MSG_NOSIGNAL);
  return sent == static_cast<ssize_t>(header.size()) &&
         (payload_len == 0 ||
          SendAll(socket_fd_, static_cast<const char*>(payload), payload_len));
#endif
}
