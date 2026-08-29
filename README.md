# Artisan

A high-performance, web-first Archetype Entity Component System (ECS) game engine built in Rust, compiled to WebAssembly with SIMD and multi-threaded Rayon WebWorkers, and rendered via WebGPU.

This repository contains the full engine source code, interactive demonstrators, and evaluation benchmark suites.

---

## Core Architecture

- **Archetype ECS (`src/ecs/`)**: Contiguous columnar component storage (`BlobVec`) providing cache locality, vectorized iteration, and O(1) swap-remove entity deletion.
- **Zero-Copy Memory Bridge (`packages/artisan-js/`)**: Direct typed array views into WebAssembly memory for fast batch data transfers into WebGPU buffers (`writeBuffer`).
- **Parallel Work-Stealing Scheduler (`src/engine/mod.rs`)**: Automatic conflict-free stage generation based on system read/write access sets, executed across Rayon WebWorker thread pools.
- **WebGPU Rendering Pipeline (`packages/artisan-js/`)**: Instanced batch rendering for 2D and 3D scenes, frustum culling, and support for GPU Compute Shader simulation paths.

---

## Demonstrators

The repository includes six interactive demonstrators under `demos/`:

- **Vivarium** (`demos/vivarium_civ`): Planetary civilization simulation with procedural icosphere generation, climate and biome modeling, graph-based settler pathfinding, and instanced WebGPU rendering.
- **Murmuration** (`demos/murmuration`): Vector flow-field entity simulation comparing multi-threaded Rayon CPU/ECS execution against WebGPU Compute Shaders.
- **Bouncing Rects** (`demos/bouncing_rects`): High-density simulation updated via WebGPU storage buffers and compute shaders.
- **Scheduler** (`demos/scheduler`): Interactive stage visualizer displaying parallel execution timelines and system conflict graphs.
- **Metamorphosis** (`demos/metamorphosis`): Structural archetype migrations across eight component combinations.
- **Procedural Planet** (`demos/planet`): Interactive procedural icosphere planet generator with subdivision level controls and data layer overlays.

---

## Evaluation

Artisan includes a neutral cross-engine benchmark suite comparing Artisan, Bevy 0.18.1, and Flecs across 34 standardized measurement categories (iteration, topology fragmentation, entity lifecycle, structural mutation, and random access):

- **Artisan**: Fastest in 20 of 34 categories (1.05x geometric mean).
- **Flecs**: Fastest in 12 of 34 categories (1.37x geometric mean).
- **Bevy 0.18.1**: Fastest in 2 of 34 categories (1.50x geometric mean).

---

## Quick Start

### Prerequisites
- Rust Nightly (`nightly-2025-11-15` pinned in `rust-toolchain.toml`, with `rust-src` and `wasm32-unknown-unknown` targets)
- `wasm-pack` >= 0.15 (`cargo install wasm-pack`)
- Node.js >= 20
- Desktop browser with WebGPU support (Chrome, Edge, or Firefox Nightly)

### Building WebAssembly Packages
```bash
npm run build:wasm
```

### Running Demonstrators
Start the local development server (serves with COOP/COEP headers required for multi-threading):
```bash
npm run dev
```

Run individual demonstrators:
```bash
npm run vivarium
npm run murmuration
npm run rects
npm run scheduler
npm run metamorphosis
npm run planet
```

### Running Benchmarks
```bash
# Run Artisan native ECS suite
npm run bench:ecs

# Run the 34-category cross-engine suite
npm run bench:fair

# Run correctness verification tests
npm run verify:render-cache
```

---

## Repository Structure

```text
Artisan/
├── benches/            # Reference benchmark suites (Bevy 0.18.1, Flecs)
├── demos/              # Interactive demonstrators
├── packages/           # JavaScript runtime and WebGPU renderers
├── results/            # Measured evaluation datasets
├── src/                # Rust Archetype ECS engine and WASM exports
├── tools/              # Dev server and benchmark scripts
├── Cargo.toml          # Cargo workspace configuration
└── package.json        # NPM scripts and tooling
```

---

## License

MIT License - see [`LICENSE`](LICENSE) for details.

The bundled Flecs benchmark source is distributed under its upstream MIT license. See [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
