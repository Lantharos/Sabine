#include "osr/handler.h"

#include <algorithm>
#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <limits>
#include <string>
#include <utility>

#include "common/bridge_policy.h"
#include "common/switches.h"
#include "include/cef_browser.h"
#include "include/cef_parser.h"
#include "osr/accelerated/paint.h"
#include "osr/common/registry.h"

using namespace sabine_osr;

namespace {

std::string ReadAuthenticationToken(const std::string& token_file) {
  std::string token;
  if (token_file.empty()) {
    return token;
  }
  std::ifstream input(token_file.c_str(), std::ios::in | std::ios::binary);
  if (input) {
    std::getline(input, token);
    while (!token.empty() && (token.back() == '\r' || token.back() == '\n')) {
      token.pop_back();
    }
  }
  // The secret must not linger; a relaunch hands over a fresh file.
  std::remove(token_file.c_str());
  return token;
}

#if defined(OS_WIN)
bool ParseParentWindow(const std::string& value, CefWindowHandle* parent) {
  if (value.empty()) {
    return true;
  }
  char* end = nullptr;
  errno = 0;
  const unsigned long long handle = std::strtoull(value.c_str(), &end, 10);
  if (errno || value.front() == '-' || *end ||
      handle > std::numeric_limits<uintptr_t>::max()) {
    return false;
  }
  *parent = reinterpret_cast<CefWindowHandle>(static_cast<uintptr_t>(handle));
  return true;
}
#endif

}  // namespace

SabineOsrHandler::SabineOsrHandler(std::string endpoint,
                                   std::string authentication_token,
                                   const BrowserViewport& viewport,
                                   CefRefPtr<CefDictionaryValue> bridge_policy,
                                   bool dev_mode,
                                   bool transparent_background,
                                   bool retain_hidden_frame)
    : endpoint_(std::move(endpoint)),
      authentication_token_(std::move(authentication_token)),
      width_(viewport.width),
      height_(viewport.height),
      scale_(viewport.scale),
      active_frame_rate_(viewport.active_frame_rate),
      background_frame_rate_(viewport.background_frame_rate),
      bridge_policy_(bridge_policy),
      dialogs_(new PageDialogs(this)),
      downloads_(new Downloads(this)),
#if defined(OS_LINUX)
      print_dialogs_(new PrintDialogs(this)),
#endif
      transparent_background_(transparent_background),
      retain_hidden_frame_(retain_hidden_frame),
      dev_mode_(dev_mode) {
  const auto commands = sabine_bridge::Commands(bridge_policy);
  bridge_commands_.insert(commands.begin(), commands.end());
  RegisterHandler(this);
}

SabineOsrHandler::~SabineOsrHandler() {
  UnregisterHandler(this);
}

void SabineOsrHandler::UseProfile(BrowserProfile profile, bool shared_texture) {
  profile_ = std::move(profile);
  shared_texture_ = shared_texture;
}

bool CreateSabineOsrBrowser(CefRefPtr<CefCommandLine> command_line,
                            const BrowserProfile& profile) {
  const std::string url_value = command_line->GetSwitchValue("url");
  const std::string url =
      url_value.empty() ? "about:blank" : std::string(url_value);
  const BrowserViewport viewport{
      std::max(1, SwitchInt(command_line, "sabine-width", 800)),
      std::max(1, SwitchInt(command_line, "sabine-height", 600)),
      std::max(0.25f, SwitchFloat(command_line, "sabine-scale", 1.0f)),
      std::max(1, SwitchInt(command_line, "sabine-active-frame-rate", 60)),
      std::max(1, SwitchInt(command_line, "sabine-background-frame-rate", 5)),
  };
  const std::string authentication_token = ReadAuthenticationToken(
      command_line->GetSwitchValue("sabine-osr-token-file"));
  if (authentication_token.empty()) {
    std::cerr << "Sabine OSR: missing authentication token file" << std::endl;
    return false;
  }

  auto policy_value = CefParseJSON(
      command_line->GetSwitchValue("sabine-bridge-policy"), JSON_PARSER_RFC);
  auto policy = policy_value ? policy_value->GetDictionary() : nullptr;
  if (!policy) {
    std::cerr << "Sabine OSR: missing bridge policy" << std::endl;
    return false;
  }
  if (!profile.renderer_cgroup.empty()) {
    policy->SetBool(sabine_bridge::kReportRendererKey, true);
  }
  const bool transparent = command_line->HasSwitch("sabine-transparent");
  CefBrowserSettings browser_settings;
  browser_settings.windowless_frame_rate = viewport.active_frame_rate;
  browser_settings.background_color =
      transparent ? CefColorSetARGB(0, 0, 0, 0)
                  : SwitchColor(command_line, "sabine-background-color",
                                CefColorSetARGB(255, 17, 17, 19));

  CefWindowInfo window_info;
  CefWindowHandle parent_window = kNullWindowHandle;
#if defined(OS_WIN)
  if (!ParseParentWindow(command_line->GetSwitchValue("sabine-parent-window"),
                         &parent_window)) {
    std::cerr << "Sabine OSR: invalid native parent window" << std::endl;
    return false;
  }
#endif
  window_info.SetAsWindowless(parent_window);
  const bool shared_texture = sabine_osr::PreferSharedTexture(command_line);
  sabine_osr::ApplySharedTexture(&window_info, shared_texture);
  CefRefPtr<SabineOsrHandler> handler(new SabineOsrHandler(
      command_line->GetSwitchValue("sabine-osr-endpoint"), authentication_token,
      viewport, policy, command_line->HasSwitch("sabine-dev-mode"), transparent,
      command_line->HasSwitch("sabine-retain-hidden-frame")));
  handler->UseProfile(profile, shared_texture);
  handler->UseAcceleratedPaint(shared_texture);
#if defined(OS_MAC)
  handler->UseSurfaceService(
      command_line->GetSwitchValue("sabine-surface-service"));
#elif defined(OS_WIN)
  handler->UseParentProcess(SwitchInt(command_line, "sabine-parent-pid", 0));
#endif
  return CefBrowserHost::CreateBrowser(
      window_info, handler, url, browser_settings, policy, profile.context);
}
