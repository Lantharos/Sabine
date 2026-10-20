#ifndef SABINE_CEF_HOST_APP_PROFILE_H_
#define SABINE_CEF_HOST_APP_PROFILE_H_

#include <string>

#include "include/cef_request_context.h"

// Where a window's pages keep their data: the request context of the app's
// profile, or the browser's own when the app runs its own Chromium, and the
// cgroup its renderers belong to when a shared runtime hosts them.
struct BrowserProfile {
  CefRefPtr<CefRequestContext> context;
  // The app's profile folder, which its own Chromium uses as its user data.
  std::string directory;
  // The shared runtime's user data folder, empty in the app's own Chromium.
  std::string shared;
  std::string renderer_cgroup;
};

namespace sabine_profile {

// The folder Chromium keeps |name| of the profile in, such as "browser" or a
// guest partition. Chromium opens profiles only directly inside its user data
// folder, so the shared runtime keeps them in its own, named after the app's
// profile folder, and moves them there from that folder the first time.
std::string CachePath(const BrowserProfile& profile, const std::string& name);

#if defined(OS_LINUX)
// Holds the app's profile in the shared runtime until it quits. Fails while
// the app's own Chromium or another runtime has the profile open.
bool ClaimForSharedRuntime(const std::string& directory);

// Before the app's own Chromium opens |directory|, takes back what the shared
// runtime moved out of it. Fails while a shared runtime has it open.
bool ReclaimFromSharedRuntime(const std::string& directory);
#endif

}  // namespace sabine_profile

#endif
