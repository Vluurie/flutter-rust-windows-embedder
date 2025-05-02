# Flutter Rust Embedder

A minimal Windows host application in Rust that embeds a Flutter engine and renders your Flutter UI in a native Win32 window.
Advances. It loads the plugins not static. Compiled as lib other Rust Apps can load Flutter Apps into the same App Process whenever they want.

## Prerequisites

- **Rust** (stable toolchain)
- **Windows 10+ SDK**
- **Flutter 3.29.3** (only tested on this version)

## 1. Build your Flutter app

From your Flutter project directory:

```bash
flutter build windows --release
````

This produces a `build/windows/runner/Release/` folder containing:

* `flutter_assets/`
* `icudtl.dat`
* Your AOT library (e.g. `app.so`)
* The as .cpp compiled .exe statically linked plugins (Rust Embedder replaces this - You dont need this .exe anymore normally)
* Native Flutter Plugins

All three must exist or it will not load!!!

## 2. Build the Rust host

```bash
git clone https://gitlab.yasupa.de/nams/flutter-rust-embedder.git
cd flutter-rust-embedder
cargo build --release
```

## 3. Prepare and run

Copy the Rust flutter_rust_windows_embedder.exe into the release folder next to the original .exe of your App.
Start the embedder.

A window titled **“Flutter Rust App”** should appear, displaying your Flutter UI.

## License

- **This project** is licensed under the MIT License. See [LICENSE](./LICENSE) for details.  
- **Flutter engine & C API bindings** are distributed under the BSD 3-Clause (“New” or “Revised”) License. See [LICENSE-THIRD-PARTY](./LICENSE-THIRD-PARTY) for the full text.  

```
