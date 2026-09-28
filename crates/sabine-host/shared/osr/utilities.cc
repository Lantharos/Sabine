#include "osr/handler.h"

#include <algorithm>
#include <cctype>
#include <cerrno>
#include <cmath>
#include <cstdint>
#include <sys/types.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <limits>
#include <set>
#include <sstream>
#include <string>
#include <string_view>
#include <thread>
#include <utility>
#include <vector>

#ifdef _WIN32
#include <winsock2.h>
#include <ws2tcpip.h>
#else
#include <sys/socket.h>
#endif

#include "guest/input.h"
#include "guest/manager.h"
#include "include/cef_app.h"
#include "include/cef_browser.h"
#include "include/cef_request_context_handler.h"
#include "include/cef_task.h"
#include "include/internal/cef_types.h"
#include "include/wrapper/cef_helpers.h"
#include "common/json.h"
#include "osr/utilities.h"

using namespace sabine_osr;

namespace sabine_osr {

SabineOsrHandler* g_instance = nullptr;
std::mutex g_handlers_mutex;
std::vector<SabineOsrHandler*> g_handlers;
const size_t kSharedPaintThreshold = 256 * 1024;
const size_t kBatchEntryLen = 28;
#if defined(SYS_memfd_create) && !defined(MFD_CLOEXEC)
constexpr unsigned int MFD_CLOEXEC = 0x0001U;
#endif

void RegisterHandler(SabineOsrHandler* handler) {
  std::lock_guard<std::mutex> lock(g_handlers_mutex);
  g_handlers.push_back(handler);
}

void UnregisterHandler(SabineOsrHandler* handler) {
  std::lock_guard<std::mutex> lock(g_handlers_mutex);
  g_handlers.erase(std::remove(g_handlers.begin(), g_handlers.end(), handler),
                   g_handlers.end());
}

bool HasRegisteredHandlers() {
  std::lock_guard<std::mutex> lock(g_handlers_mutex);
  return !g_handlers.empty();
}

std::vector<SabineOsrHandler*> SnapshotHandlers() {
  std::lock_guard<std::mutex> lock(g_handlers_mutex);
  return g_handlers;
}

int SwitchInt(CefRefPtr<CefCommandLine> command_line,
              const std::string& name,
              int fallback) {
  const std::string value = command_line->GetSwitchValue(name);
  if (value.empty()) {
    return fallback;
  }
  return std::atoi(value.c_str());
}

float SwitchFloat(CefRefPtr<CefCommandLine> command_line,
                  const std::string& name,
                  float fallback) {
  const std::string value = command_line->GetSwitchValue(name);
  if (value.empty()) {
    return fallback;
  }
  return std::atof(value.c_str());
}

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

void PutU32(std::vector<char>* buffer, size_t offset, uint32_t value) {
  (*buffer)[offset + 0] = static_cast<char>(value & 0xff);
  (*buffer)[offset + 1] = static_cast<char>((value >> 8) & 0xff);
  (*buffer)[offset + 2] = static_cast<char>((value >> 16) & 0xff);
  (*buffer)[offset + 3] = static_cast<char>((value >> 24) & 0xff);
}

void PutI32(std::vector<char>* buffer, size_t offset, int32_t value) {
  PutU32(buffer, offset, static_cast<uint32_t>(value));
}

void PutU64(std::vector<char>* buffer, size_t offset, uint64_t value) {
  for (size_t i = 0; i < 8; ++i) {
    (*buffer)[offset + i] = static_cast<char>((value >> (i * 8)) & 0xff);
  }
}

bool SendAll(intptr_t fd, const char* bytes, size_t len) {
  size_t sent = 0;
  while (sent < len) {
    const int result = send(
#ifdef _WIN32
        static_cast<SOCKET>(fd),
#else
        static_cast<int>(fd),
#endif
        bytes + sent, static_cast<int>(len - sent),
#ifdef _WIN32
        0
#else
        MSG_NOSIGNAL
#endif
    );
    if (result <= 0) {
      return false;
    }
    sent += static_cast<size_t>(result);
  }
  return true;
}

void PutPaintEntry(std::vector<char>* payload,
                   size_t offset,
                   const PaintRectBytes& rect) {
  PutI32(payload, offset + 0, rect.x);
  PutI32(payload, offset + 4, rect.y);
  PutU32(payload, offset + 8, static_cast<uint32_t>(rect.width));
  PutU32(payload, offset + 12, static_cast<uint32_t>(rect.height));
  PutU64(payload, offset + 16, rect.offset);
  PutU32(payload, offset + 24, rect.len);
}

namespace {

std::vector<char> BuildPaintMetadata(const std::string& prefix,
                                     const std::vector<PaintRectBytes>& rects,
                                     size_t slot_len) {
  const size_t entries_start = prefix.size() + slot_len + 4;
  std::vector<char> metadata(entries_start + rects.size() * kBatchEntryLen, 0);
  std::memcpy(metadata.data(), prefix.data(), prefix.size());
  PutU32(&metadata, prefix.size() + slot_len,
         static_cast<uint32_t>(rects.size()));
  for (size_t i = 0; i < rects.size(); ++i) {
    PutPaintEntry(&metadata, entries_start + i * kBatchEntryLen, rects[i]);
  }
  return metadata;
}

}  // namespace

std::vector<char> PaintMetadata(const std::string& prefix,
                                const std::vector<PaintRectBytes>& rects) {
  return BuildPaintMetadata(prefix, rects, 0);
}

std::vector<char> PaintMetadata(const std::string& prefix,
                                const std::vector<PaintRectBytes>& rects,
                                uint32_t slot,
                                uint32_t generation) {
  std::vector<char> metadata = BuildPaintMetadata(prefix, rects, 8);
  PutU32(&metadata, prefix.size(), slot);
  PutU32(&metadata, prefix.size() + 4, generation);
  return metadata;
}

void CopyPaintRect(char* destination,
                   const void* buffer,
                   int buffer_width,
                   const PaintRectBytes& rect) {
  const char* source = static_cast<const char*>(buffer);
  const int source_stride = buffer_width * 4;
  const int row_bytes = rect.width * 4;
  for (int row = 0; row < rect.height; ++row) {
    std::memcpy(
        destination + rect.offset + static_cast<size_t>(row * row_bytes),
        source + (rect.y + row) * source_stride + rect.x * 4, row_bytes);
  }
}

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
    {"Backspace", 0x08},   {"Tab", 0x09},        {"Enter", 0x0D},
    {"Shift", 0x10},       {"Control", 0x11},    {"Alt", 0x12},
    {"Pause", 0x13},       {"CapsLock", 0x14},   {"Escape", 0x1B},
    {"Space", 0x20},       {"PageUp", 0x21},     {"PageDown", 0x22},
    {"End", 0x23},         {"Home", 0x24},       {"ArrowLeft", 0x25},
    {"ArrowUp", 0x26},     {"ArrowRight", 0x27}, {"ArrowDown", 0x28},
    {"PrintScreen", 0x2C}, {"Insert", 0x2D},     {"Delete", 0x2E},
    {"Meta", 0x5B},        {"Super", 0x5B},      {"ContextMenu", 0x5D},
    {"NumLock", 0x90},     {"ScrollLock", 0x91}, {"AltGraph", 0xE1},
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

cef_mouse_button_type_t MouseButtonFromString(const std::string& value) {
  if (value == "right")
    return MBT_RIGHT;
  if (value == "middle")
    return MBT_MIDDLE;
  return MBT_LEFT;
}

uint32_t BatchKind(PaintSurface surface) {
  switch (surface) {
    case PaintSurface::kMain:
      return kMainBatch;
    case PaintSurface::kPopup:
      return kPopupBatch;
    case PaintSurface::kGuest:
      return kGuestBatch;
  }
  return kMainBatch;
}

uint32_t SharedBatchKind(PaintSurface surface) {
  switch (surface) {
    case PaintSurface::kMain:
      return kMainSharedBatch;
    case PaintSurface::kPopup:
      return kPopupSharedBatch;
    case PaintSurface::kGuest:
      return kGuestSharedBatch;
  }
  return kMainSharedBatch;
}

std::string CursorName(cef_cursor_type_t type) {
  switch (type) {
    case CT_HAND:
      return "pointer";
    case CT_IBEAM:
      return "text";
    case CT_CROSS:
      return "crosshair";
    case CT_MOVE:
      return "move";
    case CT_WAIT:
      return "wait";
    case CT_HELP:
      return "help";
    case CT_NOTALLOWED:
    case CT_NODROP:
      return "not-allowed";
    case CT_EASTWESTRESIZE:
    case CT_COLUMNRESIZE:
      return "ew-resize";
    case CT_NORTHSOUTHRESIZE:
    case CT_ROWRESIZE:
      return "ns-resize";
    case CT_NORTHEASTRESIZE:
      return "ne-resize";
    case CT_NORTHWESTRESIZE:
      return "nw-resize";
    case CT_SOUTHEASTRESIZE:
      return "se-resize";
    case CT_SOUTHWESTRESIZE:
      return "sw-resize";
    default:
      return "default";
  }
}

}  // namespace sabine_osr
