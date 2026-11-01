#include "app/app.h"

#include <cstdio>
#include <string>

#include "app/probe.h"
#include "app/scheme.h"
#include "common/switches.h"
#include "include/cef_command_line.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/registry.h"
#include "osr/handler.h"
#if defined(OS_LINUX)
#include "linux/runtime/server.h"
#endif

namespace {

void CreateBrowser(CefRefPtr<CefCommandLine> command_line) {
  const BrowserProfile profile{
      nullptr,
      CefCommandLine::GetGlobalCommandLine()->GetSwitchValue("root-cache-path"),
      std::string(), std::string()};
  if (!CreateSabineOsrBrowser(command_line, profile)) {
    std::fprintf(stderr, "Sabine OSR: browser creation failed\n");
    if (!sabine_osr::HasRegisteredHandlers())
      CefQuitMessageLoop();
  }
}
}  // namespace

SabineApp::SabineApp(bool runtime_smoke_test)
    : renderer_(new SabineRenderer()),
      runtime_smoke_test_(runtime_smoke_test) {}

void SabineApp::OnRegisterCustomSchemes(
    CefRawPtr<CefSchemeRegistrar> registrar) {
  sabine_app::RegisterAppScheme(
      registrar,
      CefCommandLine::GetGlobalCommandLine()->HasSwitch(kDevelopmentSwitch));
}

void SabineApp::OnContextInitialized() {
  CEF_REQUIRE_UI_THREAD();
  if (runtime_smoke_test_) {
    StartRuntimeProbe();
    return;
  }
  CefRefPtr<CefCommandLine> command_line =
      CefCommandLine::GetGlobalCommandLine();
#if defined(OS_LINUX)
  if (sabine_runtime::IsSharedRuntime(command_line)) {
    sabine_runtime::Serve(command_line);
    return;
  }
#endif
  const std::string app_root = command_line->GetSwitchValue("sabine-app-root");
  if (!app_root.empty()) {
    sabine_app::ServeAppFiles(app_root);
  }
  if (command_line->HasSwitch("sabine-local-files")) {
    sabine_app::ServeLocalFiles(command_line->GetSwitchValue("url"));
  }
  CreateBrowser(command_line);
}

void SabineApp::OnBeforeChildProcessLaunch(
    CefRefPtr<CefCommandLine> command_line) {
  CefRefPtr<CefCommandLine> browser = CefCommandLine::GetGlobalCommandLine();
  if (browser->HasSwitch(kDevelopmentSwitch)) {
    command_line->AppendSwitch(kDevelopmentSwitch);
  }
#if defined(OS_WIN)
  const CefString cef_dir = browser->GetSwitchValue("sabine-cef-dir");
  if (!cef_dir.empty()) {
    command_line->AppendSwitchWithValue("sabine-cef-dir", cef_dir);
  }
#endif
}

bool SabineApp::OnAlreadyRunningAppRelaunch(
    CefRefPtr<CefCommandLine> command_line,
    const CefString& current_directory) {
  CEF_REQUIRE_UI_THREAD();
#if defined(OS_LINUX)
  if (sabine_runtime::IsSharedRuntime(CefCommandLine::GetGlobalCommandLine())) {
    return true;
  }
#endif
  CreateBrowser(command_line);
  return true;
}

CefRefPtr<CefClient> SabineApp::GetDefaultClient() {
  return sabine_osr::PrimaryHandler();
}
