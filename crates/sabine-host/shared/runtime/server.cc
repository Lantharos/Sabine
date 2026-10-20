#include "runtime/server.h"

#include <fcntl.h>
#include <poll.h>
#include <sys/socket.h>
#include <unistd.h>

#include <cerrno>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <future>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <vector>

#include "app/scheme.h"
#include "app/profile.h"
#include "include/cef_app.h"
#include "include/cef_request_context.h"
#include "include/cef_task.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/registry.h"
#include "osr/handler.h"
#include "runtime/identity.h"

namespace sabine_runtime {
namespace {

constexpr char kRuntimeSwitch[] = "sabine-runtime";
constexpr char kDataDirSwitch[] = "sabine-data-dir";
constexpr char kProfilesSwitch[] = "sabine-profiles";
constexpr char kLeaseSwitch[] = "sabine-runtime-lease";
constexpr char kAppIdSwitch[] = "sabine-app-id";
constexpr char kProtocol[] = "SABINE-RUNTIME 1";
constexpr int kListenFd = 3;
constexpr size_t kRequestLimit = 1 << 20;
constexpr int kAcceptPollMs = 500;
constexpr int kLingerMs = 30000;
constexpr auto kCreateTimeout = std::chrono::seconds(15);
constexpr timeval kReadTimeout = {5, 0};

struct Runtime {
  std::string profiles;
  std::string user_data;
  std::string data_dir;
  std::string lease;
  std::mutex mutex;
  bool stopping = false;
  int serving = 0;
};

Runtime& State() {
  static Runtime runtime;
  return runtime;
}

std::string ProfileDirectory(const std::string& app_id) {
  uint64_t hash = 0xcbf29ce484222325ull;
  const auto mix = [&hash](unsigned char byte) {
    hash ^= byte;
    hash *= 0x100000001b3ull;
  };
  for (const unsigned char byte : app_id) {
    mix(byte);
  }
  mix(0xff);
  char name[17];
  std::snprintf(name, sizeof(name), "%016llx",
                static_cast<unsigned long long>(hash));
  return State().profiles + "/" + name + "/profile";
}

std::string CreateBrowser(CefRefPtr<CefCommandLine> request, const Peer& peer) {
  CEF_REQUIRE_UI_THREAD();
  BrowserProfile profile{
      nullptr, ProfileDirectory(request->GetSwitchValue(kAppIdSwitch)),
      State().user_data, peer.cgroup};
  if (!sabine_profile::ClaimForSharedRuntime(profile.directory)) {
    return "the app's profile is open in another Chromium";
  }
  const std::string cache_path = sabine_profile::CachePath(profile, "browser");
  if (cache_path.empty()) {
    return "the app's profile is unavailable";
  }
  CefRequestContextSettings settings;
  CefString(&settings.cache_path).FromString(cache_path);
  profile.context = CefRequestContext::CreateContext(settings, nullptr);
  const std::string app_root = request->GetSwitchValue("sabine-app-root");
  if (!app_root.empty()) {
    sabine_app::ServeAppFiles(app_root, profile.context);
  }
  if (request->HasSwitch("sabine-local-files")) {
    sabine_app::ServeLocalFiles(profile.context);
  }
  return CreateSabineOsrBrowser(request, profile)
             ? std::string()
             : "the browser could not be created";
}

class CreateBrowserTask : public CefTask {
 public:
  CreateBrowserTask(CefRefPtr<CefCommandLine> request, Peer peer)
      : request_(request), peer_(std::move(peer)) {}

  void Execute() override {
    created_.set_value(CreateBrowser(request_, peer_));
  }

  std::future<std::string> Created() { return created_.get_future(); }

 private:
  CefRefPtr<CefCommandLine> request_;
  const Peer peer_;
  std::promise<std::string> created_;
  IMPLEMENT_REFCOUNTING(CreateBrowserTask);
};

class QuitWhenIdleTask : public CefTask {
 public:
  void Execute() override {
    if (sabine_osr::HasRegisteredHandlers()) {
      return;
    }
    Runtime& runtime = State();
    {
      std::lock_guard<std::mutex> lock(runtime.mutex);
      if (runtime.serving > 0) {
        return;
      }
      runtime.stopping = true;
    }
    CefQuitMessageLoop();
  }
  IMPLEMENT_REFCOUNTING(QuitWhenIdleTask);
};

bool ReadRequest(int connection, std::vector<std::string>* lines) {
  setsockopt(connection, SOL_SOCKET, SO_RCVTIMEO, &kReadTimeout,
             sizeof(kReadTimeout));
  std::string request;
  char buffer[16384];
  while (request.size() < kRequestLimit &&
         request.find("\n\n") == std::string::npos) {
    const ssize_t length = recv(connection, buffer, sizeof(buffer), 0);
    if (length <= 0) {
      return false;
    }
    request.append(buffer, static_cast<size_t>(length));
  }
  const size_t end = request.find("\n\n");
  if (end == std::string::npos) {
    return false;
  }
  size_t start = 0;
  while (start < end) {
    const size_t line_end = request.find('\n', start);
    lines->push_back(request.substr(start, line_end - start));
    start = line_end + 1;
  }
  return true;
}

CefRefPtr<CefCommandLine> ParseSwitches(const std::vector<std::string>& lines) {
  CefRefPtr<CefCommandLine> command_line = CefCommandLine::CreateCommandLine();
  for (size_t index = 1; index < lines.size(); ++index) {
    const std::string& line = lines[index];
    if (line.rfind("--", 0) != 0) {
      return nullptr;
    }
    const size_t equals = line.find('=');
    if (equals == std::string::npos) {
      command_line->AppendSwitch(line.substr(2));
    } else {
      command_line->AppendSwitchWithValue(line.substr(2, equals - 2),
                                          line.substr(equals + 1));
    }
  }
  return command_line;
}

std::string Answer(int connection) {
  std::vector<std::string> lines;
  if (!ReadRequest(connection, &lines) || lines.empty() ||
      lines.front() != kProtocol) {
    return "this runtime speaks another protocol";
  }
  CefRefPtr<CefCommandLine> request = ParseSwitches(lines);
  if (!request) {
    return "the request is malformed";
  }
  const std::optional<Peer> peer = VerifyPeer(
      connection, request->GetSwitchValue(kAppIdSwitch), State().data_dir);
  if (!peer) {
    return "the app could not be identified";
  }
  CefRefPtr<CreateBrowserTask> task(new CreateBrowserTask(request, *peer));
  std::future<std::string> created = task->Created();
  if (!CefPostTask(TID_UI, task.get()) ||
      created.wait_for(kCreateTimeout) != std::future_status::ready) {
    return "the browser could not be created";
  }
  return created.get();
}

void Respond(int connection) {
  const std::string error = Answer(connection);
  const std::string reply = error.empty() ? "ok\n" : "error\t" + error + "\n";
  send(connection, reply.data(), reply.size(), MSG_NOSIGNAL);
}

void Listen() {
  Runtime& runtime = State();
  while (true) {
    pollfd listener = {kListenFd, POLLIN, 0};
    const int ready = poll(&listener, 1, kAcceptPollMs);
    {
      std::lock_guard<std::mutex> lock(runtime.mutex);
      if (runtime.stopping) {
        return;
      }
    }
    if (ready <= 0) {
      continue;
    }
    const int connection = accept4(kListenFd, nullptr, nullptr, SOCK_CLOEXEC);
    if (connection < 0) {
      if (errno == EINTR || errno == EAGAIN || errno == ECONNABORTED) {
        continue;
      }
      return;
    }
    {
      std::lock_guard<std::mutex> lock(runtime.mutex);
      if (runtime.stopping) {
        close(connection);
        return;
      }
      ++runtime.serving;
    }
    Respond(connection);
    close(connection);
    {
      std::lock_guard<std::mutex> lock(runtime.mutex);
      --runtime.serving;
    }
    WindowsClosed();
  }
}

bool TakeListener() {
  const char* listen_pid = std::getenv("LISTEN_PID");
  const char* listen_fds = std::getenv("LISTEN_FDS");
  const bool ours = listen_pid && listen_fds &&
                    std::atoi(listen_pid) == static_cast<int>(getpid()) &&
                    std::atoi(listen_fds) >= 1;
  unsetenv("LISTEN_PID");
  unsetenv("LISTEN_FDS");
  unsetenv("LISTEN_FDNAMES");
  return ours && fcntl(kListenFd, F_SETFD, FD_CLOEXEC) == 0;
}

}  // namespace

bool IsSharedRuntime(CefRefPtr<CefCommandLine> command_line) {
  return command_line->HasSwitch(kRuntimeSwitch);
}

void Serve(CefRefPtr<CefCommandLine> command_line) {
  CEF_REQUIRE_UI_THREAD();
  Runtime& runtime = State();
  runtime.user_data = command_line->GetSwitchValue("root-cache-path");
  runtime.profiles = command_line->GetSwitchValue(kProfilesSwitch);
  runtime.data_dir = command_line->GetSwitchValue(kDataDirSwitch);
  runtime.lease = command_line->GetSwitchValue(kLeaseSwitch);
  if (!TakeListener() || runtime.profiles.empty() ||
      runtime.user_data.empty() || runtime.data_dir.empty()) {
    std::fprintf(stderr, "Sabine runtime: started without its socket\n");
    CefQuitMessageLoop();
    return;
  }
  if (!runtime.lease.empty()) {
    std::ofstream(runtime.lease) << "pid=" << getpid() << "\n";
  }
  std::thread(Listen).detach();
  WindowsClosed();
}

void WindowsClosed() {
  CefPostDelayedTask(TID_UI, new QuitWhenIdleTask(), kLingerMs);
}

void Stop() {
  Runtime& runtime = State();
  {
    std::lock_guard<std::mutex> lock(runtime.mutex);
    runtime.stopping = true;
  }
  if (!runtime.lease.empty()) {
    std::remove(runtime.lease.c_str());
  }
}

}  // namespace sabine_runtime
