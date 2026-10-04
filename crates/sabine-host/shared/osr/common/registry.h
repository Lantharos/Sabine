#ifndef SABINE_CEF_HOST_OSR_COMMON_REGISTRY_H_
#define SABINE_CEF_HOST_OSR_COMMON_REGISTRY_H_

#include <mutex>
#include <string>
#include <vector>

#include "include/cef_command_line.h"

class SabineOsrHandler;

namespace sabine_osr {

extern SabineOsrHandler* g_instance;
extern std::mutex g_handlers_mutex;
extern std::vector<SabineOsrHandler*> g_handlers;

void RegisterHandler(SabineOsrHandler* handler);
void UnregisterHandler(SabineOsrHandler* handler);
bool HasRegisteredHandlers();
std::vector<SabineOsrHandler*> SnapshotHandlers();

bool TraceEnabled();
int SwitchInt(CefRefPtr<CefCommandLine> command_line,
              const std::string& name,
              int fallback);
float SwitchFloat(CefRefPtr<CefCommandLine> command_line,
                  const std::string& name,
                  float fallback);

}  // namespace sabine_osr

#endif
