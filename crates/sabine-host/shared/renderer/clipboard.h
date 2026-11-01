#ifndef SABINE_RENDERER_CLIPBOARD_H_
#define SABINE_RENDERER_CLIPBOARD_H_
#include "include/cef_frame.h"
#include "include/cef_process_message.h"
#include "include/cef_v8.h"
namespace sabine_clipboard {
void Install(CefRefPtr<CefFrame> frame,
             CefRefPtr<CefV8Context> context,
             bool trusted);
void Release(CefRefPtr<CefV8Context> context);
bool Receive(CefRefPtr<CefFrame> frame, CefRefPtr<CefProcessMessage> message);
}  // namespace sabine_clipboard
#endif
