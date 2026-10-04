#ifndef SABINE_CEF_HOST_OSR_UTILITIES_H_
#define SABINE_CEF_HOST_OSR_UTILITIES_H_

#include <cstdint>
#include <mutex>
#include <set>
#include <string>
#include <vector>

#include "include/cef_command_line.h"
#include "include/internal/cef_types.h"

class SabineOsrHandler;
enum class PaintSurface;

namespace sabine_osr {

extern SabineOsrHandler* g_instance;
extern std::mutex g_handlers_mutex;
extern std::vector<SabineOsrHandler*> g_handlers;
extern const size_t kSharedPaintThreshold;
extern const size_t kBatchEntryLen;

struct PaintRectBytes {
  int x = 0;
  int y = 0;
  int width = 0;
  int height = 0;
  uint64_t offset = 0;
  uint32_t len = 0;
};

void RegisterHandler(SabineOsrHandler* handler);
void UnregisterHandler(SabineOsrHandler* handler);
bool HasRegisteredHandlers();
std::vector<SabineOsrHandler*> SnapshotHandlers();

bool TraceEnabled();
int SwitchInt(CefRefPtr<CefCommandLine> command_line,
              const std::string& name,
              int fallback);
float SwitchFloat(CefRefPtr<CefCommandLine> command_line,
                  const std::string& name,
                  float fallback);
std::vector<std::string> Split(const std::string& value, char separator);
std::string DecodeControlComponent(const std::string& value);
std::string HtmlEscape(const std::string& value);
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

void PutU32(std::vector<char>* buffer, size_t offset, uint32_t value);
void PutI32(std::vector<char>* buffer, size_t offset, int32_t value);
void PutU64(std::vector<char>* buffer, size_t offset, uint64_t value);
bool SendAll(intptr_t fd, const char* bytes, size_t len);
void PutPaintEntry(std::vector<char>* payload,
                   size_t offset,
                   const PaintRectBytes& rect);
std::vector<char> PaintMetadata(const std::string& prefix,
                                const std::vector<PaintRectBytes>& rects);
std::vector<char> PaintMetadata(const std::string& prefix,
                                const std::vector<PaintRectBytes>& rects,
                                uint32_t slot,
                                uint32_t generation);
void CopyPaintRect(char* destination,
                   const void* buffer,
                   int buffer_width,
                   const PaintRectBytes& rect);
uint32_t BatchKind(PaintSurface surface);
uint32_t SharedBatchKind(PaintSurface surface);

int KeyCodeForName(const std::string& key);
std::u16string Utf8ToUtf16(const std::string& value);
cef_mouse_button_type_t MouseButtonFromString(const std::string& value);

}  // namespace sabine_osr

#endif
