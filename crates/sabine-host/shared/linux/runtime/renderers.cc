#include "linux/runtime/renderers.h"

#include <unistd.h>

#include <algorithm>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <set>
#include <sstream>
#include <vector>

#include "include/base/cef_callback.h"
#include "include/cef_task.h"
#include "include/wrapper/cef_closure_task.h"
#include "include/wrapper/cef_helpers.h"

namespace sabine_runtime {
namespace {

constexpr char kCgroupRoot[] = "/sys/fs/cgroup";

std::string ReadAll(const std::string& path) {
  std::ifstream input(path, std::ios::in | std::ios::binary);
  std::ostringstream contents;
  contents << input.rdbuf();
  return contents.str();
}

std::vector<pid_t> Children(pid_t pid) {
  std::vector<pid_t> children;
  std::error_code error;
  const std::filesystem::path tasks = "/proc/" + std::to_string(pid) + "/task";
  for (const auto& task : std::filesystem::directory_iterator(tasks, error)) {
    std::istringstream listed(ReadAll((task.path() / "children").string()));
    pid_t child = 0;
    while (listed >> child) {
      children.push_back(child);
    }
  }
  return children;
}

// Chromium rewrites a child's process title into one line, so its arguments
// may be separated by spaces instead of the kernel's NULs.
bool IsRenderer(pid_t pid, const std::string& client_switch) {
  std::string arguments = ReadAll("/proc/" + std::to_string(pid) + "/cmdline");
  std::replace(arguments.begin(), arguments.end(), '\0', ' ');
  bool renderer = false;
  bool client = false;
  std::istringstream split(arguments);
  std::string argument;
  while (split >> argument) {
    renderer |= argument == "--type=renderer";
    client |= argument == client_switch;
  }
  return renderer && client;
}

// Chromium names a frame "<renderer client id in hex>-<frame token>".
int ClientId(CefRefPtr<CefFrame> frame) {
  const std::string identifier = frame->GetIdentifier();
  char* end = nullptr;
  const long client_id = std::strtol(identifier.c_str(), &end, 16);
  return *end == '-' ? static_cast<int>(client_id) : 0;
}

pid_t FindRenderer(int client_id) {
  const std::string client_switch =
      "--renderer-client-id=" + std::to_string(client_id);
  std::vector<pid_t> pending = Children(getpid());
  while (!pending.empty()) {
    const pid_t pid = pending.back();
    pending.pop_back();
    if (IsRenderer(pid, client_switch)) {
      return pid;
    }
    const std::vector<pid_t> children = Children(pid);
    pending.insert(pending.end(), children.begin(), children.end());
  }
  return 0;
}

void MoveRenderer(int client_id, const std::string& cgroup) {
  const pid_t renderer = FindRenderer(client_id);
  if (renderer <= 0) {
    return;
  }
  std::ofstream procs(std::string(kCgroupRoot) + cgroup + "/cgroup.procs");
  procs << renderer << std::flush;
}

}  // namespace

void AdoptRenderer(CefRefPtr<CefFrame> frame, const std::string& cgroup) {
  CEF_REQUIRE_UI_THREAD();
  static std::set<int> adopted;
  const int client_id = ClientId(frame);
  if (client_id <= 0 || !adopted.insert(client_id).second) {
    return;
  }
  CefPostTask(TID_FILE_USER_VISIBLE,
              base::BindOnce(&MoveRenderer, client_id, cgroup));
}

}  // namespace sabine_runtime
