mod bindings {
    wit_bindgen::generate!({
        path: "../wit",
        world: "api-gateway",
        generate_all,
    });
}

use bindings::wasmcloud::messaging::consumer;

use serde::{Deserialize, Serialize};
use wstd::http::{Body, Request, Response, StatusCode};
use wstd::time::Duration;

static UI_HTML: &str = include_str!("../ui.html");
static LOGO_PNG: &[u8] = include_bytes!("../../icons/Cosmonic.Logo-Hrztl_Color.png");

#[wstd::http_server]
async fn main(req: Request<Body>) -> anyhow::Result<Response<Body>> {
    match (req.method().as_str(), req.uri().path()) {
        (_, "/") => serve_html(),
        ("GET", "/logo.png") => serve_logo(),
        ("GET", "/api/vessels") => get_vessels(req).await,
        ("POST", "/api/detect") => post_detect(req).await,
        _ => not_found(),
    }
}

fn serve_logo() -> anyhow::Result<Response<Body>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "image/png")
        .header("Cache-Control", "public, max-age=86400")
        .body(LOGO_PNG.to_vec().into())
        .map_err(Into::into)
}

fn serve_html() -> anyhow::Result<Response<Body>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(UI_HTML.into())
        .map_err(Into::into)
}

fn not_found() -> anyhow::Result<Response<Body>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body("Not found\n".into())
        .map_err(Into::into)
}

fn json_response(body: &[u8]) -> anyhow::Result<Response<Body>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(body.to_vec().into())
        .map_err(Into::into)
}

fn error_response(status: StatusCode, message: &str) -> anyhow::Result<Response<Body>> {
    let body = serde_json::json!({ "error": message }).to_string();
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(body.into())
        .map_err(Into::into)
}

/// GET /api/vessels — run detection with default params, return GeoJSON
async fn get_vessels(_req: Request<Body>) -> anyhow::Result<Response<Body>> {
    let request_body = b"{}";
    let timeout = Duration::from_secs(120).as_millis() as u32;

    match consumer::request("tasks.sar-processor", request_body, timeout) {
        Ok(resp) => {
            // Parse the DetectionResult and convert to GeoJSON
            match serde_json::from_slice::<DetectionResult>(&resp.body) {
                Ok(result) => {
                    let geojson = to_geojson(&result);
                    let body = serde_json::to_vec(&geojson)
                        .unwrap_or_else(|_| b"{}".to_vec());
                    json_response(&body)
                }
                Err(e) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("failed to parse processor response: {e}"),
                ),
            }
        }
        Err(e) => error_response(
            StatusCode::BAD_GATEWAY,
            &format!("processor error: {e}"),
        ),
    }
}

/// POST /api/detect — run detection with custom params
async fn post_detect(mut req: Request<Body>) -> anyhow::Result<Response<Body>> {
    let body_bytes = req.body_mut().contents().await?.to_vec();
    let timeout = Duration::from_secs(120).as_millis() as u32;

    match consumer::request("tasks.sar-processor", &body_bytes, timeout) {
        Ok(resp) => {
            match serde_json::from_slice::<DetectionResult>(&resp.body) {
                Ok(result) => {
                    let geojson = to_geojson(&result);
                    let body = serde_json::to_vec(&geojson)
                        .unwrap_or_else(|_| b"{}".to_vec());
                    json_response(&body)
                }
                Err(e) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("failed to parse processor response: {e}"),
                ),
            }
        }
        Err(e) => error_response(
            StatusCode::BAD_GATEWAY,
            &format!("processor error: {e}"),
        ),
    }
}

// --- Types mirroring sar-processor (for deserialization) ---

#[derive(Deserialize)]
struct DetectionResult {
    vessels: Vec<VesselDetection>,
    stats: ProcessingStats,
}

#[derive(Deserialize)]
struct VesselDetection {
    status: VesselStatus,
    sar: Option<SarDetection>,
    ais: Option<AisRecord>,
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum VesselStatus {
    Matched,
    DarkVessel,
    AisOnly,
}

#[derive(Deserialize, Serialize)]
struct SarDetection {
    lat: f64,
    lon: f64,
    intensity_db: f32,
    rcs: f32,
    pixel_x: u32,
    pixel_y: u32,
    length_m: f32,
    beam_m: f32,
    pixel_count: u32,
    size_class: String,
}

#[derive(Deserialize, Serialize)]
struct AisRecord {
    mmsi: u32,
    name: String,
    vessel_type: String,
    lat: f64,
    lon: f64,
    heading: f32,
    speed_knots: f32,
    destination: String,
}

#[derive(Deserialize, Serialize)]
struct ProcessingStats {
    sar_image_width: u32,
    sar_image_height: u32,
    total_pixels: u64,
    cfar_detections: u32,
    ais_records: u32,
    matched: u32,
    dark_vessels: u32,
    ais_only: u32,
    gpu_processing_ms: f64,
    total_processing_ms: f64,
    region: String,
    compute_backend: String,
    small_vessels: u32,
    medium_vessels: u32,
    large_vessels: u32,
}

// --- GeoJSON conversion ---

#[derive(Serialize)]
struct GeoJsonResponse {
    #[serde(rename = "type")]
    type_: &'static str,
    features: Vec<GeoJsonFeature>,
    stats: ProcessingStats,
}

#[derive(Serialize)]
struct GeoJsonFeature {
    #[serde(rename = "type")]
    type_: &'static str,
    geometry: GeoJsonGeometry,
    properties: GeoJsonProperties,
}

#[derive(Serialize)]
struct GeoJsonGeometry {
    #[serde(rename = "type")]
    type_: &'static str,
    coordinates: [f64; 2],
}

#[derive(Serialize)]
struct GeoJsonProperties {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mmsi: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vessel_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    heading: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed_knots: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    intensity_db: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rcs: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    length_m: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    beam_m: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pixel_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_class: Option<String>,
}

fn to_geojson(result: &DetectionResult) -> GeoJsonResponse {
    let features = result
        .vessels
        .iter()
        .map(|v| {
            let status = match v.status {
                VesselStatus::Matched => "matched",
                VesselStatus::DarkVessel => "dark_vessel",
                VesselStatus::AisOnly => "ais_only",
            };

            GeoJsonFeature {
                type_: "Feature",
                geometry: GeoJsonGeometry {
                    type_: "Point",
                    coordinates: [v.lon, v.lat],
                },
                properties: GeoJsonProperties {
                    status: status.to_string(),
                    mmsi: v.ais.as_ref().map(|a| a.mmsi),
                    name: v.ais.as_ref().map(|a| a.name.clone()),
                    vessel_type: v.ais.as_ref().map(|a| a.vessel_type.clone()),
                    heading: v.ais.as_ref().map(|a| a.heading),
                    speed_knots: v.ais.as_ref().map(|a| a.speed_knots),
                    destination: v.ais.as_ref().map(|a| a.destination.clone()),
                    intensity_db: v.sar.as_ref().map(|s| s.intensity_db),
                    rcs: v.sar.as_ref().map(|s| s.rcs),
                    length_m: v.sar.as_ref().map(|s| s.length_m),
                    beam_m: v.sar.as_ref().map(|s| s.beam_m),
                    pixel_count: v.sar.as_ref().map(|s| s.pixel_count),
                    size_class: v.sar.as_ref().map(|s| s.size_class.clone()),
                },
            }
        })
        .collect();

    GeoJsonResponse {
        type_: "FeatureCollection",
        features,
        stats: ProcessingStats {
            sar_image_width: result.stats.sar_image_width,
            sar_image_height: result.stats.sar_image_height,
            total_pixels: result.stats.total_pixels,
            cfar_detections: result.stats.cfar_detections,
            ais_records: result.stats.ais_records,
            matched: result.stats.matched,
            dark_vessels: result.stats.dark_vessels,
            ais_only: result.stats.ais_only,
            gpu_processing_ms: result.stats.gpu_processing_ms,
            total_processing_ms: result.stats.total_processing_ms,
            region: result.stats.region.clone(),
            compute_backend: result.stats.compute_backend.clone(),
            small_vessels: result.stats.small_vessels,
            medium_vessels: result.stats.medium_vessels,
            large_vessels: result.stats.large_vessels,
        },
    }
}
