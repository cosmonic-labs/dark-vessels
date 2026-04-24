/// Simple ocean/land classifier for the Persian Gulf and surrounding regions.
/// Uses a set of simplified coastal boundary polygons to determine if a
/// lat/lon point is over water. Not globally accurate — tuned for the
/// demo regions (Persian Gulf, Strait of Hormuz, Gulf of Oman, Bab el-Mandeb).

/// Returns true if the point is likely over water (not land).
pub fn is_ocean(lat: f64, lon: f64) -> bool {
    // Quick rejection: if clearly outside all supported regions, assume ocean
    // (conservative — better to show a point than wrongly filter it)
    if lat < 10.0 || lat > 32.0 || lon < 38.0 || lon > 65.0 {
        return true;
    }

    // Check against simplified land polygons
    !is_land(lat, lon)
}

fn is_land(lat: f64, lon: f64) -> bool {
    // Iran (north shore of Persian Gulf + Gulf of Oman coast)
    if point_in_polygon(lat, lon, &IRAN_COAST) { return true; }
    // UAE + Oman (Musandam peninsula and south shore)
    if point_in_polygon(lat, lon, &UAE_OMAN_COAST) { return true; }
    // Saudi Arabia (west shore of Persian Gulf)
    if point_in_polygon(lat, lon, &SAUDI_COAST) { return true; }
    // Qatar peninsula
    if point_in_polygon(lat, lon, &QATAR) { return true; }
    // Oman (south of Gulf of Oman)
    if point_in_polygon(lat, lon, &OMAN_SOUTH) { return true; }
    // Yemen/Djibouti (Bab el-Mandeb)
    if point_in_polygon(lat, lon, &YEMEN_COAST) { return true; }
    if point_in_polygon(lat, lon, &DJIBOUTI_COAST) { return true; }

    false
}

/// Ray-casting point-in-polygon test.
fn point_in_polygon(lat: f64, lon: f64, polygon: &[(f64, f64)]) -> bool {
    let n = polygon.len();
    if n < 3 { return false; }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (yi, xi) = polygon[i];
        let (yj, xj) = polygon[j];
        if ((yi > lat) != (yj > lat)) && (lon < (xj - xi) * (lat - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// Simplified coastal polygons: (lat, lon) vertices
// These are rough approximations — enough to filter obvious land points
// in the demo. Vertices trace the coastline clockwise, then close inland.

/// Iran — north coast of Persian Gulf and Strait of Hormuz
static IRAN_COAST: [(f64, f64); 16] = [
    (30.0, 48.0),  // Iraq-Iran border at Shatt al-Arab
    (29.8, 49.0),  // Abadan area
    (29.0, 50.5),  // Bushehr approach
    (28.5, 51.0),  // Bushehr
    (27.5, 52.0),  // Kangan
    (27.0, 53.0),  // Assaluyeh
    (26.5, 54.0),  // Bandar Lengeh approach
    (26.5, 55.5),  // Bandar Lengeh
    (27.0, 56.0),  // Qeshm north
    (26.5, 56.5),  // Strait narrows
    (25.8, 57.5),  // Jask approach
    (25.5, 58.5),  // Jask
    (25.3, 60.0),  // Chabahar approach
    (25.5, 61.5),  // Chabahar
    (27.0, 62.0),  // Interior (close polygon inland)
    (31.0, 48.0),  // Interior (close polygon inland)
];

/// UAE and northern Oman (south shore of Persian Gulf + Musandam)
static UAE_OMAN_COAST: [(f64, f64); 12] = [
    (24.0, 51.5),  // UAE-Saudi border
    (24.5, 52.5),  // Abu Dhabi coast
    (24.8, 54.0),  // Abu Dhabi
    (25.0, 55.0),  // Dubai approach
    (25.3, 55.3),  // Dubai/Sharjah
    (25.5, 56.0),  // Ras al-Khaimah
    (26.0, 56.2),  // Musandam tip
    (26.2, 56.5),  // Musandam east
    (25.5, 56.5),  // Musandam south
    (24.5, 56.8),  // Fujairah
    (23.5, 58.5),  // Oman coast continues south
    (22.0, 51.5),  // Interior (close polygon inland)
];

/// Saudi Arabia — west coast of Persian Gulf
static SAUDI_COAST: [(f64, f64); 10] = [
    (29.0, 48.0),  // Kuwait-Saudi border area
    (28.5, 48.5),  // Jubail approach
    (27.0, 49.5),  // Jubail/Ras Tanura
    (26.5, 50.0),  // Dammam
    (26.0, 50.2),  // Dhahran coast
    (25.5, 50.5),  // Near Bahrain
    (25.0, 50.8),  // South of Bahrain
    (24.5, 51.0),  // UAE border approach
    (24.0, 51.5),  // UAE border
    (30.0, 46.0),  // Interior
];

/// Qatar peninsula
static QATAR: [(f64, f64); 8] = [
    (25.0, 50.8),  // South
    (25.3, 50.7),  // West coast
    (25.8, 51.0),  // Northwest
    (26.2, 51.2),  // North tip
    (26.0, 51.6),  // Northeast
    (25.5, 51.6),  // East coast
    (25.0, 51.4),  // Southeast
    (24.8, 51.2),  // South (close)
];

/// Oman — south coast of Gulf of Oman
static OMAN_SOUTH: [(f64, f64); 8] = [
    (23.5, 58.5),  // Sur area
    (23.0, 58.0),  // Ras al-Hadd
    (22.5, 59.0),  // South
    (21.5, 59.5),  // Masirah approach
    (20.5, 58.5),  // South Oman
    (20.0, 57.5),  // Salalah area
    (20.5, 56.0),  // Interior
    (24.0, 56.5),  // Interior
];

/// Yemen coast (Bab el-Mandeb area)
static YEMEN_COAST: [(f64, f64); 8] = [
    (12.8, 43.2),  // Bab el-Mandeb north
    (12.5, 43.5),  // Aden approach
    (12.7, 44.5),  // Aden
    (13.5, 45.5),  // East of Aden
    (14.0, 47.0),  // Yemen coast east
    (16.0, 47.0),  // Interior
    (16.0, 42.5),  // Interior
    (13.5, 43.0),  // Close polygon
];

/// Djibouti/Eritrea coast (west side of Bab el-Mandeb)
static DJIBOUTI_COAST: [(f64, f64); 6] = [
    (11.5, 43.0),  // South
    (11.5, 42.5),  // Djibouti coast
    (12.0, 42.5),  // North Djibouti
    (12.5, 43.0),  // Strait west
    (13.0, 42.0),  // Interior
    (11.0, 42.0),  // Interior
];
