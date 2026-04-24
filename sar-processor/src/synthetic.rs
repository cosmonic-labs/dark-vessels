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

    /// Box-Muller transform for Gaussian noise
    fn gaussian(&mut self, mean: f32, std_dev: f32) -> f32 {
        let u1 = self.next_f32().max(1e-10);
        let u2 = self.next_f32();
        let z = (-2.0 * ln_approx(u1)).sqrt() * cos_approx(2.0 * core::f32::consts::PI * u2);
        mean + std_dev * z
    }
}

/// Fast ln approximation
fn ln_approx(x: f32) -> f32 {
    let bits = x.to_bits() as f32;
    let log2 = bits * 1.1920928955078125e-7 - 126.94269504;
    log2 * 0.6931471805599453
}

/// Fast cosine approximation (Bhaskara I)
fn cos_approx(mut x: f32) -> f32 {
    let pi = core::f32::consts::PI;
    let two_pi = 2.0 * pi;
    x = x % two_pi;
    if x < 0.0 {
        x += two_pi;
    }
    if x > pi {
        x = two_pi - x;
    }
    let x2 = x * x;
    let pi2 = pi * pi;
    (pi2 - 4.0 * x2) / (pi2 + x2)
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

/// Generate a synthetic SAR intensity image with ship targets.
/// Returns (image_data, placed_targets)
pub fn generate_sar_image(
    width: u32,
    height: u32,
    num_targets: u32,
    bbox: &BoundingBox,
    seed: u64,
) -> (Vec<f32>, Vec<SarTarget>) {
    let mut rng = Rng::new(seed);
    let size = (width * height) as usize;
    let mut image = Vec::with_capacity(size);

    // Generate sea clutter background (Gaussian noise in dB scale)
    for _ in 0..size {
        let clutter_db = rng.gaussian(-10.0, 2.5);
        image.push(from_db(clutter_db));
    }

    // Place ship targets as bright Gaussian blobs
    let mut targets = Vec::with_capacity(num_targets as usize);
    for _ in 0..num_targets {
        let cx = rng.range_u32(20, width - 20);
        let cy = rng.range_u32(20, height - 20);
        let ship_db = rng.range_f32(0.0, 8.0); // Ships are bright
        let ship_intensity = from_db(ship_db);
        let blob_radius = rng.range_u32(2, 5);
        let rcs = rng.range_f32(10.0, 500.0); // Radar cross section m^2

        // Draw Gaussian blob
        let r = blob_radius as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    let dist_sq = (dx * dx + dy * dy) as f32;
                    let sigma = blob_radius as f32 * 0.6;
                    let weight = (-dist_sq / (2.0 * sigma * sigma)).exp_approx();
                    let idx = py as usize * width as usize + px as usize;
                    image[idx] += ship_intensity * weight;
                }
            }
        }

        // Convert pixel to lat/lon
        let lat = bbox.min_lat + (cy as f64 / height as f64) * (bbox.max_lat - bbox.min_lat);
        let lon = bbox.min_lon + (cx as f64 / width as f64) * (bbox.max_lon - bbox.min_lon);

        targets.push(SarTarget {
            pixel_x: cx,
            pixel_y: cy,
            lat,
            lon,
            intensity_db: ship_db,
            rcs,
        });
    }

    (image, targets)
}

/// Generate synthetic AIS records.
/// `sar_targets` is used to create matching AIS records for some targets.
/// Returns AIS records where ~70% match SAR targets, ~30% are AIS-only.
pub fn generate_ais_records(
    sar_targets: &[SarTarget],
    bbox: &BoundingBox,
    extra_ais_count: u32,
    dark_vessel_ratio: f32,
    seed: u64,
) -> Vec<AisRecord> {
    let mut rng = Rng::new(seed.wrapping_add(42));
    let mut records = Vec::new();

    // For each SAR target, maybe create a matching AIS record
    for target in sar_targets {
        if rng.next_f32() < dark_vessel_ratio {
            // This is a dark vessel — no AIS record
            continue;
        }

        // Create matching AIS record with small position offset
        let lat_offset = rng.range_f64(-0.003, 0.003); // ~300m
        let lon_offset = rng.range_f64(-0.003, 0.003);

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
        });
    }

    // Add extra AIS-only vessels (no SAR detection)
    for _ in 0..extra_ais_count {
        let name_idx = rng.range_u32(0, VESSEL_NAMES.len() as u32 - 1) as usize;
        let type_idx = rng.range_u32(0, VESSEL_TYPES.len() as u32 - 1) as usize;
        let dest_idx = rng.range_u32(0, DESTINATIONS.len() as u32 - 1) as usize;
        let mmsi = rng.range_u32(200000000, 799999999);

        records.push(AisRecord {
            mmsi,
            name: VESSEL_NAMES[name_idx].into(),
            vessel_type: VESSEL_TYPES[type_idx].into(),
            lat: rng.range_f64(bbox.min_lat, bbox.max_lat),
            lon: rng.range_f64(bbox.min_lon, bbox.max_lon),
            heading: rng.range_f32(0.0, 360.0),
            speed_knots: rng.range_f32(0.5, 12.0),
            destination: DESTINATIONS[dest_idx].into(),
        });
    }

    records
}

/// Convert dB to linear power
fn from_db(db: f32) -> f32 {
    (db * 0.23025851).exp_approx() // 10^(db/10) = e^(db * ln(10)/10)
}

trait ExpApprox {
    fn exp_approx(self) -> Self;
}

impl ExpApprox for f32 {
    fn exp_approx(self) -> f32 {
        // Pade approximation, good for small |x|
        if self > 10.0 {
            return 22026.0;
        }
        if self < -10.0 {
            return 0.0;
        }
        let x = self;
        let x2 = x * x;
        let x3 = x2 * x;
        (120.0 + 60.0 * x + 12.0 * x2 + x3) / (120.0 - 60.0 * x + 12.0 * x2 - x3)
    }
}

