#include "osr/transport/socket.h"

#include <array>
#include <cstring>
#include <vector>

#if defined(OS_WIN)
#include <winsock2.h>
#include <afunix.h>
#else
#include <sys/socket.h>
#include <sys/uio.h>
#include <sys/un.h>
#include <unistd.h>
#endif

namespace sabine_osr {

namespace {

using MessageHeader = std::array<char, 28>;

#if defined(OS_WIN)
using NativeSocket = SOCKET;
using Buffer = WSABUF;
constexpr NativeSocket kInvalidSocket = INVALID_SOCKET;

void CloseNative(NativeSocket socket) {
  closesocket(socket);
}

Buffer MakeBuffer(const char* data, size_t size) {
  return {static_cast<ULONG>(size), const_cast<char*>(data)};
}

size_t Length(const Buffer& buffer) {
  return buffer.len;
}

void Advance(Buffer& buffer, size_t count) {
  buffer.buf += count;
  buffer.len -= static_cast<ULONG>(count);
}
#else
using NativeSocket = int;
using Buffer = iovec;
constexpr NativeSocket kInvalidSocket = -1;

void CloseNative(NativeSocket socket) {
  close(socket);
}

Buffer MakeBuffer(const char* data, size_t size) {
  return {const_cast<char*>(data), size};
}

size_t Length(const Buffer& buffer) {
  return buffer.iov_len;
}

void Advance(Buffer& buffer, size_t count) {
  buffer.iov_base = static_cast<char*>(buffer.iov_base) + count;
  buffer.iov_len -= count;
}
#endif

NativeSocket Native(intptr_t fd) {
  return static_cast<NativeSocket>(fd);
}

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
                     size_t payload_len) {
  MessageHeader header{'S', 'A', 'B', '1'};
  PutHeaderU32(&header, 4, kind);
  PutHeaderU32(&header, 8, width);
  PutHeaderU32(&header, 12, height);
  PutHeaderU32(&header, 16, static_cast<uint32_t>(x));
  PutHeaderU32(&header, 20, static_cast<uint32_t>(y));
  PutHeaderU32(&header, 24, static_cast<uint32_t>(payload_len));
  return header;
}

void Consume(std::vector<Buffer>* buffers, size_t* first, size_t sent) {
  while (sent > 0) {
    Buffer& buffer = (*buffers)[*first];
    if (sent < Length(buffer)) {
      Advance(buffer, sent);
      return;
    }
    sent -= Length(buffer);
    ++*first;
  }
}

bool SendBuffers(intptr_t socket, std::vector<Buffer> buffers, int fd) {
  size_t first = 0;
  while (first < buffers.size()) {
#if defined(OS_WIN)
    DWORD sent = 0;
    if (WSASend(Native(socket), buffers.data() + first,
                static_cast<DWORD>(buffers.size() - first), &sent, 0, nullptr,
                nullptr) != 0 ||
        sent == 0) {
      return false;
    }
#else
    msghdr message{};
    message.msg_iov = buffers.data() + first;
    message.msg_iovlen =
        static_cast<decltype(message.msg_iovlen)>(buffers.size() - first);
    alignas(cmsghdr) char control[CMSG_SPACE(sizeof(int))] = {};
    if (fd >= 0) {
      message.msg_control = control;
      message.msg_controllen = sizeof(control);
      cmsghdr* descriptor = CMSG_FIRSTHDR(&message);
      descriptor->cmsg_level = SOL_SOCKET;
      descriptor->cmsg_type = SCM_RIGHTS;
      descriptor->cmsg_len = CMSG_LEN(sizeof(int));
      std::memcpy(CMSG_DATA(descriptor), &fd, sizeof(int));
      fd = -1;
    }
    const ssize_t sent = sendmsg(Native(socket), &message, MSG_NOSIGNAL);
    if (sent <= 0) {
      return false;
    }
#endif
    Consume(&buffers, &first, static_cast<size_t>(sent));
  }
  return true;
}

std::vector<Buffer> MessageBuffers(
    const MessageHeader& header,
    std::initializer_list<std::string_view> payload) {
  std::vector<Buffer> buffers;
  buffers.reserve(payload.size() + 1);
  buffers.push_back(MakeBuffer(header.data(), header.size()));
  for (std::string_view part : payload) {
    if (!part.empty()) {
      buffers.push_back(MakeBuffer(part.data(), part.size()));
    }
  }
  return buffers;
}

size_t PayloadSize(std::initializer_list<std::string_view> payload) {
  size_t size = 0;
  for (std::string_view part : payload) {
    size += part.size();
  }
  return size;
}

}  // namespace

Socket::~Socket() {
  if (fd_ < 0) {
    return;
  }
  CloseNative(Native(fd_));
#if defined(OS_WIN)
  WSACleanup();
#endif
}

bool Socket::Connect(const std::string& endpoint, const std::string& token) {
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  if (endpoint.size() >= sizeof(address.sun_path)) {
    return false;
  }
  std::memcpy(address.sun_path, endpoint.data(), endpoint.size());
#if defined(OS_WIN)
  WSADATA data{};
  if (WSAStartup(MAKEWORD(2, 2), &data) != 0) {
    return false;
  }
#endif
  const NativeSocket connection = socket(AF_UNIX, SOCK_STREAM, 0);
  const std::string authentication = token + "\n";
  if (connection == kInvalidSocket ||
      connect(connection, reinterpret_cast<sockaddr*>(&address),
              sizeof(address)) != 0 ||
      !SendBuffers(static_cast<intptr_t>(connection),
                   {MakeBuffer(authentication.data(), authentication.size())},
                   -1)) {
    if (connection != kInvalidSocket) {
      CloseNative(connection);
    }
#if defined(OS_WIN)
    WSACleanup();
#endif
    return false;
  }
  fd_ = static_cast<intptr_t>(connection);
  return true;
}

int Socket::Receive(char* buffer, int size) {
  return recv(Native(fd_), buffer, size, 0);
}

void Socket::Shutdown() {
  if (fd_ < 0) {
    return;
  }
#if defined(OS_WIN)
  shutdown(Native(fd_), SD_BOTH);
#else
  shutdown(Native(fd_), SHUT_RDWR);
#endif
}

bool Socket::Send(uint32_t kind,
                  uint32_t width,
                  uint32_t height,
                  int32_t x,
                  int32_t y,
                  std::initializer_list<std::string_view> payload) {
  if (fd_ < 0) {
    return false;
  }
  const MessageHeader header =
      Header(kind, width, height, x, y, PayloadSize(payload));
  std::lock_guard<std::mutex> lock(send_mutex_);
  return SendBuffers(fd_, MessageBuffers(header, payload), -1);
}

#if !defined(OS_WIN)
bool Socket::SendWithFd(uint32_t kind,
                        uint32_t width,
                        uint32_t height,
                        int32_t x,
                        int32_t y,
                        std::string_view payload,
                        int fd) {
  if (fd_ < 0 || fd < 0) {
    return false;
  }
  const MessageHeader header =
      Header(kind, width, height, x, y, payload.size());
  std::lock_guard<std::mutex> lock(send_mutex_);
  return SendBuffers(fd_, MessageBuffers(header, {payload}), fd);
}
#endif

}  // namespace sabine_osr
