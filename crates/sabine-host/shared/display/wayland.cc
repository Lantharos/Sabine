#include "display/wayland.h"

#include <dlfcn.h>

namespace sabine_display {
namespace {

template <typename T>
bool Resolve(void* library, const char* name, T& slot) {
  slot = reinterpret_cast<T>(dlsym(library, name));
  return slot != nullptr;
}

ClientLibrary* OpenClient() {
  void* library = dlopen("libwayland-client.so.0", RTLD_NOW | RTLD_LOCAL);
  if (!library) {
    return nullptr;
  }
  static ClientLibrary client;
  const bool resolved =
      Resolve(library, "wl_display_connect", client.display_connect) &&
      Resolve(library, "wl_display_disconnect", client.display_disconnect) &&
      Resolve(library, "wl_display_roundtrip", client.display_roundtrip) &&
      Resolve(library, "wl_proxy_marshal_flags", client.proxy_marshal_flags) &&
      Resolve(library, "wl_proxy_add_dispatcher",
              client.proxy_add_dispatcher) &&
      Resolve(library, "wl_proxy_get_user_data", client.proxy_get_user_data) &&
      Resolve(library, "wl_proxy_destroy", client.proxy_destroy) &&
      Resolve(library, "wl_registry_interface", client.registry_interface) &&
      Resolve(library, "wl_seat_interface", client.seat_interface) &&
      Resolve(library, "wl_keyboard_interface", client.keyboard_interface);
  return resolved ? &client : nullptr;
}

ServerLibrary* OpenServer() {
  void* library = dlopen("libwayland-server.so.0", RTLD_NOW | RTLD_LOCAL);
  if (!library) {
    return nullptr;
  }
  static ServerLibrary server;
  const bool resolved =
      Resolve(library, "wl_display_create", server.display_create) &&
      Resolve(library, "wl_display_get_event_loop",
              server.display_get_event_loop) &&
      Resolve(library, "wl_display_add_socket_fd",
              server.display_add_socket_fd) &&
      Resolve(library, "wl_display_flush_clients",
              server.display_flush_clients) &&
      Resolve(library, "wl_event_loop_dispatch", server.event_loop_dispatch) &&
      Resolve(library, "wl_global_create", server.global_create) &&
      Resolve(library, "wl_resource_create", server.resource_create) &&
      Resolve(library, "wl_resource_set_dispatcher",
              server.resource_set_dispatcher) &&
      Resolve(library, "wl_resource_post_event", server.resource_post_event) &&
      Resolve(library, "wl_resource_destroy", server.resource_destroy) &&
      Resolve(library, "wl_resource_get_version",
              server.resource_get_version) &&
      Resolve(library, "wl_resource_get_client", server.resource_get_client) &&
      Resolve(library, "wl_client_post_no_memory",
              server.client_post_no_memory) &&
      Resolve(library, "wl_compositor_interface",
              server.compositor_interface) &&
      Resolve(library, "wl_shm_interface", server.shm_interface) &&
      Resolve(library, "wl_output_interface", server.output_interface) &&
      Resolve(library, "wl_buffer_interface", server.buffer_interface) &&
      Resolve(library, "wl_data_device_manager_interface",
              server.data_device_manager_interface) &&
      Resolve(library, "wl_seat_interface", server.seat_interface) &&
      Resolve(library, "wl_keyboard_interface", server.keyboard_interface);
  return resolved ? &server : nullptr;
}

}  // namespace

const ClientLibrary* LoadClient() {
  static const ClientLibrary* client = OpenClient();
  return client;
}

const ServerLibrary* LoadServer() {
  static const ServerLibrary* server = OpenServer();
  return server;
}

}  // namespace sabine_display
