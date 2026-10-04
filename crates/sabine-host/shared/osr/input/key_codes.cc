#include "osr/input/key_codes.h"

#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <string_view>
#include <utility>

namespace sabine_osr {

namespace {

constexpr int kKeyCodeSemicolon = 0xBA;
constexpr int kKeyCodeEquals = 0xBB;
constexpr int kKeyCodeComma = 0xBC;
constexpr int kKeyCodeMinus = 0xBD;
constexpr int kKeyCodePeriod = 0xBE;
constexpr int kKeyCodeSlash = 0xBF;
constexpr int kKeyCodeBacktick = 0xC0;
constexpr int kKeyCodeOpenBracket = 0xDB;
constexpr int kKeyCodeBackslash = 0xDC;
constexpr int kKeyCodeCloseBracket = 0xDD;
constexpr int kKeyCodeQuote = 0xDE;

int KeyCodeForCharacter(unsigned char c) {
  if (c >= 'a' && c <= 'z') {
    return c - 'a' + 'A';
  }
  if ((c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == ' ') {
    return c;
  }
  switch (c) {
    case ')':
      return '0';
    case '!':
      return '1';
    case '@':
      return '2';
    case '#':
      return '3';
    case '$':
      return '4';
    case '%':
      return '5';
    case '^':
      return '6';
    case '&':
      return '7';
    case '*':
      return '8';
    case '(':
      return '9';
    case ';':
    case ':':
      return kKeyCodeSemicolon;
    case '=':
    case '+':
      return kKeyCodeEquals;
    case ',':
    case '<':
      return kKeyCodeComma;
    case '-':
    case '_':
      return kKeyCodeMinus;
    case '.':
    case '>':
      return kKeyCodePeriod;
    case '/':
    case '?':
      return kKeyCodeSlash;
    case '`':
    case '~':
      return kKeyCodeBacktick;
    case '[':
    case '{':
      return kKeyCodeOpenBracket;
    case '\\':
    case '|':
      return kKeyCodeBackslash;
    case ']':
    case '}':
      return kKeyCodeCloseBracket;
    case '\'':
    case '"':
      return kKeyCodeQuote;
    default:
      return 0;
  }
}

constexpr std::pair<std::string_view, int> kNamedKeyCodes[] = {
    {"Backspace", 0x08},   {"Tab", 0x09},         {"Enter", 0x0D},
    {"Shift", 0x10},       {"Control", 0x11},     {"Alt", 0x12},
    {"Pause", 0x13},       {"CapsLock", 0x14},    {"Escape", 0x1B},
    {"Space", 0x20},       {"PageUp", 0x21},      {"PageDown", 0x22},
    {"End", 0x23},         {"Home", 0x24},        {"ArrowLeft", 0x25},
    {"ArrowUp", 0x26},     {"ArrowRight", 0x27},  {"ArrowDown", 0x28},
    {"PrintScreen", 0x2C}, {"Insert", 0x2D},      {"Delete", 0x2E},
    {"Meta", 0x5B},        {"ContextMenu", 0x5D}, {"NumLock", 0x90},
    {"ScrollLock", 0x91},  {"AltGraph", 0xE1},
};

}  // namespace

int KeyCodeForName(const std::string& key) {
  if (key.size() == 1) {
    return KeyCodeForCharacter(static_cast<unsigned char>(key[0]));
  }
  if (key.rfind("Key", 0) == 0 && key.size() == 4) {
    return key[3];
  }
  for (const auto& [name, code] : kNamedKeyCodes) {
    if (key == name) {
      return code;
    }
  }
  if (key.size() >= 2 && key[0] == 'F') {
    const std::string number = key.substr(1);
    if (!number.empty() &&
        std::all_of(number.begin(), number.end(),
                    [](unsigned char c) { return std::isdigit(c); })) {
      const int function_key = std::atoi(number.c_str());
      if (function_key >= 1 && function_key <= 24) {
        return 111 + function_key;
      }
    }
  }
  return 0;
}

cef_mouse_button_type_t MouseButtonFromString(const std::string& value) {
  if (value == "right")
    return MBT_RIGHT;
  if (value == "middle")
    return MBT_MIDDLE;
  return MBT_LEFT;
}

}  // namespace sabine_osr
