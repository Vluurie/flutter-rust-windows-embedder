@echo off
REM RUSTC_WRAPPER shim: cargo invokes this as `<wrapper> <rustc-path> <args...>`.
REM clippy-driver is a drop-in for rustc and accepts the same argv layout, so we
REM forward everything. Combined with `absolute_paths = "deny"` in Cargo.toml this
REM makes inline absolute paths a hard error on every `cargo build`.
clippy-driver %*
