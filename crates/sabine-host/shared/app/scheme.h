#ifndef SABINE_CEF_HOST_APP_SCHEME_H_
#define SABINE_CEF_HOST_APP_SCHEME_H_

#include <string>

#include "include/cef_request_context.h"
#include "include/cef_scheme.h"

namespace sabine_app {

/// Registers the sabine scheme. Only sabine pages may show its files, except
/// in |development|, where the development server's page shows them as the
/// bundled page will.
void RegisterAppScheme(CefRawPtr<CefSchemeRegistrar> registrar,
                       bool development);

/// Serves the files below |root| at sabine://app/ to pages of |context|, or
/// of every context when it is null.
void ServeAppFiles(const std::string& root,
                   CefRefPtr<CefRequestContext> context = nullptr);

/// Serves any file the user can read at sabine://file/<absolute path>, for
/// apps that opt into local file access, and lets the window's page at
/// |page_url| read them.
void ServeLocalFiles(const std::string& page_url,
                     CefRefPtr<CefRequestContext> context = nullptr);

}  // namespace sabine_app

#endif
