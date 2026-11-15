#ifndef SABINE_CEF_HOST_OSR_PAGE_PERMISSIONS_H_
#define SABINE_CEF_HOST_OSR_PAGE_PERMISSIONS_H_

#include <cstdint>
#include <functional>
#include <map>
#include <string>
#include <vector>

#include "osr/transport/transport.h"

// The names of the permissions in a mask of cef_permission_request_types_t,
// or nothing when it holds one Sabine never grants.
std::vector<std::string> PromptPermissions(uint32_t requested);
// The names of the permissions in a mask of
// cef_media_access_permission_types_t.
std::vector<std::string> MediaPermissions(uint32_t requested);

// What a page asks for, once the app allows it to ask.
struct PermissionAsk {
  int browser_id = 0;
  // Chromium's id of the prompt, or 0 for camera, microphone and screen
  // requests, which have none.
  uint64_t prompt_id = 0;
  std::string origin;
  // The app's own document asks, rather than a guest.
  bool app = false;
  std::string guest;
  std::vector<std::string> permissions;
};

// Permission requests waiting for the window, which asks the app and the
// person, or remembers what they chose.
class PagePermissions {
 public:
  using Answer = std::function<void(bool allowed)>;

  explicit PagePermissions(sabine_osr::Transport& transport)
      : transport_(transport) {}

  void Ask(const PermissionAsk& ask, Answer answer);
  void Finish(uint32_t id, bool allowed);
  // Chromium took back the prompt, as when its page navigated away.
  void Dismiss(uint64_t prompt_id);
  void Forget(int browser_id);

 private:
  struct Pending {
    int browser_id;
    uint64_t prompt_id;
    Answer answer;
  };

  void Withdraw(std::map<uint32_t, Pending>::iterator request);

  sabine_osr::Transport& transport_;
  std::map<uint32_t, Pending> pending_;
  uint32_t serial_ = 0;
};

#endif
