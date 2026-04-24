use crate::types::{AisRecord, SarDetection, VesselDetection, VesselStatus};

const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Haversine distance between two points in meters.
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let lat1_r = lat1.to_radians();
    let lat2_r = lat2.to_radians();

    let a = sin_approx(dlat / 2.0).powi(2)
        + cos_approx(lat1_r) * cos_approx(lat2_r) * sin_approx(dlon / 2.0).powi(2);
    let c = 2.0 * atan2_approx(a.sqrt(), (1.0 - a).sqrt());

    EARTH_RADIUS_M * c
}

/// Perform spatial join between SAR detections and AIS records.
/// Returns classified vessel detections.
pub fn spatial_join(
    sar: &[SarDetection],
    ais: &[AisRecord],
    threshold_m: f64,
) -> Vec<VesselDetection> {
    let mut results = Vec::new();
    let mut ais_matched = vec![false; ais.len()];

    // For each SAR detection, find the nearest AIS record within threshold
    for sar_det in sar {
        let mut best_dist = f64::MAX;
        let mut best_idx: Option<usize> = None;

        for (i, ais_rec) in ais.iter().enumerate() {
            if ais_matched[i] {
                continue;
            }
            let dist = haversine_distance(sar_det.lat, sar_det.lon, ais_rec.lat, ais_rec.lon);
            if dist < best_dist && dist <= threshold_m {
                best_dist = dist;
                best_idx = Some(i);
            }
        }

        match best_idx {
            Some(idx) => {
                ais_matched[idx] = true;
                results.push(VesselDetection {
                    status: VesselStatus::Matched,
                    lat: sar_det.lat,
                    lon: sar_det.lon,
                    sar: Some(sar_det.clone()),
                    ais: Some(ais[idx].clone()),
                });
            }
            None => {
                // Dark vessel — SAR detection with no AIS match
                results.push(VesselDetection {
                    status: VesselStatus::DarkVessel,
                    lat: sar_det.lat,
                    lon: sar_det.lon,
                    sar: Some(sar_det.clone()),
                    ais: None,
                });
            }
        }
    }

    // Add unmatched AIS records as AIS-only anomalies
    for (i, ais_rec) in ais.iter().enumerate() {
        if !ais_matched[i] {
            results.push(VesselDetection {
                status: VesselStatus::AisOnly,
                lat: ais_rec.lat,
                lon: ais_rec.lon,
                sar: None,
                ais: Some(ais_rec.clone()),
            });
        }
    }

    results
}

fn sin_approx(x: f64) -> f64 {
    // Use the standard libm sin since WASI provides it
    let x = x % (2.0 * core::f64::consts::PI);
    // Bhaskara I approximation
    let pi = core::f64::consts::PI;
    let mut x = x;
    if x < 0.0 { x += 2.0 * pi; }
    let sign = if x > pi { x -= pi; -1.0 } else { 1.0 };
    sign * (16.0 * x * (pi - x)) / (5.0 * pi * pi - 4.0 * x * (pi - x))
}

fn cos_approx(x: f64) -> f64 {
    sin_approx(x + core::f64::consts::PI / 2.0)
}

fn atan2_approx(y: f64, x: f64) -> f64 {
    let pi = core::f64::consts::PI;
    if x == 0.0 {
        if y > 0.0 { return pi / 2.0; }
        if y < 0.0 { return -pi / 2.0; }
        return 0.0;
    }
    let a = y / x;
    let s = a / (1.0 + a * a).sqrt_approx();
    let mut r = asin_approx(s);
    if x < 0.0 {
        r = if y >= 0.0 { pi - r } else { -pi - r };
    }
    r
}

fn asin_approx(x: f64) -> f64 {
    // Polynomial approximation for |x| <= 1
    let x2 = x * x;
    x * (1.0 + x2 * (1.0 / 6.0 + x2 * (3.0 / 40.0 + x2 * 15.0 / 336.0)))
}

trait SqrtApprox64 {
    fn sqrt_approx(self) -> Self;
}

impl SqrtApprox64 for f64 {
    fn sqrt_approx(self) -> f64 {
        if self <= 0.0 { return 0.0; }
        let mut g = self * 0.5;
        for _ in 0..10 {
            g = 0.5 * (g + self / g);
        }
        g
    }
}
