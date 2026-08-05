//! GeoJSON conversion for the HTTP API (moved from the api-gateway when the
//! demo collapsed into a single component).

use crate::types::{DetectionResult, ProcessingStats, VesselStatus};
use serde::Serialize;

#[derive(Serialize)]
pub struct GeoJsonResponse<'a> {
    #[serde(rename = "type")]
    type_: &'static str,
    features: Vec<GeoJsonFeature>,
    stats: &'a ProcessingStats,
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

pub fn to_geojson(result: &DetectionResult) -> GeoJsonResponse<'_> {
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
                    size_class: v.sar.as_ref().map(|s| format!("{:?}", s.size_class).to_lowercase()),
                },
            }
        })
        .collect();

    GeoJsonResponse { type_: "FeatureCollection", features, stats: &result.stats }
}
