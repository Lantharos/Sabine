#ifndef SABINE_CEF_HOST_COMMON_SWITCHES_H_
#define SABINE_CEF_HOST_COMMON_SWITCHES_H_

#include <string>

#include "include/cef_command_line.h"

// Marks a development run, whose page comes from a development server.
inline constexpr char kDevelopmentSwitch[] = "sabine-dev-mode";

int SwitchInt(CefRefPtr<CefCommandLine> command_line,
              const std::string& name,
              int fallback);
float SwitchFloat(CefRefPtr<CefCommandLine> command_line,
                  const std::string& name,
                  float fallback);
cef_color_t SwitchColor(CefRefPtr<CefCommandLine> command_line,
                        const std::string& name,
                        cef_color_t fallback);

#endif
