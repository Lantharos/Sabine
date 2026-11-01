#include <algorithm>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <string>
#include <utility>

#include "common/json.h"
#include "include/base/cef_callback.h"
#include "include/cef_devtools_message_observer.h"
#include "include/wrapper/cef_closure_task.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/handler.h"

// Chromium offers DevTools only as a window of its own, so they open instead
// as a guest view docked along the bottom of the window, loading the DevTools
// frontend against the page's target on the remote debugging server that
// development windows run.
namespace {

// Chromium answers this request before ExecuteDevToolsMethod returns, so the
// request carries an id of its own, far above the ones Chromium assigns.
constexpr int kTargetInfoRequest = 1 << 30;

std::string DevToolsPort() {
  const std::filesystem::path root(CefCommandLine::GetGlobalCommandLine()
                                       ->GetSwitchValue("root-cache-path")
                                       .ToString());
  std::ifstream file(root / "DevToolsActivePort");
  std::string port;
  std::getline(file, port);
  return port;
}

class DevToolsTarget : public CefDevToolsMessageObserver {
 public:
  DevToolsTarget(SabineOsrHandler* owner, std::string port)
      : owner_(owner), port_(std::move(port)) {}

  void OnDevToolsMethodResult(CefRefPtr<CefBrowser> browser,
                              int message_id,
                              bool success,
                              const void* result,
                              size_t result_size) override {
    if (message_id != kTargetInfoRequest) {
      return;
    }
    const std::string target =
        success
            ? JsonObject::Parse(
                  std::string(static_cast<const char*>(result), result_size))
                  .Object("targetInfo")
                  .String("targetId")
            : std::string();
    if (!target.empty()) {
      CefPostTask(
          TID_UI,
          base::BindOnce(&SabineOsrHandler::OpenDevToolsFrontend,
                         CefRefPtr<SabineOsrHandler>(owner_), port_, target));
    }
  }

 private:
  SabineOsrHandler* const owner_;
  const std::string port_;

  IMPLEMENT_REFCOUNTING(DevToolsTarget);
};

}  // namespace

CefRect SabineOsrHandler::DevToolsBounds() const {
  const int height = std::max(1, height_ * 2 / 5);
  return CefRect(0, height_ - height, width_, height);
}

void SabineOsrHandler::ShowDevTools(CefRefPtr<CefBrowser> inspected) {
  CEF_REQUIRE_UI_THREAD();
  CloseDevTools();
  const std::string port = DevToolsPort();
  if (port.empty()) {
    std::fprintf(stderr,
                 "Sabine: DevTools need the remote debugging port, which "
                 "this app turned off\n");
    return;
  }
  CefRefPtr<CefBrowserHost> host = inspected->GetHost();
  CefRefPtr<DevToolsTarget> target(new DevToolsTarget(this, port));
  devtools_registration_ = host->AddDevToolsMessageObserver(target);
  devtools_inspected_ = inspected;
  host->ExecuteDevToolsMethod(kTargetInfoRequest, "Target.getTargetInfo",
                              nullptr);
}

void SabineOsrHandler::OpenDevToolsFrontend(const std::string& port,
                                            const std::string& target) {
  CEF_REQUIRE_UI_THREAD();
  devtools_registration_ = nullptr;
  if (!devtools_inspected_) {
    return;
  }
  GuestCreateRequest request;
  request.id = kSabineDevToolsGuestId;
  request.url =
      "devtools://devtools/bundled/devtools_app.html?ws=127.0.0.1:" + port +
      "/devtools/page/" + target;
  request.bounds = DevToolsBounds();
  request.allow_downloads = false;
  ContinueCreateGuest(
      request, nullptr, [](bool success, const std::string& result) {
        if (!success) {
          std::fprintf(stderr, "Sabine: DevTools did not open: %s\n",
                       result.c_str());
        }
      });
  FocusGuest(kSabineDevToolsGuestId);
}

void SabineOsrHandler::CloseDevTools() {
  CEF_REQUIRE_UI_THREAD();
  devtools_registration_ = nullptr;
  devtools_inspected_ = nullptr;
  DestroyGuest(kSabineDevToolsGuestId);
}
