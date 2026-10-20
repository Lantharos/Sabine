#ifndef SABINE_CEF_HOST_APP_SCHEME_H_
#define SABINE_CEF_HOST_APP_SCHEME_H_

#include <string>

#include "include/cef_request_context.h"
#include "include/cef_scheme.h"

namespace sabine_app {

void RegisterAppScheme(CefRawPtr<CefSchemeRegistrar> registrar);

/// Serves the files below |root| at sabine://app/ to pages of |context|, or
/// of every context when it is null.
void ServeAppFiles(const std::string& root,
                   CefRefPtr<CefRequestContext> context = nullptr);

/// Serves any file the user can read at sabine://file/<absolute path>, for
/// apps that opt into local file access.
void ServeLocalFiles(CefRefPtr<CefRequestContext> context = nullptr);

}  // namespace sabine_app

#endif
