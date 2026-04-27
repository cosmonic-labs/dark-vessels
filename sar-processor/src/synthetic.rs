use crate::ocean;
use crate::types::{AisRecord, BoundingBox, SarTarget};

/// Simple LCG PRNG (no external deps needed in WASI)
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(1),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }

    fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }

    fn range_u32(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next_u64() % (hi - lo + 1) as u64) as u32
    }
}

pub struct SarTargetInternal {
    pub target: SarTarget,
    pub ship_intensity: f32,
    pub blob_radius_x: u32,
    pub blob_radius_y: u32,
}

const VESSEL_NAMES: &[&str] = &[
    "Al Jazeera Star", "Gulf Pioneer", "Hormuz Trader", "Oman Pearl",
    "Dubai Express", "Bahrain Spirit", "Qatar Voyager", "Abu Dhabi Crown",
    "Sharjah Fortune", "Muscat Dawn", "Bandar Abbas", "Kish Islander",
    "Fujairah Grace", "Ras Tanura", "Kharg Carrier", "Sohar Merchant",
    "Persian Pride", "Arabian Sea", "Strait Runner", "Dhow Master",
    "Kuwait Dignity", "Basra Horizon", "Chabahar Wind", "Lavan Glory",
    "Qeshm Tide", "Sirri Shadow", "Farsi Phantom", "Tunb Sentinel",
    "Hengam Drift", "Mina Salman", "Jebel Ali Star", "Deira Nomad",
    "Salalah Breeze", "Sur Mariner", "Nizwa Thunder", "Rustaq Wave",
    "Socotra Ghost", "Masirah Mist", "Halaniyat Reef", "Duqm Anchor",
];

const VESSEL_TYPES: &[&str] = &[
    "tanker", "tanker", "tanker", "cargo", "cargo",
    "container", "container", "fishing", "fishing", "military",
];

const DESTINATIONS: &[&str] = &[
    "Jebel Ali, UAE", "Fujairah, UAE", "Bandar Abbas, Iran",
    "Ras Tanura, KSA", "Kuwait City, Kuwait", "Basra, Iraq",
    "Muscat, Oman", "Sohar, Oman", "Doha, Qatar", "Bahrain",
    "Chabahar, Iran", "Karachi, Pakistan", "Mumbai, India",
];

/// Generate ship targets for the entire requested area.
pub fn generate_targets(
    full_width: u32,
    full_height: u32,
    num_targets: u32,
    bbox: &BoundingBox,
    seed: u64,
) -> Vec<SarTargetInternal> {
    let mut rng = Rng::new(seed);
    let mut targets = Vec::with_capacity(num_targets as usize);

    for _ in 0..num_targets {
        let mut cx = 0u32;
        let mut cy = 0u32;
        let mut on_water = false;
        for _ in 0..20 {
            cx = rng.range_u32(20, full_width - 20);
            cy = rng.range_u32(20, full_height - 20);
            let lat = bbox.min_lat + (cy as f64 / full_height as f64) * (bbox.max_lat - bbox.min_lat);
            let lon = bbox.min_lon + (cx as f64 / full_width as f64) * (bbox.max_lon - bbox.min_lon);
            if ocean::is_ocean(lat, lon) {
                on_water = true;
                break;
            }
        }
        if !on_water { continue; }

        let ship_intensity = rng.range_f32(1.5, 8.0);
        let size_roll = rng.next_f32();
        let (blob_radius_x, blob_radius_y) = if size_roll < 0.35 {
            (rng.range_u32(1, 2), rng.range_u32(1, 1))
        } else if size_roll < 0.75 {
            let len = rng.range_u32(3, 9);
            (len, rng.range_u32(1, len / 3 + 1))
        } else {
            let len = rng.range_u32(10, 17);
            (len, rng.range_u32(2, len / 4 + 1))
        };
        let rcs = rng.range_f32(10.0, 500.0);

        let lat = bbox.min_lat + (cy as f64 / full_height as f64) * (bbox.max_lat - bbox.min_lat);
        let lon = bbox.min_lon + (cx as f64 / full_width as f64) * (bbox.max_lon - bbox.min_lon);

        targets.push(SarTargetInternal {
            target: SarTarget {
                pixel_x: cx,
                pixel_y: cy,
                lat,
                lon,
                intensity_db: 10.0 * (ship_intensity.log10()),
                rcs,
            },
            ship_intensity,
            blob_radius_x,
            blob_radius_y,
        });
    }
    targets
}

/// Render a specific tile of the SAR image.
pub fn render_tile(
    targets: &[SarTargetInternal],
    _full_width: u32,
    _full_height: u32,
    tile_x: u32,
    tile_y: u32,
    tile_width: u32,
    tile_height: u32,
    seed: u64,
) -> Vec<f32> {
    // Deterministic seed for this tile's background noise
    let tile_seed = seed.wrapping_add(tile_y as u64 * 0xFFFF).wrapping_add(tile_x as u64);
    let mut rng = Rng::new(tile_seed);
    
    let size = (tile_width * tile_height) as usize;
    let mut image = Vec::with_capacity(size);

    for _ in 0..size {
        let base = 0.1 + rng.next_f32() * 0.08;
        image.push(base);
    }

    for internal in targets {
        let t = &internal.target;
        // Check if target blob could overlap with this tile
        let margin = internal.blob_radius_x.max(internal.blob_radius_y) as i32;
        let tx = t.pixel_x as i32;
        let ty = t.pixel_y as i32;
        
        if tx + margin < tile_x as i32 || tx - margin >= (tile_x + tile_width) as i32 ||
           ty + margin < tile_y as i32 || ty - margin >= (tile_y + tile_height) as i32 {
            continue;
        }

        let rx = internal.blob_radius_x as i32;
        let ry = internal.blob_radius_y as i32;
        let sigma_x = internal.blob_radius_x as f32 * 0.6;
        let sigma_y = internal.blob_radius_y as f32 * 0.6;
        
        for dy in -ry..=ry {
            for dx in -rx..=rx {
                let gx = tx + dx; // Global x
                let gy = ty + dy; // Global y
                
                // If the pixel is within the current tile
                if gx >= tile_x as i32 && gx < (tile_x + tile_width) as i32 &&
                   gy >= tile_y as i32 && gy < (tile_y + tile_height) as i32 {
                    let local_x = (gx - tile_x as i32) as u32;
                    let local_y = (gy - tile_y as i32) as u32;
                    
                    let nx = dx as f32 / sigma_x;
                    let ny = dy as f32 / sigma_y;
                    let weight = f32::exp(-0.5 * (nx * nx + ny * ny));
                    let idx = (local_y * tile_width + local_x) as usize;
                    image[idx] += internal.ship_intensity * weight;
                }
            }
        }
    }

    image
}

/// Legacy wrapper for single-tile generation (if still used)
pub fn generate_sar_image(
    width: u32,
    height: u32,
    num_targets: u32,
    bbox: &BoundingBox,
    seed: u64,
) -> (Vec<f32>, Vec<SarTarget>) {
    let internal_targets = generate_targets(width, height, num_targets, bbox, seed);
    let image = render_tile(&internal_targets, width, height, 0, 0, width, height, seed);
    let targets = internal_targets.into_iter().map(|it| it.target).collect();
    (image, targets)
}

/// Generate synthetic AIS records.
pub fn generate_ais_records(
    sar_targets: &[SarTarget],
    bbox: &BoundingBox,
    extra_ais_count: u32,
    dark_vessel_ratio: f32,
    seed: u64,
) -> Vec<AisRecord> {
    let mut rng = Rng::new(seed.wrapping_add(42));
    let mut records = Vec::new();

    for target in sar_targets {
        if rng.next_f32() < dark_vessel_ratio {
            continue;
        }

        let lat_offset = rng.range_f64(-0.001, 0.001);
        let lon_offset = rng.range_f64(-0.001, 0.001);

        let name_idx = rng.range_u32(0, VESSEL_NAMES.len() as u32 - 1) as usize;
        let type_idx = rng.range_u32(0, VESSEL_TYPES.len() as u32 - 1) as usize;
        let dest_idx = rng.range_u32(0, DESTINATIONS.len() as u32 - 1) as usize;
        let mmsi = rng.range_u32(200000000, 799999999);

        records.push(AisRecord {
            mmsi,
            name: VESSEL_NAMES[name_idx].into(),
            vessel_type: VESSEL_TYPES[type_idx].into(),
            lat: target.lat + lat_offset,
            lon: target.lon + lon_offset,
            heading: rng.range_f32(0.0, 360.0),
            speed_knots: rng.range_f32(0.5, 18.0),
            destination: DESTINATIONS[dest_idx].into(),
            on_land: false,
        });
    }

    for _ in 0..extra_ais_count {
        let mut lat;
        let mut lon;
        let mut found = false;
        for _ in 0..20 {
            lat = rng.range_f64(bbox.min_lat, bbox.max_lat);
            lon = rng.range_f64(bbox.min_lon, bbox.max_lon);
            if ocean::is_ocean(lat, lon) {
                found = true;
                let name_idx = rng.range_u32(0, VESSEL_NAMES.len() as u32 - 1) as usize;
                let type_idx = rng.range_u32(0, VESSEL_TYPES.len() as u32 - 1) as usize;
                let dest_idx = rng.range_u32(0, DESTINATIONS.len() as u32 - 1) as usize;
                let mmsi = rng.range_u32(200000000, 799999999);
                records.push(AisRecord {
                    mmsi,
                    name: VESSEL_NAMES[name_idx].into(),
                    vessel_type: VESSEL_TYPES[type_idx].into(),
                    lat,
                    lon,
                    heading: rng.range_f32(0.0, 360.0),
                    speed_knots: rng.range_f32(0.5, 12.0),
                    destination: DESTINATIONS[dest_idx].into(),
                    on_land: false,
                });
                break;
            }
        }
        if !found { continue; }
    }

    records
}
