#ifndef SABINE_RUNTIME_SERVER_H_
#define SABINE_RUNTIME_SERVER_H_

#include "include/cef_command_line.h"

namespace sabine_runtime {

// Whether this host is the shared runtime, which serves windows of every app
// that asks on the socket systemd hands it, each in its app's own profile.
bool IsSharedRuntime(CefRefPtr<CefCommandLine> command_line);

// Starts serving apps. Runs on the UI thread once Chromium is initialized.
void Serve(CefRefPtr<CefCommandLine> command_line);

// Quits once no window has been open for a while, so the next launch of a
// newer runtime takes over from this one.
void WindowsClosed();

void Stop();

}  // namespace sabine_runtime

#endif
