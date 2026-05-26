# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**fits-view** — a fast, VSCode-like FITS file viewer written in Rust. GPU rendering via `wgpu` with automatic CPU fallback via `rayon`. Target users are astronomers and data pipeline engineers dealing with GB–TB scale FITS files.

The project is currently in the **planning stage**. Implementation starts at Step 0 (workspace scaffold) and progresses through Steps 1–5 as defined in `implementation_plan.md`.

## Workspace Structure

```
fits-viewer/
├── Cargo.toml               # workspace root
├── crates/
│   ├── fitsview-core/       # FITS I/O, rendering pipeline, tile manager
│   ├── fitsview-gui/        # egui/eframe GUI application
│   └── fitsview-cli/        # clap-based CLI entrypoint
├── assets/colormaps/        # LUT data for colormaps
└── tests/fixtures/          # small FITS files for tests
```

`fitsview-cli` depends on both `fitsview-gui` and `fitsview-core`. GUI and CLI share one binary; the mode is determined by whether a subcommand is present.

## Build Commands

```bash
# Standard build (GPU enabled by default)
cargo build --release

# CPU-only build (no wgpu — always works in WSL2/SSH/CI)
cargo build --release --no-default-features --features cpu-only

# Run the GUI on a file or directory
./target/release/fits-view image.fits
./target/release/fits-view .

# Headless render (no GUI)
./target/release/fits-view render image.fits --output thumb.png --size 256x256
```

## Testing

```bash
# Run all tests
cargo test --workspace

# Run a single test
cargo test -p fitsview-core test_name

# CPU-only CI mode
cargo test --workspace --no-default-features --features cpu-only

# Lint
cargo clippy --workspace -- -D warnings
```

## Feature Flags

Defined in `fitsview-gui/Cargo.toml`:

```toml
[features]
default = ["gpu"]
gpu = ["wgpu"]
cpu-only = []
```

All CI must pass with both `--features gpu` (default) and `--no-default-features --features cpu-only`.

## Architecture

### Core Engine (`fitsview-core`)

| File | Responsibility |
|---|---|
| `fits_reader.rs` | Read FITS via `fitsrs`; normalize all BITPIX types to `Vec<f32>`; return header as `HashMap<String, String>` |
| `mmap_reader.rs` | (Step 3) `memmap2`-based tile-streamed reader for large files; zero-copy BITPIX→f32 conversion |
| `tile_manager.rs` | LRU tile cache keyed by `(file_id, hdu, zoom_level, tx, ty)`; default 512×512 px tiles |
| `tile_loader.rs` | Async tile requests via `tokio`; CPU-bound decode via `rayon` |
| `scale.rs` | Scaling algorithms: `ZScale`, `Linear`, `Log`, `Sqrt`, `Asinh`, `MinMax` |
| `colormap.rs` | Colormaps as compile-time `const` LUT arrays; `Gray`, `Viridis`, `Plasma`, `Inferno`, `Hot`, `Rainbow` |
| `wcs.rs` | (Step 5) WCS keyword parsing; pixel↔RA/Dec conversion |
| `session.rs` | (Step 5) `.fvs` JSON session serialization/deserialization |

### GUI Layer (`fitsview-gui`)

| File | Responsibility |
|---|---|
| `app.rs` | Top-level `eframe::App`; integrates all panels |
| `viewport.rs` | `ViewState { offset, zoom }`; pan/zoom input handling |
| `header_panel.rs` | Right panel; scrollable FITS header display with keyword search |
| `file_explorer.rs` | (Step 2) Left sidebar directory tree; FITS file highlighting |
| `tab_manager.rs` | (Step 2) `Vec<Tab>`; `Ctrl+Tab`/`Ctrl+W` handling |
| `split_view.rs` | (Step 2) `Single` / `SideBySide` / `Grid2x2` layouts |
| `status_bar.rs` | (Step 2) Bottom bar: filename, cursor pixel value, scale/colormap/zoom |
| `dir_watcher.rs` | (Step 2) `notify` watcher → `mpsc` channel → GUI update |
| `command_palette.rs` | (Step 2) `Ctrl+Shift+P` overlay with incremental search |
| `renderer/mod.rs` | (Step 3) `Renderer` trait + auto-selection logic |
| `renderer/gpu.rs` | (Step 3) wgpu pipeline: Transfer Buffer → Compute Shader (tonemap + 1D LUT) → Fragment Shader |
| `renderer/cpu.rs` | rayon-parallel RGBA buffer → egui Texture |

### CLI Layer (`fitsview-cli`)

Subcommands: `render`, `info`, `stats`, `check`. No subcommand → GUI mode.

## Rendering Pipeline

```
FITS data (fitsrs / memmap2)
    │  f32 pixel data
    ▼
Auto-select renderer
    ├─ GPU available → wgpu compute + fragment shaders
    └─ No GPU / --renderer cpu → rayon parallel CPU renderer
         ↓
    RGBA buffer → egui Texture → screen
```

Renderer fallback triggers: wgpu device unavailable, GPU OOM, `--renderer cpu` flag.

## WSL2 Development Notes

- **GPU**: wgpu uses DirectX 12 (`dx12` backend) or Vulkan (Mesa D3D12) via `/dev/dxg`. If unavailable, CPU fallback activates automatically.
- **Files**: Avoid `memmap2` on `/mnt/c/...` (Windows FS over 9P is slow). Use WSL2-native paths (`/home/...`) for development and tests.
- **Rust version**: managed by mise (`rust = "1.95.0"` in `.mise.toml`). MSRV is 1.75.

## Implementation Sequence

Work within each step in this order to keep CI green at all times:
1. `fitsview-core` data structures + unit tests
2. `fitsview-gui` UI components + manual verification
3. `fitsview-cli` integration last

Steps: **0** (workspace) → **1** (MVP viewer) → **2** (VSCode-like UI) → **3** (large file + GPU) → **4** (CLI headless) → **5** (session/WCS/Wasm/PyO3).
