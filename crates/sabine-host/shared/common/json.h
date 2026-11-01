#ifndef SABINE_CEF_HOST_COMMON_JSON_H_
#define SABINE_CEF_HOST_COMMON_JSON_H_

#include <string>
#include <vector>

#include "include/cef_values.h"

std::string JsonEscape(const std::string& value);
std::string JsString(const std::string& value);
std::string JsonMessage(const std::string& message);

// The fields of a JSON object, read with the value each field falls back to
// when the object lacks it or holds another type there.
class JsonObject {
 public:
  static JsonObject Parse(const std::string& json);

  std::string String(const char* key) const;
  bool Bool(const char* key, bool fallback) const;
  int Int(const char* key, int fallback) const;
  double Number(const char* key, double fallback) const;
  std::vector<std::string> Strings(const char* key) const;
  JsonObject Object(const char* key) const;

 private:
  explicit JsonObject(CefRefPtr<CefDictionaryValue> fields);

  CefRefPtr<CefDictionaryValue> fields_;
};

#endif
