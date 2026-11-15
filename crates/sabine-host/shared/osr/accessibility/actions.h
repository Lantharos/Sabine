#ifndef SABINE_CEF_HOST_OSR_ACCESSIBILITY_ACTIONS_H_
#define SABINE_CEF_HOST_OSR_ACCESSIBILITY_ACTIONS_H_

#include <map>
#include <string>
#include <vector>

#include "include/cef_browser.h"
#include "include/cef_devtools_message_observer.h"
#include "include/cef_registration.h"

// What assistive technology asks of the page's elements. Chromium takes
// accessibility actions only from its own platform accessibility, which a
// windowless browser lacks, so they reach the elements through the page's
// DevTools session, which addresses an element by the DOM node id its
// accessibility node carries.
class AccessibilityActions : public CefDevToolsMessageObserver {
 public:
  void Perform(CefRefPtr<CefBrowser> browser,
               const std::vector<std::string>& parts);
  void Forget() { registration_ = nullptr; }

  void OnDevToolsMethodResult(CefRefPtr<CefBrowser> browser,
                              int message_id,
                              bool success,
                              const void* result,
                              size_t result_size) override;

 private:
  // A script run on one or more elements: the first is `this`, the others
  // follow the script's literal arguments.
  struct ElementCall {
    std::string script;
    CefRefPtr<CefListValue> arguments;
    std::vector<int> nodes;
    std::vector<std::string> objects;
    std::string object_group;
  };

  void Send(CefRefPtr<CefBrowser> browser,
            const std::string& method,
            CefRefPtr<CefDictionaryValue> params,
            ElementCall* call);
  void Call(CefRefPtr<CefBrowser> browser, ElementCall call);
  void Continue(CefRefPtr<CefBrowser> browser, ElementCall call);

  CefRefPtr<CefRegistration> registration_;
  std::map<int, ElementCall> pending_;
  int next_message_id_ = 1 << 29;
  int next_object_group_ = 0;

  IMPLEMENT_REFCOUNTING(AccessibilityActions);
};

#endif
