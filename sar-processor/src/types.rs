use serde::{Deserialize, Serialize};

/// Bounding box for a geographic region
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
}

/// A target placed in the synthetic SAR image
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SarTarget {
    pub pixel_x: u32,
    pub pixel_y: u32,
    pub lat: f64,
    pub lon: f64,
    pub intensity_db: f32,
    pub rcs: f32,
}

/// A ship detection from CFAR processing
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SarDetection {
    pub lat: f64,
    pub lon: f64,
    pub intensity_db: f32,
    pub rcs: f32,
    pub pixel_x: u32,
    pub pixel_y: u32,
    /// Estimated ship length in meters (from SAR pixel extent)
    pub length_m: f32,
    /// Estimated ship beam (width) in meters
    pub beam_m: f32,
    /// Number of detected pixels in the connected component
    pub pixel_count: u32,
    /// Size classification based on SAR dimensions
    pub size_class: SizeClass,
}

/// Ship size classification derived from SAR dimensions
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    /// < 50m length (fishing boats, dhows, small craft)
    Small,
    /// 50-200m length (cargo, tankers, ferries)
    Medium,
    /// > 200m length (VLCCs, container mega-ships, military capital ships)
    Large,
}

impl SizeClass {
    pub fn from_length_m(length: f32) -> Self {
        if length < 50.0 {
            SizeClass::Small
        } else if length < 200.0 {
            SizeClass::Medium
        } else {
            SizeClass::Large
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SizeClass::Small => "Small",
            SizeClass::Medium => "Medium",
            SizeClass::Large => "Large",
        }
    }
}

/// An AIS telemetry record
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AisRecord {
    pub mmsi: u32,
    pub name: String,
    pub vessel_type: String,
    pub lat: f64,
    pub lon: f64,
    pub heading: f32,
    pub speed_knots: f32,
    pub destination: String,
}

/// Classification of a vessel after spatial join
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VesselStatus {
    Matched,
    DarkVessel,
    AisOnly,
}

/// A classified vessel detection
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VesselDetection {
    pub status: VesselStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sar: Option<SarDetection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ais: Option<AisRecord>,
    pub lat: f64,
    pub lon: f64,
}

/// Processing statistics
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessingStats {
    pub sar_image_width: u32,
    pub sar_image_height: u32,
    pub total_pixels: u64,
    pub cfar_detections: u32,
    pub ais_records: u32,
    pub matched: u32,
    pub dark_vessels: u32,
    pub ais_only: u32,
    pub gpu_processing_ms: f64,
    pub total_processing_ms: f64,
    pub region: String,
    pub compute_backend: String,
    pub small_vessels: u32,
    pub medium_vessels: u32,
    pub large_vessels: u32,
}

/// Full detection result returned by the processor
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectionResult {
    pub vessels: Vec<VesselDetection>,
    pub stats: ProcessingStats,
}

/// Request to the SAR processor
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectionRequest {
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_sar_width")]
    pub sar_width: u32,
    #[serde(default = "default_sar_height")]
    pub sar_height: u32,
    #[serde(default = "default_num_targets")]
    pub num_targets: u32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// Custom bounding box (overrides region if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_bbox: Option<BoundingBox>,
    /// Copernicus CDSE access token (for real SAR data)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdse_token: Option<String>,
    /// AISHub API key (for real AIS data)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aishub_key: Option<String>,
}

fn default_region() -> String {
    "strait_of_hormuz".into()
}
fn default_sar_width() -> u32 {
    512
}
fn default_sar_height() -> u32 {
    512
}
fn default_num_targets() -> u32 {
    45
}
fn default_seed() -> u64 {
    12345
}

impl Default for DetectionRequest {
    fn default() -> Self {
        Self {
            region: default_region(),
            sar_width: default_sar_width(),
            sar_height: default_sar_height(),
            num_targets: default_num_targets(),
            seed: default_seed(),
            custom_bbox: None,
            cdse_token: None,
            aishub_key: None,
        }
    }
}

/// Predefined regions
pub fn get_region_bbox(name: &str) -> BoundingBox {
    match name {
        "persian_gulf" => BoundingBox {
            min_lat: 24.0,
            max_lat: 30.0,
            min_lon: 48.0,
            max_lon: 56.0,
        },
        "gulf_of_oman" => BoundingBox {
            min_lat: 22.5,
            max_lat: 26.5,
            min_lon: 56.5,
            max_lon: 62.0,
        },
        "bab_el_mandeb" => BoundingBox {
            min_lat: 11.5,
            max_lat: 13.5,
            min_lon: 42.5,
            max_lon: 44.5,
        },
        _ => BoundingBox {
            // strait_of_hormuz (default)
            min_lat: 25.0,
            max_lat: 27.0,
            min_lon: 55.0,
            max_lon: 57.5,
        },
    }
}

/// CFAR algorithm parameters
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfarParams {
    pub guard_cells: u32,
    pub training_cells: u32,
    pub threshold_factor: f32,
}

impl Default for CfarParams {
    fn default() -> Self {
        Self {
            guard_cells: 2,
            training_cells: 8,
            threshold_factor: 3.0,
        }
    }
}
