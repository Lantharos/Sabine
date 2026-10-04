#include "osr/handler.h"

#include <algorithm>
#include <array>
#include <cctype>
#include <cerrno>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <limits>
#include <set>
#include <sstream>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#ifdef _WIN32
#include <winsock2.h>
#include <afunix.h>
#else
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/uio.h>
#include <unistd.h>
#endif

#include "guest/input.h"
#include "guest/manager.h"
#include "include/cef_app.h"
#include "include/cef_browser.h"
#include "include/cef_parser.h"
#include "include/cef_request_context_handler.h"
#include "include/cef_task.h"
#include "include/internal/cef_types.h"
#include "include/wrapper/cef_helpers.h"
#include "common/bytes_message.h"
#include "common/json.h"
#include "sabine_bridge_js.h"
#include "osr/utilities.h"
#include "osr/tasks.h"

using namespace sabine_osr;

namespace {
constexpr size_t kMaxControlBytes = 64 * 1024 * 1024;
constexpr size_t kMaxControlCount = 256;

void PutHeaderU32(std::array<char, 28>* header, size_t offset, uint32_t value) {
  for (size_t i = 0; i < 4; ++i) {
    (*header)[offset + i] = static_cast<char>((value >> (i * 8)) & 0xff);
  }
}

void PutHeaderI32(std::array<char, 28>* header, size_t offset, int32_t value) {
  PutHeaderU32(header, offset, static_cast<uint32_t>(value));
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

void SabineOsrHandler::StartCommandReader() {
  if (socket_fd_ < 0) {
    return;
  }
  const intptr_t fd = socket_fd_;
  CefRefPtr<SabineOsrHandler> self(this);
  std::thread([self, fd] {
    std::string pending;
    size_t searched = 0;
    std::vector<char> buffer(64 * 1024);
    const auto disconnect = [&self](const char* reason) {
      std::fprintf(stderr, "Sabine OSR control: %s\n", reason);
      self->CloseTransport();
      CefPostTask(TID_UI, new CloseOnDisconnectTask(self));
    };
    while (true) {
      const int n = recv(
#ifdef _WIN32
          static_cast<SOCKET>(fd),
#else
          static_cast<int>(fd),
#endif
          buffer.data(), static_cast<int>(buffer.size()), 0);
      if (n <= 0) {
        // Native host exited or crashed — close this browser only so sibling
        // OSR windows sharing the process singleton keep running.
        CefPostTask(TID_UI, new CloseOnDisconnectTask(self));
        break;
      }
      pending.append(buffer.data(), static_cast<size_t>(n));
      size_t newline = 0;
      while ((newline = pending.find('\n', searched)) != std::string::npos) {
        if (newline >= kMaxControlBytes) {
          disconnect("a message exceeded 64 MiB");
          return;
        }
        std::string line = pending.substr(0, newline);
        size_t consumed = newline + 1;
        std::optional<std::string> body;
        if (line.rfind(sabine_bytes::kPrefix, 0) == 0) {
          const size_t header = sizeof(sabine_bytes::kPrefix) - 1;
          const size_t tab = line.find('\t', header);
          const unsigned long long length =
              std::strtoull(line.c_str() + header, nullptr, 10);
          if (tab == std::string::npos ||
              length > sabine_bytes::kMaxBodyBytes) {
            disconnect("a message carried malformed bytes");
            return;
          }
          if (pending.size() - consumed < length) {
            searched = newline;
            break;
          }
          body = pending.substr(consumed, length);
          consumed += length;
          line.erase(0, tab + 1);
        }
        pending.erase(0, consumed);
        searched = 0;
        if (line.rfind("resize\t", 0) == 0) {
          self->QueueResizeControlLine(std::move(line));
        } else if (!self->QueueControl(std::move(line), std::move(body))) {
          return;
        }
      }
      if (newline == std::string::npos)
        searched = pending.size();
      if (pending.size() >= kMaxControlBytes) {
        disconnect("a message exceeded 64 MiB");
        return;
      }
    }
  }).detach();
}

bool SabineOsrHandler::QueueControl(std::string line,
                                    std::optional<std::string> body) {
  const size_t bytes = line.size() + (body ? body->size() : 0);
  {
    std::unique_lock<std::mutex> lock(control_mutex_);
    control_space_.wait(lock, [&] {
      return controls_closed_ || (control_count_ < kMaxControlCount &&
                                  control_bytes_ + bytes <= kMaxControlBytes);
    });
    if (controls_closed_)
      return false;
    ++control_count_;
    control_bytes_ += bytes;
  }
  return CefPostTask(
      TID_UI, new OsrCommandTask(this, std::move(line), std::move(body)));
}

void SabineOsrHandler::CompleteQueuedControl(size_t bytes) {
  {
    std::lock_guard<std::mutex> lock(control_mutex_);
    --control_count_;
    control_bytes_ -= bytes;
  }
  control_space_.notify_one();
}

void SabineOsrHandler::HandleQueuedControl(
    const std::string& line,
    const std::optional<std::string>& body) {
  CEF_REQUIRE_UI_THREAD();
  {
    std::lock_guard<std::mutex> lock(control_mutex_);
    if (controls_closed_)
      return;
  }
  HandleControlLine(line, body);
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

void SabineOsrHandler::QueueResizeControlLine(std::string line) {
  bool schedule = false;
  {
    std::lock_guard<std::mutex> lock(resize_mutex_);
    pending_resize_line_ = std::move(line);
    if (!resize_task_pending_) {
      resize_task_pending_ = true;
      schedule = true;
    }
  }
  if (schedule && !CefPostTask(TID_UI, new OsrResizeTask(this))) {
    std::lock_guard<std::mutex> lock(resize_mutex_);
    resize_task_pending_ = false;
  }
}

void SabineOsrHandler::HandlePendingResize() {
  CEF_REQUIRE_UI_THREAD();
  std::string line;
  {
    std::lock_guard<std::mutex> lock(resize_mutex_);
    line = std::move(pending_resize_line_);
    pending_resize_line_.clear();
    resize_task_pending_ = false;
    resize_in_flight_ = !line.empty();
  }
  if (!line.empty()) {
    HandleControlLine(line);
  }
}

bool SabineOsrHandler::QualifyResizeFrame(int pixel_width, int pixel_height) {
  CEF_REQUIRE_UI_THREAD();
  const int expected_width =
      std::max(1, static_cast<int>(std::lround(width_ * scale_)));
  const int expected_height =
      std::max(1, static_cast<int>(std::lround(height_ * scale_)));
  std::lock_guard<std::mutex> lock(resize_mutex_);
  if (!resize_in_flight_) {
    return true;
  }
  return std::abs(pixel_width - expected_width) <= 1 &&
         std::abs(pixel_height - expected_height) <= 1;
}

void SabineOsrHandler::CompleteResizeFrame(int pixel_width, int pixel_height) {
  CEF_REQUIRE_UI_THREAD();
  const int expected_width =
      std::max(1, static_cast<int>(std::lround(width_ * scale_)));
  const int expected_height =
      std::max(1, static_cast<int>(std::lround(height_ * scale_)));
  if (std::abs(pixel_width - expected_width) > 1 ||
      std::abs(pixel_height - expected_height) > 1) {
    return;
  }
  bool schedule = false;
  {
    std::lock_guard<std::mutex> lock(resize_mutex_);
    if (!resize_in_flight_) {
      return;
    }
    resize_in_flight_ = false;
    if (!pending_resize_line_.empty() && !resize_task_pending_) {
      resize_task_pending_ = true;
      schedule = true;
    }
  }
  if (schedule && !CefPostTask(TID_UI, new OsrResizeTask(this))) {
    std::lock_guard<std::mutex> lock(resize_mutex_);
    resize_task_pending_ = false;
  }
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
  std::array<char, 28> header{};
  header[0] = 'S';
  header[1] = 'A';
  header[2] = 'B';
  header[3] = '1';
  PutHeaderU32(&header, 4, kind);
  PutHeaderU32(&header, 8, width);
  PutHeaderU32(&header, 12, height);
  PutHeaderI32(&header, 16, x);
  PutHeaderI32(&header, 20, y);
  PutHeaderU32(&header, 24, payload_len);
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
  std::array<char, 28> header{};
  header[0] = 'S';
  header[1] = 'A';
  header[2] = 'B';
  header[3] = '1';
  PutHeaderU32(&header, 4, kind);
  PutHeaderU32(&header, 8, width);
  PutHeaderU32(&header, 12, height);
  PutHeaderI32(&header, 16, x);
  PutHeaderI32(&header, 20, y);
  PutHeaderU32(&header, 24, payload_len);

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

bool SabineOsrHandler::SendPaintBatch(PaintSurface surface,
                                      const std::string& guest_id,
                                      int32_t origin_x,
                                      int32_t origin_y,
                                      const void* buffer,
                                      int buffer_width,
                                      int buffer_height,
                                      const RectList& dirty_rects) {
  if (buffer_width <= 0 || buffer_height <= 0 || !buffer) {
    return false;
  }

  std::vector<CefRect> source_rects;
  if (dirty_rects.empty()) {
    source_rects.push_back(CefRect(0, 0, buffer_width, buffer_height));
  } else {
    source_rects.assign(dirty_rects.begin(), dirty_rects.end());
  }

  std::vector<PaintRectBytes> rects;
  uint64_t total_bytes = 0;
  for (const auto& rect : source_rects) {
    const int left = std::max(0, rect.x);
    const int top = std::max(0, rect.y);
    const int right = std::min(buffer_width, rect.x + rect.width);
    const int bottom = std::min(buffer_height, rect.y + rect.height);
    const int width = right - left;
    const int height = bottom - top;
    if (width <= 0 || height <= 0) {
      continue;
    }
    const uint64_t len = static_cast<uint64_t>(width) * height * 4;
    if (len > std::numeric_limits<uint32_t>::max()) {
      return false;
    }
    rects.push_back(PaintRectBytes{
        left,
        top,
        width,
        height,
        total_bytes,
        static_cast<uint32_t>(len),
    });
    total_bytes += len;
  }
  if (rects.empty()) {
    return true;
  }

  const std::string prefix = surface == PaintSurface::kGuest
                                 ? GuestPayloadPrefix(guest_id)
                                 : std::string();
#ifndef _WIN32
  if (total_bytes >= kSharedPaintThreshold) {
    const int index = shared_paint_.Acquire(static_cast<size_t>(total_bytes));
    if (index >= 0) {
      SharedPaintSlot& slot = shared_paint_.Slot(index);
      for (const auto& rect : rects) {
        CopyPaintRect(slot.data, buffer, buffer_width, rect);
      }
      std::vector<char> metadata = PaintMetadata(
          prefix, rects, static_cast<uint32_t>(index), slot.generation);
      const uint32_t shared_kind = SharedBatchKind(surface);
      const uint32_t metadata_len = static_cast<uint32_t>(metadata.size());
      const bool sent =
          slot.announced
              ? SendMessage(shared_kind, static_cast<uint32_t>(buffer_width),
                            static_cast<uint32_t>(buffer_height), origin_x,
                            origin_y, metadata.data(), metadata_len)
              : SendMessageWithFd(
                    shared_kind, static_cast<uint32_t>(buffer_width),
                    static_cast<uint32_t>(buffer_height), origin_x, origin_y,
                    metadata.data(), metadata_len, slot.fd);
      if (!sent) {
        shared_paint_.Release(static_cast<uint32_t>(index), slot.generation);
        return false;
      }
      slot.announced = true;
      return true;
    }
  }
#endif

  std::vector<char> payload = PaintMetadata(prefix, rects);
  const size_t metadata_len = payload.size();
  if (metadata_len + total_bytes > std::numeric_limits<uint32_t>::max()) {
    return false;
  }
  payload.resize(metadata_len + static_cast<size_t>(total_bytes));
  for (const auto& rect : rects) {
    CopyPaintRect(payload.data() + metadata_len, buffer, buffer_width, rect);
  }
  return SendMessage(BatchKind(surface), static_cast<uint32_t>(buffer_width),
                     static_cast<uint32_t>(buffer_height), origin_x, origin_y,
                     payload.data(), static_cast<uint32_t>(payload.size()));
}

#ifndef _WIN32
void SabineOsrHandler::ReleaseSharedPaint(uint32_t slot, uint32_t generation) {
  CEF_REQUIRE_UI_THREAD();
  shared_paint_.Release(slot, generation);
}
#endif
