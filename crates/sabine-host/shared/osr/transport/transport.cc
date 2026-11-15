#include "osr/transport/transport.h"

#include <cstdio>
#include <cstdlib>
#include <thread>
#include <utility>
#include <vector>

#include "common/bytes_message.h"
#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/handler.h"

namespace sabine_osr {

namespace {

constexpr size_t kMaxPendingBytes = 64 * 1024 * 1024;
constexpr size_t kReadSize = 64 * 1024;
constexpr char kResizePrefix[] = "resize\t";
constexpr char kMouseMovePrefix[] = "mouse_move\t";

bool StartsWith(const std::string& line, const char* prefix) {
  return line.rfind(prefix, 0) == 0;
}

}  // namespace

Transport::Transport(SabineOsrHandler* owner,
                     std::string endpoint,
                     std::string authentication_token)
    : owner_(owner),
      endpoint_(std::move(endpoint)),
      authentication_token_(std::move(authentication_token)) {}

bool Transport::Connect() {
  return socket_.Connect(endpoint_, authentication_token_);
}

void Transport::StartReading() {
  reader_ = owner_;
  std::thread([this] {
    ReadControls();
    CefPostTask(TID_UI, base::BindOnce(&Transport::ReaderStopped,
                                       base::Unretained(this)));
  }).detach();
}

void Transport::ReaderStopped() {
  CEF_REQUIRE_UI_THREAD();
  const CefRefPtr<SabineOsrHandler> owner = std::move(reader_);
  owner->CloseFromNativeDisconnect();
}

void Transport::ReadControls() {
  std::string pending;
  size_t start = 0;
  size_t searched = 0;
  CefRefPtr<LatestControl> move;
  std::vector<char> buffer(kReadSize);
  const auto fail = [this](const char* reason) {
    std::fprintf(stderr, "Sabine OSR control: %s\n", reason);
    Close();
  };
  while (true) {
    const int received =
        socket_.Receive(buffer.data(), static_cast<int>(buffer.size()));
    if (received <= 0) {
      return;
    }
    pending.append(buffer.data(), static_cast<size_t>(received));
    size_t newline;
    while ((newline = pending.find('\n', start + searched)) !=
           std::string::npos) {
      size_t line_start = start;
      size_t consumed = newline + 1;
      std::optional<std::string> body;
      if (pending.compare(start, sizeof(sabine_bytes::kPrefix) - 1,
                          sabine_bytes::kPrefix) == 0) {
        const size_t length_start = start + sizeof(sabine_bytes::kPrefix) - 1;
        const size_t tab = pending.find('\t', length_start);
        const unsigned long long length =
            std::strtoull(pending.c_str() + length_start, nullptr, 10);
        if (tab >= newline || length > sabine_bytes::kMaxBodyBytes) {
          fail("a message carried malformed bytes");
          return;
        }
        if (pending.size() - consumed < length) {
          searched = newline - start;
          break;
        }
        body.emplace(pending, consumed, length);
        line_start = tab + 1;
        consumed += length;
      }
      std::string line(pending, line_start, newline - line_start);
      start = consumed;
      searched = 0;
      if (!DispatchControl(std::move(line), std::move(body), &move)) {
        return;
      }
    }
    if (newline == std::string::npos) {
      searched = pending.size() - start;
    }
    pending.erase(0, start);
    start = 0;
    if (pending.size() >= kMaxPendingBytes) {
      fail("a message exceeded 64 MiB");
      return;
    }
  }
}

bool Transport::DispatchControl(std::string line,
                                std::optional<std::string> body,
                                CefRefPtr<LatestControl>* move) {
  if (StartsWith(line, kResizePrefix)) {
    QueueResize(std::move(line));
    return true;
  }
  if (StartsWith(line, kMouseMovePrefix)) {
    if (!*move || !(*move)->Replace(line)) {
      *move = new LatestControl(std::move(line));
      CefPostTask(TID_UI,
                  base::BindOnce(&SabineOsrHandler::HandleLatestControl,
                                 CefRefPtr<SabineOsrHandler>(owner_), *move));
    }
    return true;
  }
  *move = nullptr;
  const size_t bytes = line.size() + (body ? body->size() : 0);
  return controls_.Reserve(bytes) &&
         CefPostTask(TID_UI, new QueuedControlTask(owner_, std::move(line),
                                                   std::move(body)));
}

bool Transport::Send(uint32_t kind,
                     uint32_t width,
                     uint32_t height,
                     int32_t x,
                     int32_t y,
                     const void* payload,
                     uint32_t payload_len) {
  return socket_.Send(
      kind, width, height, x, y,
      {std::string_view(static_cast<const char*>(payload), payload_len)});
}

bool Transport::Send(uint32_t kind,
                     std::initializer_list<std::string_view> parts) {
  return socket_.Send(kind, 0, 0, 0, 0, parts);
}

#if !defined(OS_WIN)
bool Transport::SendWithFd(uint32_t kind,
                           uint32_t width,
                           uint32_t height,
                           int32_t x,
                           int32_t y,
                           const void* payload,
                           uint32_t payload_len,
                           int fd) {
  return socket_.SendWithFd(
      kind, width, height, x, y,
      std::string_view(static_cast<const char*>(payload), payload_len), fd);
}
#endif

void Transport::Close() {
  controls_.Close();
  socket_.Shutdown();
}

void Transport::QueueResize(std::string line) {
  {
    std::lock_guard<std::mutex> lock(resize_.mutex);
    resize_.line = std::move(line);
    if (resize_.task_pending) {
      return;
    }
    resize_.task_pending = true;
  }
  PostResize();
}

void Transport::PostResize() {
  if (!CefPostTask(TID_UI,
                   base::BindOnce(&SabineOsrHandler::HandlePendingResize,
                                  CefRefPtr<SabineOsrHandler>(owner_)))) {
    std::lock_guard<std::mutex> lock(resize_.mutex);
    resize_.task_pending = false;
  }
}

std::string Transport::TakeResize() {
  std::lock_guard<std::mutex> lock(resize_.mutex);
  std::string line = std::move(resize_.line);
  resize_.line.clear();
  resize_.task_pending = false;
  resize_.in_flight = !line.empty();
  return line;
}

bool Transport::ResizeInFlight() {
  std::lock_guard<std::mutex> lock(resize_.mutex);
  return resize_.in_flight;
}

void Transport::ResizeFramePainted() {
  {
    std::lock_guard<std::mutex> lock(resize_.mutex);
    if (!resize_.in_flight) {
      return;
    }
    resize_.in_flight = false;
    if (resize_.line.empty() || resize_.task_pending) {
      return;
    }
    resize_.task_pending = true;
  }
  PostResize();
}

}  // namespace sabine_osr
