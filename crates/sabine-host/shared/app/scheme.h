#ifndef SABINE_CEF_HOST_APP_SCHEME_H_
#define SABINE_CEF_HOST_APP_SCHEME_H_

#include <string>

#include "include/cef_scheme.h"

namespace sabine_app {

void RegisterAppScheme(CefRawPtr<CefSchemeRegistrar> registrar);

/// Serves the files below |root| at sabine://app/.
void ServeAppFiles(const std::string& root);

/// Serves any file the user can read at sabine://file/<absolute path>, for
/// apps that opt into local file access.
void ServeLocalFiles();

}  // namespace sabine_app

#endif
