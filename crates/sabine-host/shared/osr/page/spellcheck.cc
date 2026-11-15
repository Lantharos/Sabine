#include "osr/page/spellcheck.h"

#include <algorithm>
#include <cctype>

#include "common/json.h"
#include "include/cef_parser.h"
#include "include/wrapper/cef_helpers.h"

namespace {

constexpr char kEnabledPreference[] = "browser.enable_spellchecking";
constexpr char kLanguagesPreference[] = "spellcheck.dictionaries";
constexpr size_t kMaxLanguages = 16;
// Chromium keeps no longer words in the custom dictionary.
constexpr size_t kMaxWordBytes = 99;

bool IsLanguageTag(const std::string& value) {
  size_t start = 0;
  for (size_t part = 0; start <= value.size(); ++part) {
    const size_t end = std::min(value.find('-', start), value.size());
    const std::string piece = value.substr(start, end - start);
    const bool letters = std::all_of(piece.begin(), piece.end(), [](char c) {
      return std::isalpha(static_cast<unsigned char>(c));
    });
    const bool alphanumeric = std::all_of(
        piece.begin(), piece.end(),
        [](char c) { return std::isalnum(static_cast<unsigned char>(c)); });
    const bool valid =
        part == 0 ? piece.size() >= 2 && piece.size() <= 3 && letters
                  : piece.size() >= 2 && piece.size() <= 8 && alphanumeric;
    if (!valid) {
      return false;
    }
    start = end + 1;
  }
  return true;
}

bool IsWord(const std::string& word) {
  return !word.empty() && word.size() <= kMaxWordBytes &&
         std::none_of(word.begin(), word.end(), [](char c) {
           return std::isspace(static_cast<unsigned char>(c)) ||
                  std::iscntrl(static_cast<unsigned char>(c));
         });
}

CefRefPtr<CefDictionaryValue> ParseObject(const std::string& json) {
  CefRefPtr<CefValue> value = CefParseJSON(json, JSON_PARSER_RFC);
  return value && value->GetType() == VTYPE_DICTIONARY
             ? value->GetDictionary()
             : CefDictionaryValue::Create();
}

std::string SettingsJson(CefRefPtr<CefRequestContext> context) {
  CefRefPtr<CefValue> enabled = context->GetPreference(kEnabledPreference);
  CefRefPtr<CefValue> languages = context->GetPreference(kLanguagesPreference);
  std::string list;
  if (languages && languages->GetType() == VTYPE_LIST) {
    CefRefPtr<CefListValue> values = languages->GetList();
    for (size_t index = 0; index < values->GetSize(); ++index) {
      list += (list.empty() ? "" : ",") + JsString(values->GetString(index));
    }
  }
  const bool on =
      enabled && enabled->GetType() == VTYPE_BOOL && enabled->GetBool();
  return std::string("{\"enabled\":") + (on ? "true" : "false") +
         ",\"languages\":[" + list + "]}";
}

}  // namespace

bool ApplySpellcheck(CefRefPtr<CefRequestContext> context,
                     CefRefPtr<CefDictionaryValue> settings,
                     std::string* error) {
  CEF_REQUIRE_UI_THREAD();
  CefString failure;
  if (settings->GetType("languages") == VTYPE_LIST) {
    CefRefPtr<CefListValue> languages = settings->GetList("languages");
    if (languages->GetSize() > kMaxLanguages) {
      *error = "spellcheck takes at most 16 languages";
      return false;
    }
    for (size_t index = 0; index < languages->GetSize(); ++index) {
      if (languages->GetType(index) != VTYPE_STRING ||
          !IsLanguageTag(languages->GetString(index))) {
        *error = "spellcheck languages are language tags such as en-US";
        return false;
      }
    }
    CefRefPtr<CefValue> value = CefValue::Create();
    value->SetList(languages->Copy());
    if (!context->SetPreference(kLanguagesPreference, value, failure)) {
      *error = failure.ToString();
      return false;
    }
  }
  if (settings->GetType("enabled") == VTYPE_BOOL) {
    CefRefPtr<CefValue> value = CefValue::Create();
    value->SetBool(settings->GetBool("enabled"));
    if (!context->SetPreference(kEnabledPreference, value, failure)) {
      *error = failure.ToString();
      return false;
    }
  }
  return true;
}

bool RunSpellcheckCommand(CefRefPtr<CefBrowser> browser,
                          const std::string& command,
                          const std::string& payload,
                          std::string* result,
                          std::string* error) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefRequestContext> context =
      browser->GetHost()->GetRequestContext();
  const std::string operation =
      command.substr(sizeof(kSpellcheckCommandPrefix) - 1);
  if (operation == "addWord") {
    const std::string word = JsonObject::Parse(payload).String("word");
    if (!IsWord(word)) {
      *error = "spellcheck words are one word of up to 99 bytes";
      return false;
    }
    browser->GetHost()->AddWordToDictionary(word);
    *result = "null";
    return true;
  }
  if (operation == "set" &&
      !ApplySpellcheck(context, ParseObject(payload), error)) {
    return false;
  }
  if (operation != "set" && operation != "get") {
    *error = "sabine.spellcheck takes get, set and addWord";
    return false;
  }
  *result = SettingsJson(context);
  return true;
}
