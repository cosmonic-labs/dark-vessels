# Constraints and Known Limitations

This document outlines the architectural bottlenecks, scalability limits, and scientific simplifications (hallucinations) identified in the current implementation of Dark Vessels.

## 1. High-Level Architecture & Scalability

### GPU Resource Lifecycle
- **Constraint**: The `sar-processor` re-initializes the GPU adapter, device, and shader pipeline on every request.
- **Impact**: Initialization overhead (adapter request, device creation, WGSL compilation) negates much of the speedup for smaller images.
- **Recommendation**: Persist the `GpuDevice` and `GpuComputePipeline` in a long-lived state object.

### CPU Algorithmic Complexity
- **Constraint**: The CPU CA-CFAR implementation is $O(N \times K)$ where $K$ is the window size ($margin^2$). It re-calculates sums for every pixel.
- **Impact**: Processing time scales poorly with window size (currently ~22s for 4096px images).
- **Recommendation**: Implement an **Integral Image (Summed Area Table)** to achieve $O(1)$ window sum calculation per pixel.

### Spatial Join Performance
- **Constraint**: Vessel matching uses a nested-loop $O(N \times M)$ join.
- **Impact**: Performance degrades rapidly when matching thousands of SAR detections against a high-density AIS feed.
- **Recommendation**: Implement a spatial index (e.g., GeoHash grid or R-Tree).

---

## 2. Physical & Scientific Realism

### Land Mask Resolution
- **Constraint**: Uses Natural Earth 110m polygons for coastal masking.
- **Hallucination**: The resolution is insufficient for maritime surveillance. It results in false positives (piers/buildings detected as vessels) and false negatives (vessels near shore being masked as land).
- **Requirement**: Real-world application requires ~30m resolution (e.g., GSHHG or SRTM-based masks).

### SAR Signal Statistics
- **Constraint**: Synthetic SAR generation uses Gaussian noise for sea clutter.
- **Hallucination**: Real SAR imagery exhibits **Speckle Noise** (Gamma or K-distributed). CA-CFAR thresholds are mathematically tuned for non-Gaussian distributions.
- **Requirement**: Implement Exponential or Gamma distribution modeling for clutter to properly validate CFAR performance.

### RCS vs. Physical Dimensions
- **Constraint**: Vessel classification assumes a linear relationship between pixel clusters and physical ship size.
- **Hallucination**: SAR measures **Radar Cross Section (RCS)**. A small metallic vessel can appear "larger" and brighter than a large composite vessel due to "blooming" and corner-reflector effects.

---

## 3. Wasm & Runtime Constraints

### Memory Pressure
- **Constraint**: Processing $4096 \times 4096$ tiles involves large `f32` and `u32` arrays (64MB+ per buffer).
- **Impact**: Serializing large results into GeoJSON strings inside the `api-gateway` can lead to memory exhaustion and high latency.
- **Recommendation**: Move to a tiled processing approach or binary serialization (e.g., Protobuf/FlatBuffers).
- **Note**: Wasm memory can easily be tuned

### Trig Approximations
- **Constraint**: Uses Bhaskara I polynomial approximations for trigonometric functions.
- **Impact**: While efficient, it is less accurate than host-native trig functions available via WASI/standard libraries.
