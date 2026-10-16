#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "osr/handler.h"

namespace {

constexpr int64_t kReopenRetryMs = 250;

}  // namespace

void SabineOsrHandler::RepaintAllSoon() {
  if (repaint_all_pending_) {
    return;
  }
  repaint_all_pending_ = true;
  CefRefPtr<SabineOsrHandler> self(this);
  CefPostDelayedTask(TID_UI,
                     CefCreateClosureTask(base::BindOnce(
                         [](CefRefPtr<SabineOsrHandler> handler) {
                           handler->repaint_all_pending_ = false;
                           handler->RepaintAll();
                         },
                         self)),
                     kReopenRetryMs);
}
