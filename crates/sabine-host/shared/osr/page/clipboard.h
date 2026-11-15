#ifndef SABINE_CEF_HOST_OSR_PAGE_CLIPBOARD_H_
#define SABINE_CEF_HOST_OSR_PAGE_CLIPBOARD_H_

#include <cstdint>
#include <map>
#include <optional>
#include <string>
#include <string_view>

#include "include/cef_browser.h"
#include "include/cef_values.h"
#include "osr/transport/transport.h"

constexpr char kClipboardCommandPrefix[] = "sabine.clipboard.";

// Clipboard reads and writes pages asked for, waiting for the window.
class ClipboardRequests {
 public:
  explicit ClipboardRequests(sabine_osr::Transport& transport)
      : transport_(transport) {}

  // Hands the window a page's clipboard request, trusted when `policy` lets
  // the page's document use the bridge.
  void Forward(CefRefPtr<CefBrowser> browser,
               CefRefPtr<CefFrame> frame,
               const std::string& request,
               std::optional<std::string_view> body,
               CefRefPtr<CefDictionaryValue> policy);
  // Answers the page that asked, when `request_id` is one of its requests.
  bool Resolve(const std::string& request_id,
               bool ok,
               const std::string& payload,
               const std::optional<std::string>& body);
  void Forget(CefRefPtr<CefBrowser> browser);

 private:
  struct Request {
    CefRefPtr<CefBrowser> browser;
    CefString frame;
    int page_request;
  };

  sabine_osr::Transport& transport_;
  std::map<std::string, Request> pending_;
  uint64_t serial_ = 0;
};

#endif
