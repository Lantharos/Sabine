#include "osr/common/registry.h"

#include <algorithm>
#include <cstdlib>

namespace sabine_osr {

SabineOsrHandler* g_instance = nullptr;
std::mutex g_handlers_mutex;
std::vector<SabineOsrHandler*> g_handlers;

void RegisterHandler(SabineOsrHandler* handler) {
  std::lock_guard<std::mutex> lock(g_handlers_mutex);
  g_handlers.push_back(handler);
}

void UnregisterHandler(SabineOsrHandler* handler) {
  std::lock_guard<std::mutex> lock(g_handlers_mutex);
  g_handlers.erase(std::remove(g_handlers.begin(), g_handlers.end(), handler),
                   g_handlers.end());
}

bool HasRegisteredHandlers() {
  std::lock_guard<std::mutex> lock(g_handlers_mutex);
  return !g_handlers.empty();
}

std::vector<SabineOsrHandler*> SnapshotHandlers() {
  std::lock_guard<std::mutex> lock(g_handlers_mutex);
  return g_handlers;
}

bool TraceEnabled() {
  static const bool enabled = std::getenv("SABINE_TRACE") != nullptr;
  return enabled;
}

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
  return std::atof(value.c_str());
}

}  // namespace sabine_osr
