//! `kestrel_glass_v1`, through which the Kestrel compositor says which
//! material a surface's glass has.

#![allow(
    non_upper_case_globals,
    non_camel_case_types,
    unused_imports,
    clippy::all
)]

use wayland_client;
use wayland_client::protocol::*;

pub(crate) mod __interfaces {
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("protocols/kestrel-glass-v1.xml");
}
use self::__interfaces::*;

wayland_scanner::generate_client_code!("protocols/kestrel-glass-v1.xml");
