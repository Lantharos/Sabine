#ifndef SABINE_CEF_HOST_OSR_INPUT_KEY_CODES_H_
#define SABINE_CEF_HOST_OSR_INPUT_KEY_CODES_H_

#include <string>

#include "include/internal/cef_types.h"

namespace sabine_osr {

int KeyCodeForName(const std::string& key);
cef_mouse_button_type_t MouseButtonFromString(const std::string& value);

}  // namespace sabine_osr

#endif
