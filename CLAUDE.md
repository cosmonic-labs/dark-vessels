# Dark Vessels — wasmCloud 2.0 + WebGPU Maritime Surveillance

## Architecture

**ONE workload, ONE component.** `sar-processor` (compiled to `wasm32-wasip2`) serves the frontend SPA, the GeoJSON API, and GPU CA-CFAR detection, all behind its async `wasi:http/handler@0.3.0` (P3) export:

- Detection lives behind the **async** P3 HTTP export because `wasi:webgpu@0.3.0-rc.2` declares its acquisition/map functions `async func` — a sync-lifted export cannot await them.
- The `wasmcloud:messaging` handler remains as a CPU-only compatibility trigger on subject `tasks.sar-processor`.
- Do NOT reintroduce a separate HTTP gateway component: the wasmCloud runtime dispatches `wasi:http` to only **one component per workload** (the host logs `multiple components carry an export...routing to the first in manifest order` and the second component never receives traffic). An earlier `api-gateway` crate was removed for this reason (see git history).

Deployment manifests (all single-workload): `deploy/desktop/workload.yaml` (Cosmonic Desktop, flat `Workload`), `deploy/control/httptrigger.yaml` (Cosmonic Control), `deploy/wasmcloud/workloaddeployment.yaml` (vanilla wasmCloud runtime-operator).

The WIT worlds are defined in `wit/world.wit`. There are two worlds: `sar-processor` (CPU-only) and `sar-processor-gpu` (with `wasi:webgpu` import). The `gpu` Cargo feature switches between them.

## wasmCloud WebGPU — Critical Configuration

`wash dev` does **NOT** have a `--wasi-webgpu` CLI flag. To enable WebGPU, you must set **both** of these in `.wash/config.yaml`:

```yaml
build:
  command: cargo build --workspace --target wasm32-wasip2 --release --features gpu
dev:
  wasi_webgpu: true
```

- `--features gpu` in the build command makes `sar-processor` compile against the `sar-processor-gpu` WIT world, which imports `wasi:webgpu/webgpu@0.0.1`
- `wasi_webgpu: true` tells the wash runtime to activate its built-in WebGPU plugin (backed by `wgpu-core`)
- Without `--features gpu`, the component doesn't import webgpu and falls back to CPU CFAR automatically
- Without `wasi_webgpu: true`, the host won't provide the interface and the component will fail to link

## Building and Running

```bash
# Build (uses .wash/config.yaml build command)
wash build --skip-fetch

# Run dev server (port 8000, not 8080)
wash dev

# --skip-fetch is needed because wasi:webgpu WIT is vendored in wit/deps/
# (it's not in the OCI registry yet)
```

## WIT Dependencies

- `wasmcloud:messaging@0.2.0` — fetched from `wasmcloud.com` registry via `wash wit fetch`
- `wasi:webgpu@0.0.1` — **vendored** in `wit/deps/wasi-webgpu-0.0.1/` (not in OCI registry)
- `wasi:graphics-context@0.0.1` — vendored, transitive dep of webgpu
- `wasi:io@0.2.0` — vendored, transitive dep

Do NOT add `wit/deps/` to `.gitignore` — the webgpu WIT files must be committed since they can't be fetched from a registry.

## Key Modules

| File | Purpose |
|------|---------|
| `sar-processor/src/lib.rs` | Message handler, detection pipeline orchestration, connected component extraction |
| `sar-processor/src/gpu.rs` | `wasi:webgpu` device init, buffer management, CFAR compute dispatch (only compiled with `gpu` feature) |
| `sar-processor/src/cfar.wgsl` | WGSL compute shader — CA-CFAR algorithm, 16x16 workgroups |
| `sar-processor/src/cfar_cpu.rs` | CPU fallback implementation of the same CFAR algorithm |
| `sar-processor/src/synthetic.rs` | Synthetic SAR image + AIS data generators with ocean-only placement |
| `sar-processor/src/spatial_join.rs` | Haversine distance, AIS/SAR matching (500m threshold) |
| `sar-processor/src/ocean.rs` | Global land/ocean classifier from Natural Earth 110m coastlines (128 polygons) |
| `sar-processor/src/types.rs` | All shared types: DetectionRequest, VesselDetection, SarDetection, SizeClass, etc. |
| `sar-processor/src/geojson.rs` | DetectionResult → GeoJSON FeatureCollection conversion |
| `sar-processor/ui.html` | Full frontend SPA — MapLibre GL JS map, stats dashboard, draw tool, credential modals |

## Frontend

Single HTML file embedded via `include_str!("../ui.html")`. Contains:
- MapLibre GL JS map (dark theme, OSM tiles desaturated)
- Draw-bounding-box tool for custom regions
- CDSE OAuth + AISHub API key modals with cookie storage
- Ship silhouette SVGs by vessel type in popups
- SAR radar footprint rectangle in popups
- Size-scaled map markers (small/medium/large)
- Feet/meters unit toggle
- Animated stats dashboard with processing pipeline visualization

## Data Flow

1. Frontend calls `GET /api/detect?req=<urlencoded json>` with region/bbox, SAR size, seed, optional credentials
2. sar-processor's async P3 `wasi:http/handler` receives the request directly
3. sar-processor generates synthetic SAR image + AIS records (or would fetch real data with credentials)
4. CFAR runs on GPU (wasi:webgpu) or CPU (fallback)
5. Connected component labeling extracts ship detections with dimensions
6. Spatial join classifies: matched / dark_vessel / ais_only
7. sar-processor converts to GeoJSON FeatureCollection and responds (`geojson` module; `/detect` returns the raw DetectionResult for programmatic consumers)

## Default Region

Persian Gulf / Strait of Hormuz / Gulf of Oman. Map centers defined in `REGION_CENTERS` in ui.html and `get_region_bbox()` in types.rs.
