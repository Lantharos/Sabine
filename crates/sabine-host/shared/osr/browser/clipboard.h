#ifndef SABINE_CEF_HOST_OSR_BROWSER_CLIPBOARD_H_
#define SABINE_CEF_HOST_OSR_BROWSER_CLIPBOARD_H_

#include <cstdint>
#include <map>
#include <string>

#include "include/cef_browser.h"

constexpr char kClipboardCommandPrefix[] = "sabine.clipboard.";

// Clipboard reads and writes pages asked for, waiting for the window.
struct ClipboardRequests {
  struct Request {
    CefRefPtr<CefBrowser> browser;
    CefString frame;
    int page_request;
  };

  std::map<std::string, Request> pending;
  uint64_t serial = 0;

  void Forget(CefRefPtr<CefBrowser> browser);
};

#endif
