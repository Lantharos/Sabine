#include "renderer/renderer.h"

#include <algorithm>
#include <string>
#include <vector>

#include "common/bridge_policy.h"
#include "common/json.h"
#include "include/cef_command_line.h"
#include "include/wrapper/cef_helpers.h"
#include "renderer/bridge.h"
#include "renderer/clipboard.h"
#include "sabine_autoscroll_js.h"
#include "sabine_bridge_js.h"
#include "sabine_ime_state_js.h"
#include "sabine_media_state_js.h"

namespace {

std::string BridgeInstallScript(const std::vector<std::string>& commands) {
  std::string list = "[";
  for (size_t index = 0; index < commands.size(); ++index) {
    if (index > 0) {
      list += ",";
    }
    list += JsString(commands[index]);
  }
  return "window.__sabineBridgeCommands=" + list + "];" + SABINE_BRIDGE_JS_RAW;
}

}  // namespace

void SabineRenderer::OnBrowserCreated(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefDictionaryValue> extra_info) {
  CEF_REQUIRE_RENDERER_THREAD();
  bridge_policies_.push_back({browser, extra_info});
  ReportRenderer(browser->GetMainFrame(), extra_info);
}

void SabineRenderer::ReportRenderer(CefRefPtr<CefFrame> frame,
                                    CefRefPtr<CefDictionaryValue> policy) {
  if (renderer_reported_ || !frame || !frame->IsValid() || !policy ||
      !policy->GetBool(sabine_bridge::kReportRendererKey)) {
    return;
  }
  const CefString client_id =
      CefCommandLine::GetGlobalCommandLine()->GetSwitchValue(
          "renderer-client-id");
  if (client_id.empty()) {
    return;
  }
  CefRefPtr<CefProcessMessage> message =
      CefProcessMessage::Create(sabine_bridge::kRendererMessage);
  message->GetArgumentList()->SetString(0, client_id);
  frame->SendProcessMessage(PID_BROWSER, message);
  renderer_reported_ = true;
}

void SabineRenderer::OnBrowserDestroyed(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_RENDERER_THREAD();
  bridge_policies_.erase(
      std::remove_if(bridge_policies_.begin(), bridge_policies_.end(),
                     [&](const BrowserPolicy& entry) {
                       return entry.browser->IsSame(browser);
                     }),
      bridge_policies_.end());
  sabine_bridge::ReleaseBrowser(browser);
}

CefRefPtr<CefDictionaryValue> SabineRenderer::BridgePolicyFor(
    CefRefPtr<CefBrowser> browser) {
  for (const auto& entry : bridge_policies_) {
    if (entry.browser->IsSame(browser))
      return entry.policy;
  }
  return nullptr;
}

void SabineRenderer::OnContextCreated(CefRefPtr<CefBrowser> browser,
                                      CefRefPtr<CefFrame> frame,
                                      CefRefPtr<CefV8Context> context) {
  CEF_REQUIRE_RENDERER_THREAD();
  const std::string url = frame->GetURL();
  sabine_bridge::RunWithTransport(frame, context, SABINE_IME_STATE_JS_RAW,
                                  "reportImeState", "sabine.ime_state");
  sabine_bridge::RunWithTransport(frame, context, SABINE_MEDIA_STATE_JS_RAW,
                                  "reportMediaState", "sabine.media_state");
#if !defined(OS_WIN)
  frame->ExecuteJavaScript(SABINE_AUTOSCROLL_JS_RAW, url, 0);
#endif
  const auto policy = BridgePolicyFor(browser);
  if (frame->IsMain())
    ReportRenderer(frame, policy);
  const std::string security_origin = sabine_bridge::RememberContext(context);
  const bool bridged =
      frame->IsMain() && sabine_bridge::ExposesBridge(policy, url) &&
      sabine_bridge::MatchesSecurityOrigin(policy, url, security_origin);
  if (bridged) {
    sabine_bridge::InstallTransport(frame, context, "__sabineNativePostMessage",
                                    "sabine.native");
    frame->ExecuteJavaScript(
        BridgeInstallScript(sabine_bridge::CommandsFor(policy, url)), url, 0);
  }
  if (policy && policy->GetBool("clipboard"))
    sabine_clipboard::Install(frame, context,
                              sabine_bridge::AllowsDocument(policy, url));
}

void SabineRenderer::OnContextReleased(CefRefPtr<CefBrowser> browser,
                                       CefRefPtr<CefFrame> frame,
                                       CefRefPtr<CefV8Context> context) {
  CEF_REQUIRE_RENDERER_THREAD();
  sabine_bridge::ReleaseContext(context);
  sabine_clipboard::Release(context);
}

bool SabineRenderer::OnProcessMessageReceived(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefProcessId source_process,
    CefRefPtr<CefProcessMessage> message) {
  CEF_REQUIRE_RENDERER_THREAD();
  if (source_process != PID_BROWSER)
    return false;
  return sabine_clipboard::Receive(frame, message) ||
         sabine_bridge::Receive(browser, frame, message,
                                BridgePolicyFor(browser));
}
