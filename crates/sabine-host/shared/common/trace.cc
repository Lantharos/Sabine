#include "common/trace.h"

#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <string>

namespace {

bool ReadTraceSetting() {
  const char* setting = std::getenv("SABINE_TRACE");
  if (!setting) {
    return false;
  }
  std::string value(setting);
  const auto space = [](unsigned char c) { return std::isspace(c) != 0; };
  value.erase(value.begin(),
              std::find_if_not(value.begin(), value.end(), space));
  value.erase(std::find_if_not(value.rbegin(), value.rend(), space).base(),
              value.end());
  std::transform(value.begin(), value.end(), value.begin(),
                 [](unsigned char c) { return std::tolower(c); });
  return value == "1" || value == "true" || value == "yes" || value == "on" ||
         value == "trace";
}

}  // namespace

bool TraceEnabled() {
  static const bool enabled = ReadTraceSetting();
  return enabled;
}
