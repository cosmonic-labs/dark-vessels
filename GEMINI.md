# Gemini Context: Dark Vessels

A server-side maritime surveillance system powered by WebGPU compute shaders running inside WebAssembly on CNCF wasmCloud.

## Project Overview

Dark Vessels is a geospatial application that detects ships in SAR (Synthetic Aperture Radar) imagery and cross-references them against AIS telemetry to identify "dark vessels." It is built using the WebAssembly Component Model (WASI 0.2) and wasmCloud 2.0.

### Architecture

- **`api-gateway`**: A Wasm component serving as the entry point.
    - **Exports**: `wasi:http/incoming-handler` (via `wstd::http_server`).
    - **Imports**: `wasmcloud:messaging/consumer` to communicate with the processor.
    - **Functions**: Serves a MapLibre GL JS-based frontend and handles API requests by proxying to the `sar-processor`.
- **`sar-processor`**: The core compute engine.
    - **Exports**: `wasmcloud:messaging/handler`.
    - **Imports**: `wasi:webgpu/webgpu` (optional), `wasmcloud:messaging/consumer`.
    - **Pipeline**:
        1. **Synthetic Generation**: Generates SAR intensity images and AIS records.
        2. **GPU/CPU Compute**: Runs CA-CFAR (Cell-Averaging Constant False Alarm Rate) algorithm.
        3. **Spatial Join**: Matches detections using Haversine distance (500m threshold).
        4. **Classification**: Labels vessels as Matched, Dark Vessel, or AIS Only.

### Key Technologies

- **wasmCloud 2.0**: Orchestration and capability linking.
- **WASI 0.2 (wasip2)**: Standardized system interface for Wasm components.
- **WebGPU (wasi:webgpu)**: Hardware-accelerated compute in Wasm.
- **Rust**: Language for all server-side components.
- **WGSL**: Shaders for the CFAR algorithm.

---

## Building and Running

### Prerequisites

- `wash` CLI (2.0.4+)
- Rust with `wasm32-wasip2` target
- GPU drivers (Metal, Vulkan, or DX12) for WebGPU support

### Development Workflow

- **Start dev environment**:
  ```bash
  wash dev
  ```
  This command builds the components, starts a local wasmCloud host, and links the necessary providers (HTTP, Messaging, WebGPU).

- **Build with GPU support**:
  ```bash
  cargo build --workspace --target wasm32-wasip2 --release --features gpu
  ```

- **Build for CPU only**:
  ```bash
  cargo build --workspace --target wasm32-wasip2 --release
  ```

### Configuration

- **`.wash/config.yaml`**: Controls the `wash dev` environment, including feature flags and component paths.
- **`wit/world.wit`**: Defines the interfaces and capabilities for each component.

---

## Development Conventions

### Code Structure

- **`api-gateway/`**: Contains the HTTP server and the embedded SPA (`ui.html`).
- **`sar-processor/`**:
    - `src/gpu.rs`: WebGPU device initialization and dispatch.
    - `src/cfar.wgsl`: The GPU compute shader implementation.
    - `src/cfar_cpu.rs`: The fallback CPU implementation.
    - `src/synthetic.rs`: Data generators for SAR and AIS.
    - `src/spatial_join.rs`: Geographic matching logic.
- **`wit/`**: Centralized WIT definitions and dependencies.

### Styling & UI

- The frontend is a single-file SPA (`ui.html`) using MapLibre GL JS and vanilla CSS.
- Coordinates are handled in `[lon, lat]` format for GeoJSON compatibility.

### Messaging

- Communication between components is asynchronous via NATS (abstracted by `wasmcloud:messaging`).
- `api-gateway` uses `consumer::request` for a synchronous-like request/response pattern over messaging.

---

## Data Pipeline Details

1. **Detection Request**: Triggered via `POST /api/detect` with a bounding box and parameters.
2. **SAR Generation**: Creates a `f32` array representing pixel intensity.
3. **CFAR Processing**:
    - **GPU**: Uses WGSL shader with 16x16 workgroups.
    - **CPU**: Parallel execution over image rows.
4. **Extraction**: Connected Component Labeling (CCL) converts the detection mask into discrete objects with physical dimensions.
5. **Matching**: Haversine distance used to join SAR detections with AIS telemetry.
