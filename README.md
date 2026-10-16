# Dark Vessels

**Server-side maritime surveillance powered by WebGPU compute shaders running inside WebAssembly on CNCF wasmCloud.**

Dark Vessels detects ships in synthetic Sentinel-1 SAR (Synthetic Aperture Radar) imagery using a GPU-accelerated CA-CFAR algorithm, then cross-references against AIS vessel telemetry to identify "dark vessels" — ships visible on radar but not broadcasting their identity.

---

## Why This Matters

This project demonstrates that **real signal processing workloads can run inside Wasm components** with GPU acceleration, secure sandboxing, and sub-second cold starts. Everything — HTTP server, embedded frontend, GPU compute, and global coastline data — ships as a single `.wasm` file of **414 KB**.

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
                        wasmCloud Host (>= v2.6.1)
    ┌──────────────────────────────────────────────────────────┐
    │                                                          │
    │  ┌────────────────────────────────────────────────────┐  │
    │  │               sar-processor.wasm (~414 KB)         │  │
    │  │                                                    │  │
    │  │  Exports:                                          │  │
    │  │   wasi:http/handler@0.3.0 (async, P3)              │  │
    │  │     GET /              embedded SPA (MapLibre GL)  │  │
    │  │     GET /api/vessels   detection -> GeoJSON        │  │
    │  │     GET /api/detect    custom detection -> GeoJSON │  │
    │  │   wasmcloud:messaging/handler@0.2.0 (CPU-only)     │  │
    │  │                                                    │  │
    │  │  Imports:                                          │  │
    │  │   wasi:webgpu/webgpu@0.3.0-rc.2 (async)            │  │
    │  │   wasmcloud:messaging/consumer@0.2.0               │  │
    │  │                                                    │  │
    │  │  ┌──────────────────────┐                          │  │
    │  │  │ WGSL Compute Shader  │──► wgpu-core ──► GPU     │  │
    │  │  │ (CA-CFAR detection)  │    (Metal / Vulkan / DX) │  │
    │  │  └──────────────────────┘                          │  │
    │  └────────────────────────────────────────────────────┘  │
    │                                                          │
    └──────────────────────────────────────────────────────────┘
```

One workload, one component. Detection lives behind the **async**
`wasi:http/handler@0.3.0` export because `wasi:webgpu@0.3.0-rc.2` declares
its acquisition/map functions `async func` — a sync-lifted export (like the
messaging handler) cannot await them, so the messaging trigger remains as a
CPU-only compatibility surface on subject `tasks.sar-processor`.


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
| `sar-processor.wasm` | **414 KB** | < 10ms | `wasi:http/handler@0.3.0` (async), `wasmcloud:messaging/handler` | `wasi:webgpu/webgpu`, `wasmcloud:messaging/consumer` |

**414 KB** for a complete maritime surveillance system with GPU compute, HTTP server, embedded frontend, and global coastline data.

### Secure Sandbox

Each Wasm component runs in a **sandboxed execution environment** enforced by the WebAssembly runtime:

- **No filesystem access** — components cannot read or write to the host filesystem
- **No network access** except through declared WIT imports — `sar-processor` can only answer HTTP, use WebGPU, and exchange messages; outbound HTTP is further scoped by the deployment's `allowedHosts` allowlist
- **No ambient authority** — capabilities are explicitly linked at deployment time
- **Memory isolation** — each component has its own linear memory, no shared state
- **Deny by default** — if an import isn't declared in the WIT world and linked by the host, it doesn't exist

The host provides only the interfaces the component declares it needs. A compromised `sar-processor` cannot open sockets, access the filesystem, or reach any host not on its `allowedHosts` list. This is not a container boundary — it's a **language-level capability sandbox** enforced by the Wasm type system.

---

### Principle of Least Authority (POLA)

WebAssembly starts each sandbox with full adherance to the principle of least authority; with zero capabilities. Each capability - network access, file system, system call, etc. but be explicitly granted.  This provides an additional layer of protection against remote code execution, lateral movement, data exfiltraiton, and other security risks:

The component's WIT world is its complete capability surface — every import and export is declared, linkable only by the host, and visible to inspection:

Inspecting `wasm-tools component wit ./sar_processor.wasm`

```
package root:component;

world root {
  import wasi:webgpu/webgpu@0.3.0-rc.2;
  import wasmcloud:messaging/types@0.2.0;
  import wasmcloud:messaging/consumer@0.2.0;
  import wasi:http/types@0.3.0;
  import wasi:io/poll@0.2.9;
  import wasi:clocks/monotonic-clock@0.2.9;
  import wasi:io/error@0.2.9;
  import wasi:io/streams@0.2.9;
  import wasi:cli/stdout@0.2.9;
  import wasi:cli/stderr@0.2.9;
  import wasi:cli/stdin@0.2.9;
  import wasi:cli/environment@0.2.9;
  import wasi:cli/exit@0.2.9;
  import wasi:cli/terminal-input@0.2.9;
  import wasi:cli/terminal-output@0.2.9;
  import wasi:cli/terminal-stdin@0.2.9;
  import wasi:cli/terminal-stdout@0.2.9;
  import wasi:cli/terminal-stderr@0.2.9;

  export wasmcloud:messaging/handler@0.2.0;
  export wasi:http/handler@0.3.0;
}
```

Note what is *absent*: no filesystem, no sockets, no ambient network. The only compute and I/O this component can perform is what the world above declares and the host chooses to link.

---

## Quick Start

```bash
# Prerequisites: wash v2.6.1+ (wasi:webgpu@0.3.0-rc.2 + P3 HTTP), Rust with wasm32-wasip2 target
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

## Deploying on Cosmonic Desktop

The easiest way to run Dark Vessels: **one workload, ready to apply** at
[`deploy/desktop/workload.yaml`](./deploy/desktop/workload.yaml). Apply it
with the Cosmonic Desktop UI or the `cosmonic` MCP server
(`cosmonic_apply_workload`), then open
<http://dark-vessels.localhost.cosmonic.sh:8200/>.

> The wasmCloud runtime dispatches `wasi:http` to only **one component per
> workload**, and the GPU path must enter through `sar-processor`'s async P3
> HTTP export — so the single self-contained component is the correct
> single-workload shape. Every deploy target below uses the same ONE
> workload / ONE component layout.

---

## Deploying to Kubernetes

Dark Vessels has manifests for two Kubernetes targets:

- **Cosmonic Control** — `deploy/control/httptrigger.yaml` (recommended; demoed below)
- **Vanilla wasmCloud** — `deploy/wasmcloud/workloaddeployment.yaml` (the `runtime-operator` chart path; documented further down)

### Cosmonic Control (kind)

`deploy/control/httptrigger.yaml` declares one `WorkloadDeployment` running the
`sar-processor` component, which serves the SPA and the detection API and
keeps the messaging surface as a CPU-only compatibility trigger on
`tasks.sar-processor`, plus a `control.cosmonic.io/v1alpha2` `HTTPTrigger`
that routes `darkvessels.localhost.cosmonic.sh` to it. The manifest needs
Cosmonic Control v0.13.0 or later.

The `sar-processor` component imports `wasi:webgpu`, so the hostgroup needs
`--set gpu=true` — without it the workload sticks at `WORKLOAD_STATE_ERROR`
because the host doesn't advertise the WebGPU interface.

```bash
# 1. Kind cluster with port 80/443 forwarded to Traefik's NodePorts
cat > kind-config.yaml <<'EOF'
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
  - role: control-plane
    extraPortMappings:
      - containerPort: 30080
        hostPort: 80
        protocol: TCP
      - containerPort: 30443
        hostPort: 443
        protocol: TCP
EOF
kind create cluster --config ./kind-config.yaml

# 2. Cosmonic Control + hostgroup (gpu=true is required for sar-processor)
helm upgrade --install cosmonic-control oci://ghcr.io/cosmonic/cosmonic-control \
  --version 0.13.0 \
  --namespace cosmonic-system --create-namespace \
  --set 'ingress.hosts[0].host=darkvessels.localhost.cosmonic.sh'

helm upgrade --install hostgroup oci://ghcr.io/cosmonic/cosmonic-control-hostgroup \
  --version 0.13.0 --namespace cosmonic-system --set gpu=true

# 3. Apply the manifests
kubectl apply -f https://raw.githubusercontent.com/cosmonic-labs/dark-vessels/main/deploy/control/httptrigger.yaml
```

`*.localhost.cosmonic.sh` resolves to `127.0.0.1`, so once the workloads are
ready, browse to <http://darkvessels.localhost.cosmonic.sh>.

### Vanilla wasmCloud (`runtime-operator` chart)

A ready-to-apply manifest is at [`deploy/wasmcloud/workloaddeployment.yaml`](./deploy/wasmcloud/workloaddeployment.yaml).

### Requirements

- A Kubernetes cluster with the wasmCloud `runtime-operator` chart installed
- A host group with WebGPU enabled — pass `--set runtime.hostGroups[0].webgpu.enabled=true` to the chart so the wash host starts with `--wasi-webgpu`
- A node with a usable GPU exposed to the host pod (Metal/Vulkan/DX12 via `wgpu`); on managed clusters this typically means a GPU node pool plus the appropriate device-plugin daemonset

### Install + apply

```bash
helm install wasmcloud oci://ghcr.io/wasmcloud/charts/runtime-operator \
  --version 2.0.4 \
  --namespace wasmcloud --create-namespace \
  --set runtime.hostGroups[0].webgpu.enabled=true \
  --set gateway.service.type=NodePort \
  --set gateway.service.nodePort=30950

kubectl apply -f deploy/wasmcloud/workloaddeployment.yaml
```

The `runtime-gateway` (installed by the chart) routes by `Host` header. Once the `WorkloadDeployment` reaches `Ready=True`:

```bash
curl -H 'Host: dark-vessels.localhost.direct' http://<gateway-host>/
curl -H 'Host: dark-vessels.localhost.direct' \
  'http://<gateway-host>/api/detect?req=%7B%7D'
```

### Alternative: CPU-only deployment

If your cluster does not have a GPU-capable node or the chart's `webgpu.enabled` flag is unavailable, you can deploy the CPU-only build instead. This drops the runtime GPU/CPU UI toggle (the component no longer imports `wasi:webgpu`) but keeps the rest of the pipeline intact: the same CA-CFAR algorithm runs in pure Rust on the CPU.

Build and push the CPU-only image:

```bash
cargo build --workspace --target wasm32-wasip2 --release --no-default-features
wash oci push <REGISTRY>/dark-vessels/sar-processor:0.1.0-cpu \
  target/wasm32-wasip2/release/sar_processor.wasm
```

Then in `deploy/wasmcloud/workloaddeployment.yaml`, change the `dark-vessels-sar` workload's image tag from `0.1.0` to `0.1.0-cpu` and reinstall the chart without the `webgpu.enabled` flag.

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
├── sar-processor/        The component: HTTP server + frontend + GPU compute
│   ├── ui.html           Full SPA (MapLibre, stats, draw tool, modals)
│   ├── src/lib.rs        HTTP routing, pipeline orchestration, detection extraction
│   ├── src/geojson.rs    DetectionResult → GeoJSON conversion
│   ├── src/gpu.rs        wasi:webgpu device init, buffer mgmt, dispatch
│   ├── src/cfar.wgsl     WGSL compute shader (CA-CFAR algorithm)
│   ├── src/cfar_cpu.rs   CPU fallback (same algorithm)
│   ├── src/synthetic.rs  SAR image + AIS data generators
│   ├── src/spatial_join.rs  Haversine matching (500m threshold)
│   ├── src/ocean.rs      Global land/ocean mask (Natural Earth 110m)
│   └── src/types.rs      Shared types and region definitions
├── deploy/               Single-workload manifests (desktop, control, wasmcloud)
├── wit/                  WIT interface definitions
│   ├── world.wit         Component worlds (sar-processor, sar-processor-gpu)
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

*Built with wasmCloud 2.0 to demonstrate that WebAssembly components can handle complex, GPU-accelerated geospatial workloads — securely, portably, and in under 420 KB.*
