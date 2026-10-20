#include "runtime/identity.h"

#include <fcntl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

#include <cstdlib>
#include <fstream>
#include <sstream>

#include "include/cef_parser.h"

namespace sabine_runtime {
namespace {

constexpr int kPeerPidfd = 77;
constexpr char kSystemApps[] = "/usr/lib/sabine/";
constexpr char kUnifiedHierarchy[] = "0::";

class Descriptor {
 public:
  explicit Descriptor(int fd) : fd_(fd) {}
  ~Descriptor() {
    if (fd_ >= 0) {
      close(fd_);
    }
  }
  Descriptor(const Descriptor&) = delete;
  Descriptor& operator=(const Descriptor&) = delete;
  int get() const { return fd_; }

 private:
  const int fd_;
};

std::string ReadAll(const std::string& path) {
  std::ifstream input(path, std::ios::in | std::ios::binary);
  std::ostringstream contents;
  contents << input.rdbuf();
  return contents.str();
}

std::string ReadAt(int directory, const char* name) {
  const int fd = openat(directory, name, O_RDONLY | O_CLOEXEC);
  if (fd < 0) {
    return std::string();
  }
  Descriptor file(fd);
  std::string contents;
  char buffer[4096];
  ssize_t length = 0;
  while ((length = read(file.get(), buffer, sizeof(buffer))) > 0) {
    contents.append(buffer, static_cast<size_t>(length));
  }
  return contents;
}

std::string LinkAt(int directory, const char* name) {
  char buffer[4096];
  const ssize_t length =
      readlinkat(directory, name, buffer, sizeof(buffer) - 1);
  return length > 0 ? std::string(buffer, static_cast<size_t>(length))
                    : std::string();
}

int PidOf(int pidfd) {
  std::istringstream info(
      ReadAll("/proc/self/fdinfo/" + std::to_string(pidfd)));
  std::string line;
  while (std::getline(info, line)) {
    if (line.rfind("Pid:", 0) == 0) {
      return std::atoi(line.c_str() + 4);
    }
  }
  return -1;
}

int PeerPidfd(int connection, pid_t pid) {
  int pidfd = -1;
  socklen_t length = sizeof(pidfd);
  if (getsockopt(connection, SOL_SOCKET, kPeerPidfd, &pidfd, &length) == 0 &&
      pidfd >= 0) {
    return pidfd;
  }
  return static_cast<int>(syscall(SYS_pidfd_open, pid, 0));
}

std::string Canonical(const std::string& path) {
  char* resolved = realpath(path.c_str(), nullptr);
  if (!resolved) {
    return std::string();
  }
  std::string canonical(resolved);
  std::free(resolved);
  return canonical;
}

std::string RegisteredExecutable(const std::string& data_dir,
                                 const std::string& app_id) {
  const CefRefPtr<CefValue> registry =
      CefParseJSON(ReadAll(data_dir + "/apps.json"), JSON_PARSER_RFC);
  const CefRefPtr<CefDictionaryValue> apps =
      registry && registry->GetDictionary()
          ? registry->GetDictionary()->GetDictionary("apps")
          : nullptr;
  const CefRefPtr<CefDictionaryValue> app =
      apps ? apps->GetDictionary(app_id) : nullptr;
  return app ? app->GetString("executable").ToString() : std::string();
}

bool InstalledForEveryone(const std::string& executable,
                          const std::string& app_id) {
  struct stat status = {};
  return executable.rfind(std::string(kSystemApps) + app_id + "/", 0) == 0 &&
         stat(executable.c_str(), &status) == 0 && status.st_uid == 0 &&
         (status.st_mode & (S_IWGRP | S_IWOTH)) == 0;
}

std::string UnifiedCgroup(const std::string& cgroups) {
  std::istringstream lines(cgroups);
  std::string line;
  while (std::getline(lines, line)) {
    if (line.rfind(kUnifiedHierarchy, 0) == 0) {
      return line.substr(sizeof(kUnifiedHierarchy) - 1);
    }
  }
  return std::string();
}

}  // namespace

bool ValidAppId(const std::string& app_id) {
  if (app_id.empty() || app_id.size() > 255) {
    return false;
  }
  for (const char character : app_id) {
    const bool allowed = (character >= 'a' && character <= 'z') ||
                         (character >= '0' && character <= '9') ||
                         character == '.' || character == '-';
    if (!allowed) {
      return false;
    }
  }
  return true;
}

std::optional<Peer> VerifyPeer(int connection,
                               const std::string& app_id,
                               const std::string& data_dir) {
  struct ucred credentials = {};
  socklen_t length = sizeof(credentials);
  if (!ValidAppId(app_id) ||
      getsockopt(connection, SOL_SOCKET, SO_PEERCRED, &credentials, &length) !=
          0 ||
      credentials.uid != getuid()) {
    return std::nullopt;
  }
  const Descriptor pidfd(PeerPidfd(connection, credentials.pid));
  if (pidfd.get() < 0 || PidOf(pidfd.get()) != credentials.pid) {
    return std::nullopt;
  }
  const std::string directory = "/proc/" + std::to_string(credentials.pid);
  const Descriptor process(
      open(directory.c_str(), O_PATH | O_DIRECTORY | O_CLOEXEC));
  const std::string executable = LinkAt(process.get(), "exe");
  const std::string cgroup = UnifiedCgroup(ReadAt(process.get(), "cgroup"));
  if (process.get() < 0 || executable.empty() ||
      PidOf(pidfd.get()) != credentials.pid) {
    return std::nullopt;
  }
  const std::string registered =
      Canonical(RegisteredExecutable(data_dir, app_id));
  if ((registered.empty() || registered != executable) &&
      !InstalledForEveryone(executable, app_id)) {
    return std::nullopt;
  }
  return Peer{cgroup};
}

}  // namespace sabine_runtime
