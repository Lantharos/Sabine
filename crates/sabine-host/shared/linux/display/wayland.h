#ifndef SABINE_DISPLAY_WAYLAND_H_
#define SABINE_DISPLAY_WAYLAND_H_

#include <cstddef>
#include <cstdint>

// The parts of libwayland's stable ABI the private display uses. The
// libraries are loaded at runtime, so building the host needs no Wayland
// headers.
namespace sabine_display {

struct WlInterface;

struct WlMessage {
  const char* name;
  const char* signature;
  const WlInterface** types;
};

struct WlInterface {
  const char* name;
  int version;
  int method_count;
  const WlMessage* methods;
  int event_count;
  const WlMessage* events;
};

struct WlArray {
  size_t size;
  size_t alloc;
  void* data;
};

union WlArgument {
  int32_t i;
  uint32_t u;
  int32_t f;
  const char* s;
  void* o;
  uint32_t n;
  WlArray* a;
  int32_t h;
};

using WlDispatcher = int (*)(const void* data,
                             void* target,
                             uint32_t opcode,
                             const WlMessage* message,
                             WlArgument* arguments);

struct ClientLibrary {
  void* (*display_connect)(const char* name);
  void (*display_disconnect)(void* display);
  int (*display_roundtrip)(void* display);
  void* (*proxy_marshal_flags)(void* proxy,
                               uint32_t opcode,
                               const WlInterface* interface,
                               uint32_t version,
                               uint32_t flags,
                               ...);
  int (*proxy_add_dispatcher)(void* proxy,
                              WlDispatcher dispatcher,
                              const void* data,
                              void* user_data);
  void* (*proxy_get_user_data)(void* proxy);
  void (*proxy_destroy)(void* proxy);
  const WlInterface* registry_interface;
  const WlInterface* seat_interface;
  const WlInterface* keyboard_interface;
};

struct ServerLibrary {
  void* (*display_create)();
  void* (*display_get_event_loop)(void* display);
  int (*display_add_socket_fd)(void* display, int fd);
  void (*display_flush_clients)(void* display);
  int (*event_loop_dispatch)(void* loop, int timeout);
  void* (*global_create)(
      void* display,
      const WlInterface* interface,
      int version,
      void* data,
      void (*bind)(void* client, void* data, uint32_t version, uint32_t id));
  void* (*resource_create)(void* client,
                           const WlInterface* interface,
                           int version,
                           uint32_t id);
  void (*resource_set_dispatcher)(void* resource,
                                  WlDispatcher dispatcher,
                                  const void* implementation,
                                  void* data,
                                  void (*destroy)(void* resource));
  void (*resource_post_event)(void* resource, uint32_t opcode, ...);
  void (*resource_destroy)(void* resource);
  int (*resource_get_version)(void* resource);
  void* (*resource_get_client)(void* resource);
  void (*client_post_no_memory)(void* client);
  const WlInterface* compositor_interface;
  const WlInterface* shm_interface;
  const WlInterface* output_interface;
  const WlInterface* buffer_interface;
  const WlInterface* data_device_manager_interface;
  const WlInterface* seat_interface;
  const WlInterface* keyboard_interface;
};

const ClientLibrary* LoadClient();
const ServerLibrary* LoadServer();

}  // namespace sabine_display

#endif
