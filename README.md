
A Rust-based embedder for hosting Flutter applications on Windows.


This library is very specific and was mainly used for an ingame world editor (see image) and not in production or any kind, was very experimental from start, less tested for other use cases besides the editor and it
provides a custom Flutter embedder that operates in two modes:


1.  **Standalone Runner**: Launch Flutter applications within a Rust executable instead of main application.
2.  **DirectX 11 Overlay**: Renders a Flutter UI to a D3D11 texture, allowing the host application to composite it into its own render pipeline.

Example how it got used: 
 <img width="1919" height="955" alt="hkhklhhh" src="https://github.com/user-attachments/assets/422646d0-6536-4a94-86f7-3f4049795efa" />

-----


## Features

  * Host Flutter applications within a Rust executable.
  * Dynamically load desktop plugins at runtime (standalone mode only).
  * Render UI to D3D11 textures with a full API for render and input management.
  * Provides a software renderer fallback and supports hardware-accelerated OpenGL via ANGLE.
  * Supports hybrid rendering of 2D widgets and 3D primitives.
  * Apply post-processing shaders to the Flutter UI or individual widgets.
  * Facilitates bi-directional Rust-Dart communication via send ports and platform channels. For complex data, consider using [flutter\_rust\_bridge](https://github.com/fzyzcjy/flutter_rust_bridge).

-----

## Setup and Dependencies

### 1\. Flutter Engine

This library requires the `flutter_engine.dll` for version **3.29.3**.

  * **JIT (Debug)**: Download the pre-compiled JIT engine from the official Google Storage API release ZIP.
  * **AOT (Release)**: Compile the AOT engine yourself by following the [Custom Flutter Engine Embedding in AOT Mode](https://github.com/flutter/engine/blob/main/docs/Custom-Flutter-Engine-Embedding-in-AOT-Mode.md) guide.

### 2\. ANGLE Libraries (Overlay Mode Only)

Hardware acceleration in overlay mode uses ANGLE for OpenGL to DirectX translation. This requires `libEGL.dll` and `libGLESv2.dll`. These can be found in a local Google Chrome installation directory (e.g., `C:\Program Files\Google\Chrome\Application\{version}\`).

### 3\. Flutter Assets

Build your Flutter application assets using one of the following commands:

  * **Standard Build**:
    ```bash
    flutter build windows
    ```
  * **Assemble Command**:
    ```bash
    flutter assemble --output=build -dTargetPlatform=windows-x64 -dBuildMode={build_mode} {build_mode}_bundle_windows-x64_assets
    ```

Place `flutter_engine.dll`, `libEGL.dll`, and `libGLESv2.dll` in the final asset directory. The library will locate them at runtime.

-----

## Usage

First add the lib to your project:
```
flutter_rust_windows_embedder = { git = "https://github.com/Vluurie/flutter-rust-windows-embedder.git", branch = "master" }
```

### Standalone Application

To run a Flutter application in a new window managed by this library:

```rust
// Use `init_flutter_window_from_dir()` to specify a custom path.
init_flutter_window();
```

### DirectX Overlay Integration

To embed a Flutter UI as an overlay in a DirectX application:

**1. Initialize an Overlay Instance**

Get the manager handle and initialize a Flutter instance tied to your application's swap chain. The renderer will default to hardware-accelerated OpenGL via ANGLE and fall back to software if necessary.

```rust
use std::path::PathBuf;

let manager = get_flutter_overlay_manager_handle().unwrap();
let assets_path = PathBuf::from("./flutter_build");

// Creates and initializes a new Flutter overlay instance.
manager.init_instance(
    &my_dxgi_swap_chain, // The host application's IDXGISwapChain
    &assets_path,
    "main_hud", // A unique identifier for this instance
    None,       // Optional: Dart main() arguments
    None,       // Optional: Flutter Engine arguments
);
```

**2. Render the UI**

In your application's main render loop, call `render_ui()` to draw the overlay.
You can also use it in the hooked present and render flutter ui on a hooked game :).

```rust
// Inside the main render loop...
manager.render_ui();
```

-----

## License

  * This crate is licensed under the **MIT License**. See [LICENSE](https://www.google.com/search?q=./LICENSE).
  * The Flutter engine and its C API bindings are licensed under the **BSD 3-Clause License**. See [LICENSE-THIRD-PARTY](https://www.google.com/search?q=./LICENSE-THIRD-PARTY).
