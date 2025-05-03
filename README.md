# Flutter Rust Embedder

A **Rust library** for hosting Flutter on Windows. Instead of building a standalone EXE, you now link this crate into your own Rust application and call its API to spin up a Flutter window (or composite Flutter into the same rust process). You can also build an .exe out of it by renaming lib to main, init or init from dir to main() and uncomment bin in the cargo toml.

* **Dynamic plugin discovery**: scans your release folder for `*.dll` plugins at runtime
* **Flexible asset paths**: choose the Flutter “data” directory yourself, or fall back to the DLL’s folder
* **Load into same process**: loads the flutter ap into the same process, enabled zero copy heap ponter share

---

## Prerequisites

* **Rust** (stable toolchain, 1.70+ recommended)
* **Windows 10 SDK** (for the `windows` crate bindings)
* **Flutter 3.29.3+** (tested on 3.29.3) with Windows desktop support

---

## 1. Build your Flutter app

In your Flutter project:

```bash
flutter build windows --release
```

That produces a folder like:

```
build/windows/runner/Release/
├── data/
│   ├── flutter_assets/ /IMPORTANT
│   ├── icudtl.dat !IMPORTANT
│   └── app.so !IMPORTANT
|-- Plugins !IMPORTANT
|-- Cpp executable (not needed)
|-- flutter_windows.dll !IMPORTANT
```

All three of these **must** be present under `data/`, and any plugin DLLs must live alongside that folder.

---

## 2. Add this crate to your Rust project

In your `Cargo.toml`:

```toml
[dependencies]
flutter_rust_windows_embedder = { git = "https://gitlab.yasupa.de/nams/flutter-rust-embedder.git", branch = "master" }
```

Then in your code:

```rust
use flutter_rust_windows_embedder::{init_flutter_window, init_flutter_window_from_dir};
use std::path::PathBuf;

fn main() {
    // 1) Simple: use the folder where this DLL/exe lives
    init_flutter_window(); // blocking

    // — or —

    // 2) Custom: point at your release bundle
     thread::spawn(|| { // non blocking
        // r"C:\path\to\your\flutter\build\windows\runner\Release"
        if let Some(dir) = select_data_directory() {
            init_flutter_window_from_dir(Some(dir));
        } else {
            init_flutter_window_from_dir(None); // fallback to 1)
        }
}
```

## License

* **This crate** is licensed under **MIT**. See [LICENSE](./LICENSE).
* **Flutter engine & C API** bindings are under the **BSD 3-Clause** (see [LICENSE-THIRD-PARTY](./LICENSE-THIRD-PARTY)).
