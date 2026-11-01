#ifndef SABINE_CEF_HOST_RENDERER_RENDERER_H_
#define SABINE_CEF_HOST_RENDERER_RENDERER_H_

#include <vector>

#include "include/cef_render_process_handler.h"
#include "include/cef_v8.h"

// What a renderer process installs in the pages it runs: the web bridge for
// the app's own documents, and the scripts that report media and input
// method state from every frame.
class SabineRenderer : public CefRenderProcessHandler {
 public:
  void OnBrowserCreated(CefRefPtr<CefBrowser> browser,
                        CefRefPtr<CefDictionaryValue> extra_info) override;
  void OnBrowserDestroyed(CefRefPtr<CefBrowser> browser) override;
  void OnContextCreated(CefRefPtr<CefBrowser> browser,
                        CefRefPtr<CefFrame> frame,
                        CefRefPtr<CefV8Context> context) override;
  void OnContextReleased(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefFrame> frame,
                         CefRefPtr<CefV8Context> context) override;
  bool OnProcessMessageReceived(CefRefPtr<CefBrowser> browser,
                                CefRefPtr<CefFrame> frame,
                                CefProcessId source_process,
                                CefRefPtr<CefProcessMessage> message) override;

 private:
  struct BrowserPolicy {
    CefRefPtr<CefBrowser> browser;
    CefRefPtr<CefDictionaryValue> policy;
  };

  CefRefPtr<CefDictionaryValue> BridgePolicyFor(CefRefPtr<CefBrowser> browser);
  void ReportRenderer(CefRefPtr<CefFrame> frame,
                      CefRefPtr<CefDictionaryValue> policy);

  std::vector<BrowserPolicy> bridge_policies_;
  bool renderer_reported_ = false;

  IMPLEMENT_REFCOUNTING(SabineRenderer);
};

#endif
