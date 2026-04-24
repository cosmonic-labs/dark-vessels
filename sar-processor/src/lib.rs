wit_bindgen::generate!({
    path: "../wit",
    world: "sar-processor",
    generate_all,
});

pub mod types;
pub mod synthetic;
pub mod gpu;
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

fn run_detection_pipeline(request: &DetectionRequest) -> Result<DetectionResult, String> {
    let start = wstd::time::Instant::now();
    let bbox = get_region_bbox(&request.region);
    let cfar_params = CfarParams::default();

    // Generate synthetic SAR image
    let (sar_image, _targets) = synthetic::generate_sar_image(
        request.sar_width,
        request.sar_height,
        request.num_targets,
        &bbox,
        request.seed,
    );

    // Generate synthetic AIS records
    let ais_records = synthetic::generate_ais_records(
        &_targets,
        &bbox,
        (request.num_targets / 3).max(5), // extra AIS-only vessels
        0.2,                                // 20% dark vessel ratio
        request.seed,
    );

    // Run GPU CA-CFAR detection
    let gpu_start = wstd::time::Instant::now();
    let detection_mask = gpu::run_cfar_gpu(
        &sar_image,
        request.sar_width,
        request.sar_height,
        &cfar_params,
    )?;
    let gpu_elapsed = gpu_start.elapsed();

    // Extract ship detections from the mask
    let sar_detections = gpu::extract_detections(
        &detection_mask,
        request.sar_width,
        request.sar_height,
        &bbox,
        &sar_image,
    );

    // Spatial join: match SAR detections with AIS records
    let vessels = spatial_join::spatial_join(&sar_detections, &ais_records, 500.0);

    let total_elapsed = start.elapsed();

    // Compute stats
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
        gpu_processing_ms: gpu_elapsed.as_millis() as f64,
        total_processing_ms: total_elapsed.as_millis() as f64,
        region: request.region.clone(),
    };

    Ok(DetectionResult { vessels, stats })
}
