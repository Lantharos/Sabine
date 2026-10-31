#include "display/offer.h"

#include <unistd.h>

#include <algorithm>
#include <cstring>

#include "display/protocols.h"
#include "display/wayland.h"

namespace sabine_display {
namespace {

constexpr uint32_t kGetRegistry = 1;
constexpr uint32_t kBind = 0;
constexpr uint32_t kDestroy = 0;
constexpr uint32_t kGetKeyboard = 1;
constexpr uint32_t kSeatCapabilities = 0;
constexpr uint32_t kPointerCapability = 1;
constexpr uint32_t kKeyboardCapability = 2;
constexpr uint32_t kTouchCapability = 4;
constexpr uint32_t kKeymapEvent = 0;

struct Query {
  const ClientLibrary* client = nullptr;
  void* registry = nullptr;
  uint32_t dmabuf_name = 0;
  uint32_t dmabuf_version = 0;
  uint32_t seat_name = 0;
  uint32_t seat_capabilities = 0;
  Offer offer;
  Tranche tranche;
  bool done = false;
};

std::vector<uint8_t> Bytes(const WlArray* array) {
  const auto* data = static_cast<const uint8_t*>(array->data);
  return {data, data + array->size};
}

int OnRegistry(const void*,
               void* proxy,
               uint32_t opcode,
               const WlMessage*,
               WlArgument* arguments) {
  auto* query = static_cast<Query*>(LoadClient()->proxy_get_user_data(proxy));
  if (opcode != 0) {
    return 0;
  }
  if (std::strcmp(arguments[1].s, kLinuxDmabuf.name) == 0) {
    query->dmabuf_name = arguments[0].u;
    query->dmabuf_version = std::min<uint32_t>(arguments[2].u, 4);
  } else if (!query->seat_name &&
             std::strcmp(arguments[1].s, query->client->seat_interface->name) ==
                 0) {
    query->seat_name = arguments[0].u;
  }
  return 0;
}

int OnSeat(const void*,
           void* proxy,
           uint32_t opcode,
           const WlMessage*,
           WlArgument* arguments) {
  auto* query = static_cast<Query*>(LoadClient()->proxy_get_user_data(proxy));
  if (opcode == kSeatCapabilities) {
    query->seat_capabilities = arguments[0].u;
  }
  return 0;
}

int OnKeyboard(const void*,
               void* proxy,
               uint32_t opcode,
               const WlMessage*,
               WlArgument* arguments) {
  auto* query = static_cast<Query*>(LoadClient()->proxy_get_user_data(proxy));
  if (opcode == kKeymapEvent) {
    if (query->offer.keymap) {
      close(query->offer.keymap->fd);
    }
    query->offer.keymap =
        Keymap{arguments[0].u, arguments[1].h, arguments[2].u};
  }
  return 0;
}

int OnDmabuf(const void*,
             void* proxy,
             uint32_t opcode,
             const WlMessage*,
             WlArgument* arguments) {
  auto* query = static_cast<Query*>(LoadClient()->proxy_get_user_data(proxy));
  if (opcode == dmabuf::kModifierEvent) {
    query->offer.formats.formats.push_back(
        {arguments[0].u, arguments[1].u, arguments[2].u});
  }
  return 0;
}

int OnFeedback(const void*,
               void* proxy,
               uint32_t opcode,
               const WlMessage*,
               WlArgument* arguments) {
  auto* query = static_cast<Query*>(LoadClient()->proxy_get_user_data(proxy));
  switch (opcode) {
    case feedback::kDone:
      query->done = true;
      break;
    case feedback::kFormatTable:
      if (query->offer.formats.table_fd >= 0) {
        close(query->offer.formats.table_fd);
      }
      query->offer.formats.table_fd = arguments[0].h;
      query->offer.formats.table_size = arguments[1].u;
      break;
    case feedback::kMainDevice:
      query->offer.formats.main_device = Bytes(arguments[0].a);
      break;
    case feedback::kTrancheTargetDevice:
      query->tranche.target_device = Bytes(arguments[0].a);
      break;
    case feedback::kTrancheFormats:
      query->tranche.formats = Bytes(arguments[0].a);
      break;
    case feedback::kTrancheFlags:
      query->tranche.flags = arguments[0].u;
      break;
    case feedback::kTrancheDone:
      query->offer.formats.tranches.push_back(std::move(query->tranche));
      query->tranche = {};
      break;
  }
  return 0;
}

void* Bind(Query& query,
           uint32_t name,
           const WlInterface* interface,
           uint32_t version,
           WlDispatcher dispatcher) {
  void* proxy = query.client->proxy_marshal_flags(
      query.registry, kBind, interface, version, 0, name, interface->name,
      version, nullptr);
  query.client->proxy_add_dispatcher(proxy, dispatcher, nullptr, &query);
  return proxy;
}

// The seat Chromium sees, as it was when it shared the compositor's: the
// layout its keyboard starts with and the pointer and touch it hovers and taps
// with.
void CollectSeat(Query& query, void* display) {
  if (!query.seat_name) {
    return;
  }
  void* seat =
      Bind(query, query.seat_name, query.client->seat_interface, 1, OnSeat);
  if (query.client->display_roundtrip(display) < 0) {
    query.client->proxy_destroy(seat);
    return;
  }
  query.offer.pointer_and_touch =
      query.seat_capabilities & (kPointerCapability | kTouchCapability);
  if (query.seat_capabilities & kKeyboardCapability) {
    void* keyboard = query.client->proxy_marshal_flags(
        seat, kGetKeyboard, query.client->keyboard_interface, 1, 0, nullptr);
    query.client->proxy_add_dispatcher(keyboard, OnKeyboard, nullptr, &query);
    query.client->display_roundtrip(display);
    query.client->proxy_destroy(keyboard);
  }
  query.client->proxy_destroy(seat);
}

bool Collect(Query& query, void* display) {
  query.registry = query.client->proxy_marshal_flags(
      display, kGetRegistry, query.client->registry_interface, 1, 0, nullptr);
  query.client->proxy_add_dispatcher(query.registry, OnRegistry, nullptr,
                                     &query);
  if (query.client->display_roundtrip(display) < 0 || !query.dmabuf_name ||
      query.dmabuf_version < 3) {
    return false;
  }
  CollectSeat(query, display);
  void* dmabuf = Bind(query, query.dmabuf_name, &kLinuxDmabuf,
                      query.dmabuf_version, OnDmabuf);
  query.offer.formats.version = query.dmabuf_version;
  if (query.dmabuf_version < 4) {
    return query.client->display_roundtrip(display) >= 0 &&
           !query.offer.formats.formats.empty();
  }
  void* feedback =
      query.client->proxy_marshal_flags(dmabuf, dmabuf::kGetDefaultFeedback,
                                        &kLinuxDmabufFeedback, 4, 0, nullptr);
  query.client->proxy_add_dispatcher(feedback, OnFeedback, nullptr, &query);
  while (!query.done) {
    if (query.client->display_roundtrip(display) < 0) {
      return false;
    }
  }
  query.client->proxy_marshal_flags(feedback, kDestroy, nullptr, 4, 1);
  query.client->proxy_marshal_flags(dmabuf, kDestroy, nullptr, 4, 1);
  return query.offer.formats.table_fd >= 0;
}

}  // namespace

std::optional<Offer> QueryOffer() {
  const ClientLibrary* client = LoadClient();
  if (!client) {
    return std::nullopt;
  }
  void* display = client->display_connect(nullptr);
  if (!display) {
    return std::nullopt;
  }
  Query query;
  query.client = client;
  const bool collected = Collect(query, display);
  client->proxy_destroy(query.registry);
  client->display_disconnect(display);
  if (!collected) {
    if (query.offer.formats.table_fd >= 0) {
      close(query.offer.formats.table_fd);
    }
    if (query.offer.keymap) {
      close(query.offer.keymap->fd);
    }
    return std::nullopt;
  }
  return std::move(query.offer);
}

}  // namespace sabine_display
