
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(clippy::all)]

pub mod windows {
    include!("flutter_windows_bindings.rs");
}

pub mod embedder {
    include!("flutter_embedder_bindings.rs");
}

pub mod keyboard_layout;