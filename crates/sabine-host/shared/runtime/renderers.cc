#include "runtime/renderers.h"

#include <unistd.h>

#include <algorithm>

#include <filesystem>
#include <fstream>
#include <sstream>
#include <thread>
#include <vector>

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

pid_t FindRenderer(const std::string& client_id) {
  const std::string client_switch = "--renderer-client-id=" + client_id;
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

}  // namespace

void AdoptRenderer(const std::string& client_id, const std::string& cgroup) {
  std::thread([client_id, cgroup] {
    const pid_t renderer = FindRenderer(client_id);
    if (renderer <= 0) {
      return;
    }
    std::ofstream procs(std::string(kCgroupRoot) + cgroup + "/cgroup.procs");
    procs << renderer << std::flush;
  }).detach();
}

}  // namespace sabine_runtime
