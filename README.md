# Dark Vessels

**Server-side maritime surveillance powered by WebGPU compute shaders running inside WebAssembly on CNCF wasmCloud.**

Dark Vessels detects ships in synthetic Sentinel-1 SAR (Synthetic Aperture Radar) imagery using a GPU-accelerated CA-CFAR algorithm, then cross-references against AIS vessel telemetry to identify "dark vessels" — ships visible on radar but not broadcasting their identity.

---

## Why This Matters

This project demonstrates that **real signal processing workloads can run inside Wasm components** with GPU acceleration, secure sandboxing, and sub-second cold starts. Everything ships as two tiny `.wasm` files totaling **639 KB**.

---

## NOTE: Constraints
This repo is a light weight proof of concept that uses synthetic data to simplify a concept, is architectural simplified, and has not been secured for production. Please see [CONSTRAINTS.md](CONSTRAINTS.md) for a list of opportunities to make this project more architecturally sound, secure, and correct.

---

### Demonstration
![Dark Vessels — server-side GPU based SAR image processing with AIS ingration running as Wasm components](dark-vessels-basic-cpu-gpu-demo.gif) 

---

### ToDo
This demonstration is designed to show the WebAssembly inside of a GPU so data sources are currently mocked up 

- [x] CPU Workflow
- [x] GPU Workflow
- [x] Sample data generator
- [ ] SAR Integration
- [ ] AIS Integration


---

## Architecture

```
                           wasmCloud 2.0 Host (wash dev)
    ┌──────────────────────────────────────────────────────────────────────┐
    │                                                                      │
    │  ┌─────────────────────────┐    NATS     ┌──────────────────────────┐│
    │  │     api-gateway.wasm    │  messaging   │    sar-processor.wasm   ││
    │  │         391 KB          │◄────────────►│         247 KB          ││
    │  │                         │              │                         ││
    │  │  Exports:               │              │  Exports:               ││
    │  │   wasi:http/incoming-   │              │   wasmcloud:messaging/  ││
    │  │     handler             │              │     handler             ││
    │  │                         │              │                         ││
    │  │  Imports:               │              │  Imports:               ││
    │  │   wasmcloud:messaging/  │              │   wasi:webgpu/webgpu    ││
    │  │     consumer            │              │   wasmcloud:messaging/  ││
    │  │                         │              │     consumer            ││
    │  │  ┌───────────────────┐  │              │                         ││
    │  │  │  Embedded SPA UI  │  │              │  ┌───────────────────┐  ││
    │  │  │  (MapLibre GL JS) │  │              │  │  WGSL Compute     │  ││
    │  │  │  391 KB total     │  │              │  │  Shader (CFAR)    │  ││
    │  │  └───────────────────┘  │              │  └────────┬──────────┘  ││
    │  └─────────┬───────────────┘              │           │             ││
    │            │                              │     ┌─────▼─────┐       ││
    │            │ HTTP :8000                   │     │   wgpu    │       ││
    │            │                              │     │  (Metal/  │       ││
    │            │                              │     │  Vulkan)  │       ││
    │            │                              │     └───────────┘       ││
    │            │                              └─────────────────────────┘│
    └────────────┼─────────────────────────────────────────────────────────┘
                 │
    ┌────────────▼────────────────────────────────────────────────────────┐
    │                        Browser                                      │
    │                                                                     │
    │  ┌──────────────────────────────────────────────────────────────┐   │
    │  │  MapLibre GL JS Map                                          │   │
    │  │  ┌─────────┐  ┌─────────────┐  ┌──────────────────────────┐  │   │
    │  │  │ Search   │  │ Draw BBox   │ │  Stats Dashboard         │  │   │
    │  │  │ (Nominat-│  │ Tool        │ │  GPU/CPU Toggle          │  │   │
    │  │  │  im API) │  │             │ │  Density Slider          │  │   │
    │  │  └─────────┘  └─────────────┘  │  Size Classification     │  │   │
    │  │                                │  Ship Silhouette Popups  │  │   │
    │  │  ● Green = AIS Matched         │  SAR Radar Footprint     │  │   │
    │  │  ● Red (pulse) = Dark Vessel   │  Feet/Meters Toggle      │  │   │
    │  │  ○ Amber = AIS Only            └──────────────────────────┘  │   │
    │  └──────────────────────────────────────────────────────────────┘   │
    └─────────────────────────────────────────────────────────────────────┘

    External Services (optional, credentials stored in browser cookies):
    ┌─────────────────────┐    ┌──────────────────┐
    │  Copernicus CDSE    │    │  AISHub API      │
    │  (Sentinel-1 SAR)   │    │  (AIS Telemetry) │
    │  dataspace.         │    │  aishub.net      │
    │   copernicus.eu     │    │                  │
    └─────────────────────┘    └──────────────────┘

    Geocoding:
    ┌─────────────────────┐
    │  OpenStreetMap      │
    │  Nominatim          │
    │  (Location Search)  │
    └─────────────────────┘
```

---

## Data Pipeline

```
  Synthetic Generator          GPU / CPU               Spatial Join
  (or Copernicus CDSE)         (wasi:webgpu)           (Haversine)
                                                     
  ┌───────────────┐     ┌──────────────────┐     ┌────────────────────┐
  │ SAR Intensity │────►│  CA-CFAR Compute │────►│  Match SAR ↔ AIS   │
  │ Image (f32[]) │     │  Shader (WGSL)   │     │  within 500m       │
  │               │     │  16×16 workgroups│     │                    │
  │ + AIS Records │     │                  │     │  Classify:         │
  │   (synthetic  │     │  Detection mask  │     │  ● Matched         │
  │    or AISHub) │     │  → CCL → Ships   │     │  ● Dark Vessel     │
  └───────────────┘     │  with dimensions │     │  ● AIS Only        │
                        └──────────────────┘     └────────┬───────────┘
                                                          │
                                                          ▼
                                                 GeoJSON FeatureCollection
                                                 + Processing Stats
```

---

## GPU vs CPU Performance

The CA-CFAR algorithm runs on every pixel of the SAR image. The GPU compute shader (WGSL) dispatches 16x16 workgroups across the image in parallel. Toggle between GPU and CPU in the UI to see the difference live.

| SAR Image | Pixels | GPU (WebGPU) | CPU | Speedup |
|-----------|--------|-------------|-----|---------|
| 512 x 512 | 262K | **157 ms** | 431 ms | 2.7x |
| 1024 x 1024 | 1.05M | **242 ms** | 1,223 ms | **5.0x** |
| 2048 x 2048 | 4.19M | **466 ms** | 6,421 ms | **13.8x** |
| 4096 x 4096 | 16.8M | **1,918 ms** | 22,716 ms | **11.8x** |

GPU advantage scales with image size. At 4096x4096 (16.8 million pixels), GPU processes in **under 2 seconds** while CPU takes **over 22 seconds**. Real Sentinel-1 GRD scenes are 25,000 x 16,000 pixels (~400 million pixels) — GPU is not optional, it's essential.

The SAR size slider in the UI lets you scale from 512 to 4096 pixels per side. Toggle between GPU and CPU at each size to see the speedup live.

---

## Wasm Component Stats

| Component | Binary Size | Cold Start | Exports | Key Imports |
|-----------|------------|------------|---------|-------------|
| `api-gateway.wasm` | **391 KB** | < 10ms | `wasi:http/incoming-handler` | `wasmcloud:messaging/consumer` |
| `sar-processor.wasm` | **247 KB** | < 10ms | `wasmcloud:messaging/handler` | `wasi:webgpu/webgpu`, `wasmcloud:messaging/consumer` |
| **Total** | **639 KB** | | | |

**639 KB** for a complete maritime surveillance system with GPU compute, HTTP server, embedded frontend, and global coastline data.

### Secure Sandbox

Each Wasm component runs in a **sandboxed execution environment** enforced by the WebAssembly runtime:

- **No filesystem access** — components cannot read or write to the host filesystem
- **No network access** except through declared WIT imports — `api-gateway` can only serve HTTP, `sar-processor` can only use WebGPU and messaging
- **No ambient authority** — capabilities are explicitly linked at deployment time
- **Memory isolation** — each component has its own linear memory, no shared state
- **Deny by default** — if an import isn't declared in the WIT world and linked by the host, it doesn't exist

The host provides only the interfaces each component declares it needs. A compromised `sar-processor` cannot make HTTP requests, access the filesystem, or even see the other component's memory. This is not a container boundary — it's a **language-level capability sandbox** enforced by the Wasm type system.

---

## Quick Start

```bash
# Prerequisites: wash 2.0.4+, Rust with wasm32-wasip2 target
wash dev
# Open http://localhost:8000
```

The system starts with synthetic data — no API keys needed. Draw a bounding box anywhere in the world, adjust the density slider, toggle GPU/CPU, and click RUN DETECTION.

### With GPU Compute

GPU is enabled by default via `.wash/config.yaml`:

```yaml
build:
  command: cargo build --workspace --target wasm32-wasip2 --release --features gpu
dev:
  wasi_webgpu: true
```

Both settings are required — `--features gpu` compiles the `wasi:webgpu` import into the component, and `wasi_webgpu: true` tells the wash runtime to activate its WebGPU plugin (backed by `wgpu-core` → Metal/Vulkan/DX12).

### Without GPU (CPU fallback)

Remove `--features gpu` from the build command and `wasi_webgpu: true` from dev config. The system automatically falls back to a pure-Rust CPU implementation of the same CFAR algorithm.

---

## Connecting Real Data Sources

### Copernicus CDSE (Sentinel-1 SAR)

1. Create a free account at [dataspace.copernicus.eu](https://dataspace.copernicus.eu)
2. Click **Connect** in the Data Sources panel
3. Enter your credentials (stored in a browser cookie, sent only to Copernicus)

### AISHub (Vessel Telemetry)

1. Register at [aishub.net](https://www.aishub.net)
2. Share an AIS receiver or purchase a data plan to receive an API key
3. Click **Enter Key** in the Data Sources panel

Without credentials, the system uses synthetic data with realistic vessel distributions, ocean-only placement (Natural Earth 110m coastlines), and configurable density.

---

## Project Structure

```
dark-vessels/
├── api-gateway/          HTTP server + embedded frontend
│   ├── src/lib.rs        Route dispatch, GeoJSON conversion
│   └── ui.html           Full SPA (MapLibre, stats, draw tool, modals)
├── sar-processor/        GPU compute worker
│   ├── src/lib.rs        Pipeline orchestration, detection extraction
│   ├── src/gpu.rs        wasi:webgpu device init, buffer mgmt, dispatch
│   ├── src/cfar.wgsl     WGSL compute shader (CA-CFAR algorithm)
│   ├── src/cfar_cpu.rs   CPU fallback (same algorithm)
│   ├── src/synthetic.rs  SAR image + AIS data generators
│   ├── src/spatial_join.rs  Haversine matching (500m threshold)
│   ├── src/ocean.rs      Global land/ocean mask (Natural Earth 110m)
│   └── src/types.rs      Shared types and region definitions
├── wit/                  WIT interface definitions
│   ├── world.wit         Component worlds (api-gateway, sar-processor, sar-processor-gpu)
│   └── deps/             Vendored WIT packages (webgpu, messaging, io)
├── .wash/config.yaml     Build + dev configuration
└── CLAUDE.md             AI assistant context
```

---

## Technology Stack

- **[wasmCloud 2.0](https://wasmcloud.com)** — CNCF WebAssembly application platform
- **[WASI 0.2](https://wasi.dev)** — WebAssembly System Interface (component model)
- **[wasi:webgpu](https://github.com/WebAssembly/wasi-gfx)** — WebGPU for Wasm components
- **[wgpu](https://wgpu.rs)** — Rust WebGPU implementation (Metal, Vulkan, DX12)
- **Rust** — compiled to `wasm32-wasip2`
- **[MapLibre GL JS](https://maplibre.org)** — open-source map rendering
- **[Natural Earth](https://www.naturalearthdata.com)** — public domain coastline data

---

*Built with wasmCloud 2.0 to demonstrate that WebAssembly components can handle complex, GPU-accelerated geospatial workloads — securely, portably, and in under 640 KB.*
