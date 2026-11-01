#ifndef SABINE_CEF_HOST_ENTRY_H_
#define SABINE_CEF_HOST_ENTRY_H_

#include "include/cef_app.h"

// Prints the native protocol and returns true when the command line asks
// for it, before any of Chromium loads.
bool PrintsHostProtocol(int argc, char* argv[]);

int RunSabineHost(CefMainArgs main_args,
                  int argc,
                  char* argv[],
                  void* sandbox_info = nullptr);

#endif
