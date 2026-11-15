#ifndef SABINE_CEF_HOST_APP_H_
#define SABINE_CEF_HOST_APP_H_

#include "include/cef_app.h"
#include "renderer/renderer.h"

class SabineApp : public CefApp, public CefBrowserProcessHandler {
 public:
  explicit SabineApp(bool runtime_smoke_test = false);

  CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override {
    return this;
  }
  CefRefPtr<CefRenderProcessHandler> GetRenderProcessHandler() override {
    return renderer_;
  }

  void OnRegisterCustomSchemes(
      CefRawPtr<CefSchemeRegistrar> registrar) override;
  void OnContextInitialized() override;
#if defined(OS_WIN)
  void OnBeforeChildProcessLaunch(
      CefRefPtr<CefCommandLine> command_line) override;
#endif
  bool OnAlreadyRunningAppRelaunch(CefRefPtr<CefCommandLine> command_line,
                                   const CefString& current_directory) override;
  CefRefPtr<CefClient> GetDefaultClient() override;

 private:
  CefRefPtr<SabineRenderer> renderer_;
  const bool runtime_smoke_test_;

  IMPLEMENT_REFCOUNTING(SabineApp);
};

#endif
