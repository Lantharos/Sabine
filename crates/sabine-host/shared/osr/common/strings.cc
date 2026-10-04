#include "osr/common/strings.h"

#include <cstdint>
#include <sstream>

namespace sabine_osr {

std::vector<std::string> Split(const std::string& value, char separator) {
  std::vector<std::string> parts;
  std::stringstream stream(value);
  std::string item;
  while (std::getline(stream, item, separator)) {
    parts.push_back(item);
  }
  return parts;
}

std::string DecodeControlComponent(const std::string& value) {
  auto hex_digit = [](char digit) -> int {
    if (digit >= '0' && digit <= '9')
      return digit - '0';
    if (digit >= 'a' && digit <= 'f')
      return digit - 'a' + 10;
    if (digit >= 'A' && digit <= 'F')
      return digit - 'A' + 10;
    return -1;
  };
  std::string decoded;
  decoded.reserve(value.size());
  for (size_t index = 0; index < value.size(); ++index) {
    if (value[index] == '%' && index + 2 < value.size()) {
      const int high = hex_digit(value[index + 1]);
      const int low = hex_digit(value[index + 2]);
      if (high >= 0 && low >= 0) {
        decoded.push_back(static_cast<char>((high << 4) | low));
        index += 2;
        continue;
      }
    }
    decoded.push_back(value[index]);
  }
  return decoded;
}

std::string HtmlEscape(const std::string& value) {
  std::string escaped;
  escaped.reserve(value.size());
  for (const char character : value) {
    switch (character) {
      case '&':
        escaped += "&amp;";
        break;
      case '<':
        escaped += "&lt;";
        break;
      case '>':
        escaped += "&gt;";
        break;
      case '"':
        escaped += "&quot;";
        break;
      case '\'':
        escaped += "&#39;";
        break;
      default:
        escaped += character;
        break;
    }
  }
  return escaped;
}

std::u16string Utf8ToUtf16(const std::string& value) {
  std::u16string output;
  for (size_t i = 0; i < value.size();) {
    uint32_t cp = static_cast<unsigned char>(value[i++]);
    if ((cp & 0x80) == 0) {
    } else if ((cp & 0xe0) == 0xc0 && i < value.size()) {
      const uint32_t b1 = static_cast<unsigned char>(value[i++]);
      cp = ((cp & 0x1f) << 6) | (b1 & 0x3f);
    } else if ((cp & 0xf0) == 0xe0 && i + 1 < value.size()) {
      const uint32_t b1 = static_cast<unsigned char>(value[i++]);
      const uint32_t b2 = static_cast<unsigned char>(value[i++]);
      cp = ((cp & 0x0f) << 12) | ((b1 & 0x3f) << 6) | (b2 & 0x3f);
    } else if ((cp & 0xf8) == 0xf0 && i + 2 < value.size()) {
      const uint32_t b1 = static_cast<unsigned char>(value[i++]);
      const uint32_t b2 = static_cast<unsigned char>(value[i++]);
      const uint32_t b3 = static_cast<unsigned char>(value[i++]);
      cp = ((cp & 0x07) << 18) | ((b1 & 0x3f) << 12) | ((b2 & 0x3f) << 6) |
           (b3 & 0x3f);
    } else {
      continue;
    }
    if (cp <= 0xffff) {
      output.push_back(static_cast<char16_t>(cp));
    } else {
      cp -= 0x10000;
      output.push_back(static_cast<char16_t>(0xd800 + (cp >> 10)));
      output.push_back(static_cast<char16_t>(0xdc00 + (cp & 0x3ff)));
    }
  }
  return output;
}

bool ParseBridgeResponse(const std::string& line,
                         std::string* browser_id,
                         std::string* request_id,
                         bool* ok,
                         std::string* payload) {
  const std::string prefix = "SABINE_BRIDGE_RESPONSE\t";
  if (line.rfind(prefix, 0) != 0) {
    return false;
  }
  std::vector<std::string> parts;
  size_t cursor = prefix.size();
  while (parts.size() < 3) {
    const size_t next = line.find('\t', cursor);
    if (next == std::string::npos) {
      return false;
    }
    parts.push_back(line.substr(cursor, next - cursor));
    cursor = next + 1;
  }
  *browser_id = parts[0];
  *request_id = parts[1];
  *ok = parts[2] == "ok";
  *payload = line.substr(cursor);
  return true;
}

bool ParseBridgeEvent(const std::string& line,
                      std::string* name_json,
                      std::string* payload) {
  const std::string prefix = "SABINE_BRIDGE_EVENT\t";
  if (line.rfind(prefix, 0) != 0) {
    return false;
  }
  const size_t separator = line.find('\t', prefix.size());
  if (separator == std::string::npos) {
    return false;
  }
  *name_json = line.substr(prefix.size(), separator - prefix.size());
  *payload = line.substr(separator + 1);
  return true;
}

bool ParseHostControl(const std::string& line,
                      std::string* command,
                      std::string* value) {
  const std::string prefix = "SABINE_HOST_CONTROL\t";
  if (line.rfind(prefix, 0) != 0) {
    return false;
  }
  const size_t separator = line.find('\t', prefix.size());
  if (separator == std::string::npos) {
    *command = line.substr(prefix.size());
    *value = "{}";
    return !command->empty();
  }
  *command = line.substr(prefix.size(), separator - prefix.size());
  *value = line.substr(separator + 1);
  return !command->empty();
}

}  // namespace sabine_osr
