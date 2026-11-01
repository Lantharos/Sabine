//! `kestrel_window_v1`, the window placement protocol of the Kestrel
//! compositor.

#![allow(
    non_upper_case_globals,
    non_camel_case_types,
    unused_imports,
    clippy::all
)]

use wayland_client;
use wayland_client::protocol::*;
use wayland_protocols::xdg::shell::client::*;

pub(crate) mod __interfaces {
    use wayland_client::protocol::__interfaces::*;
    use wayland_protocols::xdg::shell::client::__interfaces::*;
    wayland_scanner::generate_interfaces!("protocols/kestrel-window-v1.xml");
}
use self::__interfaces::*;

wayland_scanner::generate_client_code!("protocols/kestrel-window-v1.xml");
