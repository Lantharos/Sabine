#include "osr/transport/control_queue.h"

#include <utility>

#include "osr/handler.h"

namespace sabine_osr {

namespace {

constexpr size_t kMaxQueuedBytes = 64 * 1024 * 1024;
constexpr size_t kMaxQueuedControls = 256;

}  // namespace

bool ControlQueue::Reserve(size_t bytes) {
  std::unique_lock<std::mutex> lock(mutex_);
  space_.wait(lock, [&] {
    return closed_ ||
           (count_ < kMaxQueuedControls && bytes_ + bytes <= kMaxQueuedBytes);
  });
  if (closed_) {
    return false;
  }
  ++count_;
  bytes_ += bytes;
  return true;
}

void ControlQueue::Release(size_t bytes) {
  {
    std::lock_guard<std::mutex> lock(mutex_);
    --count_;
    bytes_ -= bytes;
  }
  space_.notify_one();
}

void ControlQueue::Close() {
  {
    std::lock_guard<std::mutex> lock(mutex_);
    closed_ = true;
  }
  space_.notify_all();
}

bool ControlQueue::closed() {
  std::lock_guard<std::mutex> lock(mutex_);
  return closed_;
}

QueuedControlTask::QueuedControlTask(CefRefPtr<SabineOsrHandler> handler,
                                     std::string line,
                                     std::optional<std::string> body)
    : handler_(std::move(handler)),
      line_(std::move(line)),
      body_(std::move(body)) {}

QueuedControlTask::~QueuedControlTask() {
  handler_->CompleteQueuedControl(line_.size() + (body_ ? body_->size() : 0));
}

void QueuedControlTask::Execute() {
  handler_->HandleQueuedControl(line_, body_);
}

LatestControl::LatestControl(std::string line) : line_(std::move(line)) {}

bool LatestControl::Replace(std::string& line) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (taken_) {
    return false;
  }
  line_ = std::move(line);
  return true;
}

std::string LatestControl::Take() {
  std::lock_guard<std::mutex> lock(mutex_);
  taken_ = true;
  return std::move(line_);
}

}  // namespace sabine_osr
