#ifndef SABINE_CEF_HOST_APP_SCHEME_H_
#define SABINE_CEF_HOST_APP_SCHEME_H_

#include <string>

#include "include/cef_scheme.h"

namespace sabine_app {

void RegisterAppScheme(CefRawPtr<CefSchemeRegistrar> registrar);

/// Serves the files below |root| at sabine://app/.
void ServeAppFiles(const std::string& root);

}  // namespace sabine_app

#endif
