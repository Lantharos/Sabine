#include "common/switches.h"

#include <cerrno>
#include <cstdint>
#include <cstdlib>
#include <limits>

int SwitchInt(CefRefPtr<CefCommandLine> command_line,
              const std::string& name,
              int fallback) {
  const std::string value = command_line->GetSwitchValue(name);
  if (value.empty()) {
    return fallback;
  }
  return std::atoi(value.c_str());
}

float SwitchFloat(CefRefPtr<CefCommandLine> command_line,
                  const std::string& name,
                  float fallback) {
  const std::string value = command_line->GetSwitchValue(name);
  if (value.empty()) {
    return fallback;
  }
  return std::strtof(value.c_str(), nullptr);
}

cef_color_t SwitchColor(CefRefPtr<CefCommandLine> command_line,
                        const std::string& name,
                        cef_color_t fallback) {
  const std::string value = command_line->GetSwitchValue(name);
  if (value.empty()) {
    return fallback;
  }
  char* end = nullptr;
  errno = 0;
  const unsigned long parsed = std::strtoul(value.c_str(), &end, 0);
  if (errno != 0 || end == value.c_str() || *end != '\0' ||
      parsed > std::numeric_limits<uint32_t>::max()) {
    return fallback;
  }
  return static_cast<cef_color_t>(parsed);
}
