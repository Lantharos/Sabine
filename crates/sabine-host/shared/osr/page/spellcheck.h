#ifndef SABINE_CEF_HOST_OSR_PAGE_SPELLCHECK_H_
#define SABINE_CEF_HOST_OSR_PAGE_SPELLCHECK_H_

#include <string>

#include "include/cef_browser.h"
#include "include/cef_request_context.h"
#include "include/cef_values.h"

constexpr char kSpellcheckCommandPrefix[] = "sabine.spellcheck.";
// Replaces the misspelled word a context menu was last asked for.
constexpr char kSpellcheckReplaceCommand[] = "sabine.spellcheck.replace";

// Turns spellchecking on or off and picks its languages in |context|, for
// what |settings| holds of `enabled` and `languages`. Fails with |error| for
// languages that aren't language tags.
bool ApplySpellcheck(CefRefPtr<CefRequestContext> context,
                     CefRefPtr<CefDictionaryValue> settings,
                     std::string* error);

// Answers `sabine.spellcheck.get`, `set` and `addWord` with the JSON the
// page receives, or fails with |error|.
bool RunSpellcheckCommand(CefRefPtr<CefBrowser> browser,
                          const std::string& command,
                          const std::string& payload,
                          std::string* result,
                          std::string* error);

#endif
