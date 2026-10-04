#include "osr/handler.h"

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <thread>

#ifdef _WIN32
#include <winsock2.h>
#else
#include <sys/socket.h>
#endif

#include "common/bytes_message.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/transport/tasks.h"

using namespace sabine_osr;

namespace {
constexpr size_t kMaxControlBytes = 64 * 1024 * 1024;
constexpr size_t kMaxControlCount = 256;
}  // namespace

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
