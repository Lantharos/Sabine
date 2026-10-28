#include "display/protocols.h"

namespace sabine_display {
namespace {

extern const WlInterface kXdgPositioner;
extern const WlInterface kXdgSurface;
extern const WlInterface kXdgToplevel;
extern const WlInterface kXdgPopup;

const WlInterface* kNone[6] = {};
const WlInterface* kCreatesPositioner[] = {&kXdgPositioner};
const WlInterface* kCreatesXdgSurface[] = {&kXdgSurface, nullptr};
const WlInterface* kCreatesToplevel[] = {&kXdgToplevel};
const WlInterface* kCreatesPopup[] = {&kXdgPopup, nullptr, nullptr};
const WlInterface* kCreatesParams[] = {&kLinuxBufferParams};
const WlInterface* kCreatesFeedback[] = {&kLinuxDmabufFeedback, nullptr};

const WlMessage kXdgWmBaseRequests[] = {
    {"destroy", "", kNone},
    {"create_positioner", "n", kCreatesPositioner},
    {"get_xdg_surface", "no", kCreatesXdgSurface},
    {"pong", "u", kNone},
};
const WlMessage kXdgWmBaseEvents[] = {{"ping", "u", kNone}};

const WlMessage kXdgPositionerRequests[] = {
    {"destroy", "", kNone},
    {"set_size", "ii", kNone},
    {"set_anchor_rect", "iiii", kNone},
    {"set_anchor", "u", kNone},
    {"set_gravity", "u", kNone},
    {"set_constraint_adjustment", "u", kNone},
    {"set_offset", "ii", kNone},
};

const WlMessage kXdgSurfaceRequests[] = {
    {"destroy", "", kNone},
    {"get_toplevel", "n", kCreatesToplevel},
    {"get_popup", "n?oo", kCreatesPopup},
    {"set_window_geometry", "iiii", kNone},
    {"ack_configure", "u", kNone},
};
const WlMessage kXdgSurfaceEvents[] = {{"configure", "u", kNone}};

const WlMessage kXdgToplevelRequests[] = {
    {"destroy", "", kNone},
    {"set_parent", "?o", kNone},
    {"set_title", "s", kNone},
    {"set_app_id", "s", kNone},
    {"show_window_menu", "ouii", kNone},
    {"move", "ou", kNone},
    {"resize", "ouu", kNone},
    {"set_max_size", "ii", kNone},
    {"set_min_size", "ii", kNone},
    {"set_maximized", "", kNone},
    {"unset_maximized", "", kNone},
    {"set_fullscreen", "?o", kNone},
    {"unset_fullscreen", "", kNone},
    {"set_minimized", "", kNone},
};
const WlMessage kXdgToplevelEvents[] = {
    {"configure", "iia", kNone},
    {"close", "", kNone},
};

const WlMessage kXdgPopupRequests[] = {
    {"destroy", "", kNone},
    {"grab", "ou", kNone},
};
const WlMessage kXdgPopupEvents[] = {
    {"configure", "iiii", kNone},
    {"popup_done", "", kNone},
};

const WlMessage kLinuxDmabufRequests[] = {
    {"destroy", "", kNone},
    {"create_params", "n", kCreatesParams},
    {"get_default_feedback", "4n", kCreatesFeedback},
    {"get_surface_feedback", "4no", kCreatesFeedback},
};
const WlMessage kLinuxDmabufEvents[] = {
    {"format", "u", kNone},
    {"modifier", "3uuu", kNone},
};

const WlMessage kLinuxBufferParamsRequests[] = {
    {"destroy", "", kNone},
    {"add", "huuuuu", kNone},
    {"create", "iiuu", kNone},
    {"create_immed", "2niiuu", kNone},
};
const WlMessage kLinuxBufferParamsEvents[] = {
    {"created", "n", kNone},
    {"failed", "", kNone},
};

const WlMessage kLinuxDmabufFeedbackRequests[] = {{"destroy", "", kNone}};
const WlMessage kLinuxDmabufFeedbackEvents[] = {
    {"done", "", kNone},
    {"format_table", "hu", kNone},
    {"main_device", "a", kNone},
    {"tranche_done", "", kNone},
    {"tranche_target_device", "a", kNone},
    {"tranche_formats", "a", kNone},
    {"tranche_flags", "u", kNone},
};

template <typename T, size_t N>
constexpr int Count(const T (&)[N]) {
  return N;
}

const WlInterface kXdgPositioner = {
    "xdg_positioner",       1, Count(kXdgPositionerRequests),
    kXdgPositionerRequests, 0, nullptr};
const WlInterface kXdgSurface = {"xdg_surface",
                                 1,
                                 Count(kXdgSurfaceRequests),
                                 kXdgSurfaceRequests,
                                 Count(kXdgSurfaceEvents),
                                 kXdgSurfaceEvents};
const WlInterface kXdgToplevel = {"xdg_toplevel",
                                  1,
                                  Count(kXdgToplevelRequests),
                                  kXdgToplevelRequests,
                                  Count(kXdgToplevelEvents),
                                  kXdgToplevelEvents};
const WlInterface kXdgPopup = {"xdg_popup",
                               1,
                               Count(kXdgPopupRequests),
                               kXdgPopupRequests,
                               Count(kXdgPopupEvents),
                               kXdgPopupEvents};

}  // namespace

const WlInterface kXdgWmBase = {"xdg_wm_base",
                                1,
                                Count(kXdgWmBaseRequests),
                                kXdgWmBaseRequests,
                                Count(kXdgWmBaseEvents),
                                kXdgWmBaseEvents};
const WlInterface kLinuxDmabuf = {
    "zwp_linux_dmabuf_v1",       4,
    Count(kLinuxDmabufRequests), kLinuxDmabufRequests,
    Count(kLinuxDmabufEvents),   kLinuxDmabufEvents};
const WlInterface kLinuxBufferParams = {
    "zwp_linux_buffer_params_v1",      4,
    Count(kLinuxBufferParamsRequests), kLinuxBufferParamsRequests,
    Count(kLinuxBufferParamsEvents),   kLinuxBufferParamsEvents};
const WlInterface kLinuxDmabufFeedback = {
    "zwp_linux_dmabuf_feedback_v1",      4,
    Count(kLinuxDmabufFeedbackRequests), kLinuxDmabufFeedbackRequests,
    Count(kLinuxDmabufFeedbackEvents),   kLinuxDmabufFeedbackEvents};

}  // namespace sabine_display
