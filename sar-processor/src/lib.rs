#[cfg(feature = "gpu")]
wit_bindgen::generate!({
    path: "../wit",
    world: "sar-processor-gpu",
    generate_all,
});

#[cfg(not(feature = "gpu"))]
wit_bindgen::generate!({
    path: "../wit",
    world: "sar-processor",
    generate_all,
});

pub mod types;
pub mod synthetic;
pub mod ocean;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod cfar_cpu;
pub mod spatial_join;

use crate::types::{
    DetectionRequest, DetectionResult, ProcessingStats, VesselStatus, get_region_bbox, CfarParams,
};
use crate::wasmcloud::messaging::types::BrokerMessage;
use wasmcloud::messaging::consumer;
#[allow(unused)]
use wstd::prelude::*;

struct Component;
export!(Component);

impl exports::wasmcloud::messaging::handler::Guest for Component {
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        let Some(subject) = msg.reply_to else {
            return Err("missing reply_to".to_string());
        };

        let request: DetectionRequest = if msg.body.is_empty() {
            DetectionRequest::default()
        } else {
            serde_json::from_slice(&msg.body)
                .map_err(|e| format!("invalid request JSON: {e}"))?
        };

        let result = run_detection_pipeline(&request)?;

        let response_json = serde_json::to_vec(&result)
            .map_err(|e| format!("failed to serialize result: {e}"))?;

        let reply = BrokerMessage {
            subject,
            body: response_json,
            reply_to: None,
        };

        consumer::publish(&reply)
    }
}

fn run_cfar(
    sar_image: &[f32],
    width: u32,
    height: u32,
    params: &CfarParams,
    force_cpu: bool,
) -> (Vec<u32>, bool) {
    #[cfg(feature = "gpu")]
    if !force_cpu {
        match gpu::run_cfar_gpu(sar_image, width, height, params) {
            Ok(mask) => return (mask, true),
            Err(_) => {}
        }
    }
    let mask = cfar_cpu::run_cfar_cpu(sar_image, width, height, params);
    (mask, false)
}

fn run_detection_pipeline(request: &DetectionRequest) -> Result<DetectionResult, String> {
    let start = wstd::time::Instant::now();
    let bbox = request.custom_bbox.clone().unwrap_or_else(|| get_region_bbox(&request.region));
    let cfar_params = CfarParams::default();

    // Scale target count based on bounding box area and density multiplier.
    // Base density: ~15 ships per degree^2 of ocean (busy shipping lane).
    let lat_span = bbox.max_lat - bbox.min_lat;
    let lon_span = bbox.max_lon - bbox.min_lon;
    let area_deg2 = lat_span * lon_span;
    let base_density = 15.0; // ships per degree^2
    let density_mult = request.density.unwrap_or(1.0) as f64;
    let auto_targets = ((area_deg2 * base_density * density_mult) as u32).max(5).min(500);
    let num_targets = if request.num_targets > 0 { request.num_targets } else { auto_targets };

    // Generate synthetic SAR image
    let (sar_image, targets) = synthetic::generate_sar_image(
        request.sar_width,
        request.sar_height,
        num_targets,
        &bbox,
        request.seed,
    );

    // Generate synthetic AIS records
    let ais_records = synthetic::generate_ais_records(
        &targets,
        &bbox,
        (num_targets / 3).max(5),
        0.2,
        request.seed,
    );

    // Run CFAR detection
    let cfar_start = wstd::time::Instant::now();
    let (detection_mask, used_gpu) = run_cfar(
        &sar_image,
        request.sar_width,
        request.sar_height,
        &cfar_params,
        request.force_cpu,
    );
    let cfar_elapsed = cfar_start.elapsed();

    // Extract ship detections from the mask
    let sar_detections = extract_detections_from_mask(
        &detection_mask,
        request.sar_width,
        request.sar_height,
        &bbox,
        &sar_image,
    );

    // Spatial join: match SAR detections with AIS records
    let vessels = spatial_join::spatial_join(&sar_detections, &ais_records, 500.0);

    let total_elapsed = start.elapsed();

    let matched = vessels.iter().filter(|v| matches!(v.status, VesselStatus::Matched)).count() as u32;
    let dark_vessels = vessels.iter().filter(|v| matches!(v.status, VesselStatus::DarkVessel)).count() as u32;
    let ais_only = vessels.iter().filter(|v| matches!(v.status, VesselStatus::AisOnly)).count() as u32;

    // Count by size class (only vessels with SAR data)
    let small_vessels = vessels.iter().filter(|v| v.sar.as_ref().is_some_and(|s| matches!(s.size_class, types::SizeClass::Small))).count() as u32;
    let medium_vessels = vessels.iter().filter(|v| v.sar.as_ref().is_some_and(|s| matches!(s.size_class, types::SizeClass::Medium))).count() as u32;
    let large_vessels = vessels.iter().filter(|v| v.sar.as_ref().is_some_and(|s| matches!(s.size_class, types::SizeClass::Large))).count() as u32;

    let stats = ProcessingStats {
        sar_image_width: request.sar_width,
        sar_image_height: request.sar_height,
        total_pixels: request.sar_width as u64 * request.sar_height as u64,
        cfar_detections: sar_detections.len() as u32,
        ais_records: ais_records.len() as u32,
        matched,
        dark_vessels,
        ais_only,
        gpu_processing_ms: cfar_elapsed.as_millis() as f64,
        total_processing_ms: total_elapsed.as_millis() as f64,
        region: request.region.clone(),
        compute_backend: if used_gpu { "WebGPU".into() } else { "CPU".into() },
        small_vessels,
        medium_vessels,
        large_vessels,
    };

    Ok(DetectionResult { vessels, stats })
}

/// Extract ship detections from the binary mask using connected component labeling.
/// Computes ship dimensions from pixel bounding box + SAR spatial resolution.
fn extract_detections_from_mask(
    mask: &[u32],
    width: u32,
    height: u32,
    bbox: &types::BoundingBox,
    sar_image: &[f32],
) -> Vec<types::SarDetection> {
    let mut visited = vec![false; mask.len()];
    let mut detections = Vec::new();

    // Sentinel-1 GRD pixel spacing is ~10m. Use this as the SAR resolution
    // for computing physical ship dimensions, regardless of how we map
    // pixels to the bounding box for geographic placement.
    let sar_resolution_m: f64 = 10.0;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if mask[idx] == 0 || visited[idx] {
                continue;
            }

            let mut stack = vec![(x, y)];
            let mut pixels: Vec<(u32, u32)> = Vec::new();
            let mut max_intensity: f32 = 0.0;
            let mut min_px = x; let mut max_px = x;
            let mut min_py = y; let mut max_py = y;

            while let Some((px, py)) = stack.pop() {
                let pidx = (py * width + px) as usize;
                if visited[pidx] || mask[pidx] == 0 {
                    continue;
                }
                visited[pidx] = true;
                pixels.push((px, py));
                let intensity = sar_image[pidx];
                if intensity > max_intensity {
                    max_intensity = intensity;
                }
                if px < min_px { min_px = px; }
                if px > max_px { max_px = px; }
                if py < min_py { min_py = py; }
                if py > max_py { max_py = py; }

                if px > 0 { stack.push((px - 1, py)); }
                if px + 1 < width { stack.push((px + 1, py)); }
                if py > 0 { stack.push((px, py - 1)); }
                if py + 1 < height { stack.push((px, py + 1)); }
            }

            if pixels.is_empty() {
                continue;
            }

            let cx: f64 = pixels.iter().map(|(px, _)| *px as f64).sum::<f64>() / pixels.len() as f64;
            let cy: f64 = pixels.iter().map(|(_, py)| *py as f64).sum::<f64>() / pixels.len() as f64;

            let lat = bbox.min_lat + (cy / height as f64) * (bbox.max_lat - bbox.min_lat);
            let lon = bbox.min_lon + (cx / width as f64) * (bbox.max_lon - bbox.min_lon);

            // Skip detections over land (real SAR would show land clutter)
            if !ocean::is_ocean(lat, lon) {
                continue;
            }

            let intensity_db = 10.0 * log10_approx(max_intensity);
            let rcs = pixels.len() as f32 * max_intensity * 2.0;

            // Compute physical dimensions using SAR pixel resolution
            let extent_px_x = (max_px - min_px + 1) as f64;
            let extent_px_y = (max_py - min_py + 1) as f64;
            // Length = major axis, beam = minor axis
            let dim_x_m = (extent_px_x * sar_resolution_m) as f32;
            let dim_y_m = (extent_px_y * sar_resolution_m) as f32;
            let length_m = dim_x_m.max(dim_y_m);
            let beam_m = dim_x_m.min(dim_y_m);
            let size_class = types::SizeClass::from_length_m(length_m);

            detections.push(types::SarDetection {
                lat,
                lon,
                intensity_db,
                rcs,
                pixel_x: cx as u32,
                pixel_y: cy as u32,
                length_m,
                beam_m,
                pixel_count: pixels.len() as u32,
                size_class,
            });
        }
    }

    detections
}

fn log10_approx(x: f32) -> f32 {
    if x <= 0.0 { return -30.0; }
    let bits = x.to_bits() as f32;
    let log2 = bits * 1.1920928955078125e-7 - 126.94269504;
    log2 * 0.30103
}
