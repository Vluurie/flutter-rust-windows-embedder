
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(clippy::all)]
#![allow(clippy::absolute_paths)]

#[allow(clippy::absolute_paths)]
pub mod windows {
    include!("flutter_windows_bindings.rs");
}

#[allow(clippy::absolute_paths)]
pub mod embedder {
    include!("flutter_embedder_bindings.rs");
}

#[allow(clippy::absolute_paths)]
pub mod keyboard_layout;