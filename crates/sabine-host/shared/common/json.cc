#include "common/json.h"

#include <cstdio>
#include <utility>

#include "include/cef_parser.h"

std::string JsonEscape(const std::string& value) {
  std::string output;
  output.reserve(value.size() + 2);
  for (char c : value) {
    switch (c) {
      case '\\':
        output += "\\\\";
        break;
      case '"':
        output += "\\\"";
        break;
      case '\n':
        output += "\\n";
        break;
      case '\r':
        output += "\\r";
        break;
      case '\t':
        output += "\\t";
        break;
      default:
        if (static_cast<unsigned char>(c) < 0x20) {
          char buffer[8];
          std::snprintf(buffer, sizeof(buffer), "\\u%04x",
                        static_cast<unsigned char>(c));
          output += buffer;
        } else {
          output += c;
        }
        break;
    }
  }
  return output;
}

std::string JsString(const std::string& value) {
  return "\"" + JsonEscape(value) + "\"";
}

std::string JsonMessage(const std::string& message) {
  return "{\"message\":" + JsString(message) + "}";
}

JsonObject::JsonObject(CefRefPtr<CefDictionaryValue> fields)
    : fields_(std::move(fields)) {}

JsonObject JsonObject::Parse(const std::string& json) {
  CefRefPtr<CefValue> value = CefParseJSON(json, JSON_PARSER_RFC);
  return JsonObject(value && value->GetType() == VTYPE_DICTIONARY
                        ? value->GetDictionary()
                        : nullptr);
}

std::string JsonObject::String(const char* key) const {
  return fields_ && fields_->GetType(key) == VTYPE_STRING
             ? fields_->GetString(key).ToString()
             : std::string();
}

bool JsonObject::Bool(const char* key, bool fallback) const {
  return fields_ && fields_->GetType(key) == VTYPE_BOOL ? fields_->GetBool(key)
                                                        : fallback;
}

int JsonObject::Int(const char* key, int fallback) const {
  return static_cast<int>(Number(key, fallback));
}

double JsonObject::Number(const char* key, double fallback) const {
  if (!fields_) {
    return fallback;
  }
  switch (fields_->GetType(key)) {
    case VTYPE_INT:
      return fields_->GetInt(key);
    case VTYPE_DOUBLE:
      return fields_->GetDouble(key);
    default:
      return fallback;
  }
}

std::vector<std::string> JsonObject::Strings(const char* key) const {
  std::vector<std::string> values;
  CefRefPtr<CefListValue> list = fields_ && fields_->GetType(key) == VTYPE_LIST
                                     ? fields_->GetList(key)
                                     : nullptr;
  if (!list) {
    return values;
  }
  values.reserve(list->GetSize());
  for (size_t index = 0; index < list->GetSize(); ++index) {
    if (list->GetType(index) == VTYPE_STRING) {
      values.push_back(list->GetString(index));
    }
  }
  return values;
}

JsonObject JsonObject::Object(const char* key) const {
  return JsonObject(fields_ && fields_->GetType(key) == VTYPE_DICTIONARY
                        ? fields_->GetDictionary(key)
                        : nullptr);
}
