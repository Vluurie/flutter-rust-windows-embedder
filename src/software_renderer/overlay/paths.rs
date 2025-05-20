use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
use crate::path_utils::{get_flutter_paths, get_flutter_paths_from};

/// Returns `(assets, icu, aot_opt)`. Panics if assets/icu missing; returns `None` for AOT if absent.
pub(crate) fn load_flutter_paths(
    data_dir: Option<PathBuf>,
) -> (OsString, OsString, Option<OsString>) {
    let (assets_w, icu_w, aot_w) = match &data_dir {
        Some(dir) => get_flutter_paths_from(dir),
        None => get_flutter_paths(),
    };
    let strip = |mut v: Vec<u16>| {
        if v.last() == Some(&0) { v.pop(); }
        v
    };
    let os_from = |v: Vec<u16>| OsString::from_wide(&strip(v));

    let assets = os_from(assets_w);
    let icu    = os_from(icu_w);
    let aot_vec = strip(aot_w);
    let aot    = if aot_vec.is_empty() {
        None
    } else {
        Some(OsString::from_wide(&aot_vec))
    };

    (assets, icu, aot)
}
