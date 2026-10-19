# aseith

A Minecraft 1.20.1 world renderer in pure Rust: parses Anvil-format world data (`level.dat`, `.mca` region files) and renders with Vulkan.

## Requirements

- Rust, edition 2021 (no `rust-version` pinned in `Cargo.toml`)
- A Vulkan-capable GPU and drivers
- `glslang` on `PATH` — `build.rs` compiles `shaders/renderer.vert` and `shaders/renderer.frag` to `shaders/spirv/*.spv` at build time
- The vanilla 1.20.1 asset tree, laid out as:

```
assets/minecraft/blockstates/
assets/minecraft/models/block/
assets/minecraft/textures/block/
assets/minecraft/textures/font/
assets/minecraft/textures/environment/
```

The current `src/` code does not read these files yet; `assets/` holds the vanilla data the project targets. The working tree ships a vanilla set; to (re-)create it, extract it from the vanilla client jar:

```sh
unzip -o .cache/minecraft/1.20.1-client.jar "assets/minecraft/*"
```

## Build & run

```sh
cargo run --release
```

`main.rs` takes no CLI arguments. The binary opens a 1280x720 Vulkan window and draws a clear color plus a triangle. World loading (NBT parsing of `level.dat` and region files) is library code exercised by the test suite:

```sh
cargo test
```

Test fixtures expect an Anvil world directory containing `level.dat` and `region/r.<x>.<z>.mca` files.
