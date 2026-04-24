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
) -> (Vec<u32>, bool) {
    #[cfg(feature = "gpu")]
    {
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
    let bbox = get_region_bbox(&request.region);
    let cfar_params = CfarParams::default();

    // Generate synthetic SAR image
    let (sar_image, targets) = synthetic::generate_sar_image(
        request.sar_width,
        request.sar_height,
        request.num_targets,
        &bbox,
        request.seed,
    );

    // Generate synthetic AIS records
    let ais_records = synthetic::generate_ais_records(
        &targets,
        &bbox,
        (request.num_targets / 3).max(5),
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
    };

    Ok(DetectionResult { vessels, stats })
}

/// Extract ship detections from the binary mask using connected component labeling.
fn extract_detections_from_mask(
    mask: &[u32],
    width: u32,
    height: u32,
    bbox: &types::BoundingBox,
    sar_image: &[f32],
) -> Vec<types::SarDetection> {
    let mut visited = vec![false; mask.len()];
    let mut detections = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if mask[idx] == 0 || visited[idx] {
                continue;
            }

            let mut stack = vec![(x, y)];
            let mut pixels: Vec<(u32, u32)> = Vec::new();
            let mut max_intensity: f32 = 0.0;

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

            let intensity_db = 10.0 * log10_approx(max_intensity);
            let rcs = pixels.len() as f32 * max_intensity * 2.0;

            detections.push(types::SarDetection {
                lat,
                lon,
                intensity_db,
                rcs,
                pixel_x: cx as u32,
                pixel_y: cy as u32,
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
