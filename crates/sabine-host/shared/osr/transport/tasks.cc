#include "osr/transport/tasks.h"

#include <utility>

namespace sabine_osr {

OsrCommandTask::OsrCommandTask(CefRefPtr<SabineOsrHandler> handler,
                               std::string line,
                               std::optional<std::string> body)
    : handler_(std::move(handler)),
      line_(std::move(line)),
      body_(std::move(body)) {}

OsrCommandTask::~OsrCommandTask() {
  handler_->CompleteQueuedControl(line_.size() + (body_ ? body_->size() : 0));
}

void OsrCommandTask::Execute() {
  handler_->HandleQueuedControl(line_, body_);
}

OsrResizeTask::OsrResizeTask(CefRefPtr<SabineOsrHandler> handler)
    : handler_(std::move(handler)) {}

OsrResizeTask::~OsrResizeTask() = default;

void OsrResizeTask::Execute() {
  handler_->HandlePendingResize();
}

CloseOnDisconnectTask::CloseOnDisconnectTask(
    CefRefPtr<SabineOsrHandler> handler)
    : handler_(std::move(handler)) {}

CloseOnDisconnectTask::~CloseOnDisconnectTask() = default;

void CloseOnDisconnectTask::Execute() {
  handler_->CloseFromNativeDisconnect();
}

}  // namespace sabine_osr
