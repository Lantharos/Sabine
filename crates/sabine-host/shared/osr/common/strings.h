#ifndef SABINE_CEF_HOST_OSR_COMMON_STRINGS_H_
#define SABINE_CEF_HOST_OSR_COMMON_STRINGS_H_

#include <string>
#include <vector>

namespace sabine_osr {

std::vector<std::string> Split(const std::string& value, char separator);
std::string DecodeControlComponent(const std::string& value);
std::string HtmlEscape(const std::string& value);
std::u16string Utf8ToUtf16(const std::string& value);
bool ParseBridgeResponse(const std::string& line,
                         std::string* browser_id,
                         std::string* request_id,
                         bool* ok,
                         std::string* payload);
bool ParseBridgeEvent(const std::string& line,
                      std::string* name_json,
                      std::string* payload);
bool ParseHostControl(const std::string& line,
                      std::string* command,
                      std::string* value);

}  // namespace sabine_osr

#endif
