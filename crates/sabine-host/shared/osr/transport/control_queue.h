#ifndef SABINE_CEF_HOST_OSR_TRANSPORT_CONTROL_QUEUE_H_
#define SABINE_CEF_HOST_OSR_TRANSPORT_CONTROL_QUEUE_H_

#include <condition_variable>
#include <cstddef>
#include <mutex>
#include <optional>
#include <string>

#include "include/cef_base.h"
#include "include/cef_task.h"

class SabineOsrHandler;

namespace sabine_osr {

// Bounds the controls read from the window but not yet handled, so a window
// that writes faster than the browser handles controls waits for it.
class ControlQueue {
 public:
  // Waits for room for |bytes|; false once the queue is closed.
  bool Reserve(size_t bytes);
  void Release(size_t bytes);
  void Close();
  bool closed();

 private:
  std::mutex mutex_;
  std::condition_variable space_;
  size_t count_ = 0;
  size_t bytes_ = 0;
  bool closed_ = false;
};

// A control the browser handles once, after the reader thread queued it.
class QueuedControlTask : public CefTask {
 public:
  QueuedControlTask(CefRefPtr<SabineOsrHandler> handler,
                    std::string line,
                    std::optional<std::string> body);
  ~QueuedControlTask() override;
  void Execute() override;

 private:
  CefRefPtr<SabineOsrHandler> handler_;
  const std::string line_;
  const std::optional<std::string> body_;
  IMPLEMENT_REFCOUNTING(QueuedControlTask);
};

// The newest of the controls that supersede each other, such as pointer
// moves, waiting for the browser to take it.
class LatestControl : public CefBaseRefCounted {
 public:
  explicit LatestControl(std::string line);

  // Replaces the waiting control; false once the browser took it.
  bool Replace(std::string& line);
  std::string Take();

 private:
  std::mutex mutex_;
  std::string line_;
  bool taken_ = false;
  IMPLEMENT_REFCOUNTING(LatestControl);
};

// The resize the browser applies next, and whether the frame of the last one
// is still to come.
struct PendingResize {
  std::mutex mutex;
  std::string line;
  bool task_pending = false;
  bool in_flight = false;
};

}  // namespace sabine_osr

#endif
