#include "display/private_display.h"

#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

#include <cctype>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <set>
#include <string>
#include <thread>

#include "display/offer.h"
#include "display/protocols.h"
#include "display/wayland.h"

namespace sabine_display {
namespace {

constexpr int kCompositorVersion = 4;
constexpr int kOutputVersion = 2;
constexpr uint32_t kShmFormats[] = {0, 1};
constexpr int kOutputWidth = 1920;
constexpr int kOutputHeight = 1080;
constexpr int kOutputRefresh = 60000;
constexpr int kSeatVersion = 5;
constexpr int kDataDeviceManagerVersion = 3;
constexpr uint32_t kGetKeyboard = 1;
constexpr uint32_t kKeyboardCapability = 2;

using Handler = bool (*)(void* resource,
                         uint32_t opcode,
                         WlArgument* arguments);

const ServerLibrary* g_server = nullptr;
Offer g_offer;
const BufferFormats& g_formats = g_offer.formats;
std::string g_path;

int Dispatch(const void* implementation,
             void* resource,
             uint32_t opcode,
             const WlMessage* message,
             WlArgument* arguments);

void* Create(void* client,
             const WlInterface* interface,
             int version,
             uint32_t id,
             Handler handler = nullptr) {
  void* resource = g_server->resource_create(client, interface, version, id);
  if (!resource) {
    g_server->client_post_no_memory(client);
    return nullptr;
  }
  g_server->resource_set_dispatcher(resource, Dispatch,
                                    reinterpret_cast<const void*>(handler),
                                    nullptr, nullptr);
  return resource;
}

void* CreateBeside(void* parent,
                   const WlInterface* interface,
                   uint32_t id,
                   Handler handler = nullptr) {
  return Create(g_server->resource_get_client(parent), interface,
                g_server->resource_get_version(parent), id, handler);
}

int Dispatch(const void* implementation,
             void* resource,
             uint32_t opcode,
             const WlMessage* message,
             WlArgument* arguments) {
  const auto handler = reinterpret_cast<Handler>(implementation);
  if (handler && handler(resource, opcode, arguments)) {
    return 0;
  }
  int index = 0;
  for (const char* type = message->signature; *type; ++type) {
    if (std::isdigit(*type) || *type == '?') {
      continue;
    }
    if (*type == 'n' && message->types[index]) {
      CreateBeside(resource, message->types[index], arguments[index].n);
    } else if (*type == 'h') {
      close(arguments[index].h);
    }
    ++index;
  }
  if (std::strcmp(message->name, "destroy") == 0 ||
      std::strcmp(message->name, "release") == 0) {
    g_server->resource_destroy(resource);
  }
  return 0;
}

WlArray ArrayOf(const std::vector<uint8_t>& bytes) {
  return {bytes.size(), bytes.size(), const_cast<uint8_t*>(bytes.data())};
}

void SendFeedback(void* feedback_resource) {
  g_server->resource_post_event(feedback_resource, feedback::kFormatTable,
                                g_formats.table_fd, g_formats.table_size);
  WlArray main_device = ArrayOf(g_formats.main_device);
  g_server->resource_post_event(feedback_resource, feedback::kMainDevice,
                                &main_device);
  for (const Tranche& tranche : g_formats.tranches) {
    WlArray device = ArrayOf(tranche.target_device);
    WlArray formats = ArrayOf(tranche.formats);
    g_server->resource_post_event(feedback_resource,
                                  feedback::kTrancheTargetDevice, &device);
    g_server->resource_post_event(feedback_resource, feedback::kTrancheFormats,
                                  &formats);
    g_server->resource_post_event(feedback_resource, feedback::kTrancheFlags,
                                  tranche.flags);
    g_server->resource_post_event(feedback_resource, feedback::kTrancheDone);
  }
  g_server->resource_post_event(feedback_resource, feedback::kDone);
}

bool OnBufferParams(void* resource, uint32_t opcode, WlArgument* arguments) {
  if (opcode == params::kCreate) {
    g_server->resource_post_event(resource, params::kFailedEvent);
    return true;
  }
  if (opcode == params::kCreateImmed) {
    CreateBeside(resource, g_server->buffer_interface, arguments[0].n);
    return true;
  }
  return false;
}

bool OnDmabuf(void* resource, uint32_t opcode, WlArgument* arguments) {
  switch (opcode) {
    case dmabuf::kCreateParams:
      CreateBeside(resource, &kLinuxBufferParams, arguments[0].n,
                   OnBufferParams);
      return true;
    case dmabuf::kGetDefaultFeedback:
    case dmabuf::kGetSurfaceFeedback:
      if (void* feedback =
              CreateBeside(resource, &kLinuxDmabufFeedback, arguments[0].n)) {
        SendFeedback(feedback);
      }
      return true;
  }
  return false;
}

bool OnSeat(void* resource, uint32_t opcode, WlArgument* arguments) {
  if (opcode != kGetKeyboard || !g_offer.keymap) {
    return false;
  }
  if (void* keyboard = CreateBeside(resource, g_server->keyboard_interface,
                                    arguments[0].n)) {
    g_server->resource_post_event(keyboard, 0, g_offer.keymap->format,
                                  g_offer.keymap->fd, g_offer.keymap->size);
  }
  return true;
}

void BindSeat(void* client, void*, uint32_t version, uint32_t id) {
  if (void* seat =
          Create(client, g_server->seat_interface, version, id, OnSeat)) {
    g_server->resource_post_event(seat, 0,
                                  (g_offer.keymap ? kKeyboardCapability : 0u) |
                                      g_offer.pointer_and_touch);
    if (version >= 2) {
      g_server->resource_post_event(seat, 1, "seat0");
    }
  }
}

void BindInert(void* client, void* data, uint32_t version, uint32_t id) {
  Create(client, static_cast<const WlInterface*>(data), version, id);
}

void BindShm(void* client, void*, uint32_t version, uint32_t id) {
  if (void* shm = Create(client, g_server->shm_interface, version, id)) {
    for (uint32_t format : kShmFormats) {
      g_server->resource_post_event(shm, 0, format);
    }
  }
}

void BindOutput(void* client, void*, uint32_t version, uint32_t id) {
  void* output = Create(client, g_server->output_interface, version, id);
  if (!output) {
    return;
  }
  g_server->resource_post_event(output, 0, 0, 0, 0, 0, 0, "Sabine", "Offscreen",
                                0);
  g_server->resource_post_event(output, 1, 3u, kOutputWidth, kOutputHeight,
                                kOutputRefresh);
  g_server->resource_post_event(output, 3, 1);
  g_server->resource_post_event(output, 2);
}

void BindDmabuf(void* client, void*, uint32_t version, uint32_t id) {
  void* dmabuf = Create(client, &kLinuxDmabuf, version, id, OnDmabuf);
  if (!dmabuf || version >= 4) {
    return;
  }
  std::set<uint32_t> announced;
  for (const FormatModifier& entry : g_formats.formats) {
    if (announced.insert(entry.format).second) {
      g_server->resource_post_event(dmabuf, dmabuf::kFormatEvent, entry.format);
    }
    g_server->resource_post_event(dmabuf, dmabuf::kModifierEvent, entry.format,
                                  entry.modifier_high, entry.modifier_low);
  }
}

int Listen(const std::string& path) {
  sockaddr_un address{};
  if (path.size() >= sizeof(address.sun_path)) {
    return -1;
  }
  address.sun_family = AF_UNIX;
  std::memcpy(address.sun_path, path.c_str(), path.size() + 1);
  const int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  if (fd < 0) {
    return -1;
  }
  unlink(path.c_str());
  if (bind(fd, reinterpret_cast<sockaddr*>(&address), sizeof(address)) < 0 ||
      listen(fd, 16) < 0) {
    close(fd);
    return -1;
  }
  return fd;
}

std::string SocketPath() {
  const char* runtime = std::getenv("XDG_RUNTIME_DIR");
  if (!runtime || !*runtime) {
    return {};
  }
  const std::string folder = std::string(runtime) + "/sabine";
  mkdir(folder.c_str(), 0700);
  return folder + "/display-" + std::to_string(getpid());
}

void Serve(void* display) {
  void* loop = g_server->display_get_event_loop(display);
  for (;;) {
    g_server->display_flush_clients(display);
    g_server->event_loop_dispatch(loop, -1);
  }
}

}  // namespace

void UsePrivateDisplay() {
  g_server = LoadServer();
  const std::string path = SocketPath();
  if (!g_server || path.empty()) {
    return;
  }
  std::optional<Offer> offer = QueryOffer();
  if (!offer) {
    return;
  }
  void* display = g_server->display_create();
  const int fd = display ? Listen(path) : -1;
  if (fd < 0 || g_server->display_add_socket_fd(display, fd) < 0) {
    std::cerr << "Sabine CEF: the browser keeps the desktop's display"
              << std::endl;
    return;
  }
  g_offer = std::move(*offer);
  g_server->global_create(
      display, g_server->compositor_interface, kCompositorVersion,
      const_cast<WlInterface*>(g_server->compositor_interface), BindInert);
  g_server->global_create(display, g_server->shm_interface, 1, nullptr,
                          BindShm);
  g_server->global_create(display, &kXdgWmBase, 1,
                          const_cast<WlInterface*>(&kXdgWmBase), BindInert);
  g_server->global_create(display, g_server->output_interface, kOutputVersion,
                          nullptr, BindOutput);
  g_server->global_create(
      display, g_server->data_device_manager_interface,
      kDataDeviceManagerVersion,
      const_cast<WlInterface*>(g_server->data_device_manager_interface),
      BindInert);
  g_server->global_create(display, g_server->seat_interface, kSeatVersion,
                          nullptr, BindSeat);
  g_server->global_create(display, &kLinuxDmabuf,
                          static_cast<int>(g_formats.version), nullptr,
                          BindDmabuf);
  g_path = path;
  setenv("WAYLAND_DISPLAY", path.c_str(), 1);
  std::thread(Serve, display).detach();
}

void ClosePrivateDisplay() {
  if (!g_path.empty()) {
    unlink(g_path.c_str());
  }
}

}  // namespace sabine_display
