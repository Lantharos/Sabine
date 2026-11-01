#ifndef SABINE_CEF_HOST_OSR_COMMON_REGISTRY_H_
#define SABINE_CEF_HOST_OSR_COMMON_REGISTRY_H_

class SabineOsrHandler;

namespace sabine_osr {

// The handlers of this process's windows. The first one still registered is
// the primary handler, which serves browsers Chromium creates on its own.
void RegisterHandler(SabineOsrHandler* handler);
void UnregisterHandler(SabineOsrHandler* handler);
bool HasRegisteredHandlers();
SabineOsrHandler* PrimaryHandler();

}  // namespace sabine_osr

#endif
