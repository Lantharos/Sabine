#include "osr/handler.h"

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <thread>
#include <utility>
#include <vector>

#include "common/bytes_message.h"
#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "include/wrapper/cef_helpers.h"

using namespace sabine_osr;

namespace {

constexpr size_t kMaxPendingBytes = 64 * 1024 * 1024;
constexpr size_t kReadSize = 64 * 1024;
constexpr char kResizePrefix[] = "resize\t";
constexpr char kMouseMovePrefix[] = "mouse_move\t";

bool StartsWith(const std::string& line, const char* prefix) {
  return line.rfind(prefix, 0) == 0;
}

// Chromium sizes the view's pixels with gfx::ScaleToCeiledSize, rounding up
// in single precision.
int ViewPixels(int dips, float scale) {
  const float pixels = static_cast<float>(dips) * scale;
  return static_cast<int>(std::ceil(pixels));
}

}  // namespace

void SabineOsrHandler::StartCommandReader() {
  CefRefPtr<SabineOsrHandler> self(this);
  std::thread([self]() mutable {
    self->ReadControls();
    CefPostTask(TID_UI,
                base::BindOnce(&SabineOsrHandler::CloseFromNativeDisconnect,
                               std::move(self)));
  }).detach();
}

void SabineOsrHandler::ReadControls() {
  std::string pending;
  size_t start = 0;
  size_t searched = 0;
  CefRefPtr<LatestControl> move;
  std::vector<char> buffer(kReadSize);
  const auto fail = [this](const char* reason) {
    std::fprintf(stderr, "Sabine OSR control: %s\n", reason);
    CloseTransport();
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

bool SabineOsrHandler::DispatchControl(std::string line,
                                       std::optional<std::string> body,
                                       CefRefPtr<LatestControl>* move) {
  if (StartsWith(line, kResizePrefix)) {
    QueueResizeControlLine(std::move(line));
    return true;
  }
  if (StartsWith(line, kMouseMovePrefix)) {
    if (!*move || !(*move)->Replace(line)) {
      *move = new LatestControl(std::move(line));
      CefPostTask(TID_UI,
                  base::BindOnce(&SabineOsrHandler::HandleLatestControl,
                                 CefRefPtr<SabineOsrHandler>(this), *move));
    }
    return true;
  }
  *move = nullptr;
  const size_t bytes = line.size() + (body ? body->size() : 0);
  return controls_.Reserve(bytes) &&
         CefPostTask(TID_UI, new QueuedControlTask(this, std::move(line),
                                                   std::move(body)));
}

void SabineOsrHandler::CompleteQueuedControl(size_t bytes) {
  controls_.Release(bytes);
}

void SabineOsrHandler::HandleQueuedControl(
    const std::string& line,
    const std::optional<std::string>& body) {
  CEF_REQUIRE_UI_THREAD();
  if (!controls_.closed()) {
    HandleControlLine(line, body);
  }
}

void SabineOsrHandler::HandleLatestControl(CefRefPtr<LatestControl> control) {
  CEF_REQUIRE_UI_THREAD();
  const std::string line = control->Take();
  if (!controls_.closed()) {
    HandleControlLine(line);
  }
}

bool SabineOsrHandler::SendMessage(uint32_t kind,
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

bool SabineOsrHandler::SendMessage(
    uint32_t kind,
    std::initializer_list<std::string_view> parts) {
  return socket_.Send(kind, 0, 0, 0, 0, parts);
}

#if !defined(OS_WIN)
bool SabineOsrHandler::SendMessageWithFd(uint32_t kind,
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

void SabineOsrHandler::CloseTransport() {
  controls_.Close();
  socket_.Shutdown();
}

void SabineOsrHandler::QueueResizeControlLine(std::string line) {
  {
    std::lock_guard<std::mutex> lock(resize_.mutex);
    resize_.line = std::move(line);
    if (resize_.task_pending) {
      return;
    }
    resize_.task_pending = true;
  }
  PostPendingResize();
}

void SabineOsrHandler::PostPendingResize() {
  if (!CefPostTask(TID_UI,
                   base::BindOnce(&SabineOsrHandler::HandlePendingResize,
                                  CefRefPtr<SabineOsrHandler>(this)))) {
    std::lock_guard<std::mutex> lock(resize_.mutex);
    resize_.task_pending = false;
  }
}

void SabineOsrHandler::HandlePendingResize() {
  CEF_REQUIRE_UI_THREAD();
  std::string line;
  {
    std::lock_guard<std::mutex> lock(resize_.mutex);
    line = std::move(resize_.line);
    resize_.line.clear();
    resize_.task_pending = false;
    resize_.in_flight = !line.empty();
  }
  if (!line.empty()) {
    HandleControlLine(line);
  }
}

bool SabineOsrHandler::FitsView(int pixel_width, int pixel_height) const {
  return pixel_width == ViewPixels(width_, scale_) &&
         pixel_height == ViewPixels(height_, scale_);
}

bool SabineOsrHandler::QualifyResizeFrame(int pixel_width, int pixel_height) {
  CEF_REQUIRE_UI_THREAD();
  std::lock_guard<std::mutex> lock(resize_.mutex);
  return !resize_.in_flight || FitsView(pixel_width, pixel_height);
}

void SabineOsrHandler::CompleteResizeFrame(int pixel_width, int pixel_height) {
  CEF_REQUIRE_UI_THREAD();
  if (!FitsView(pixel_width, pixel_height)) {
    return;
  }
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
  PostPendingResize();
}
