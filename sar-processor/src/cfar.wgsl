// CA-CFAR (Cell-Averaging Constant False Alarm Rate) Ship Detection
// Processes a SAR intensity image to detect ship targets above sea clutter.

struct CfarParams {
    width: u32,
    height: u32,
    guard_cells: u32,
    training_cells: u32,
    threshold_factor: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<storage, read> sar_image: array<f32>;
@group(0) @binding(1) var<storage, read_write> detections: array<u32>;
@group(0) @binding(2) var<uniform> params: CfarParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let x = gid.x;
    let y = gid.y;

    // Bounds check
    if (x >= params.width || y >= params.height) {
        return;
    }

    let idx = y * params.width + x;
    let margin = params.guard_cells + params.training_cells;

    // Border pixels cannot be evaluated — not enough training cells
    if (x < margin || y < margin || x >= params.width - margin || y >= params.height - margin) {
        detections[idx] = 0u;
        return;
    }

    // Cell-Averaging CFAR
    // Sum intensities in the training ring (excluding guard cells and CUT)
    var sum: f32 = 0.0;
    var count: u32 = 0u;
    let outer = i32(margin);
    let inner = i32(params.guard_cells);

    for (var dy: i32 = -outer; dy <= outer; dy++) {
        for (var dx: i32 = -outer; dx <= outer; dx++) {
            let abs_dx = abs(dx);
            let abs_dy = abs(dy);

            // Skip guard cells and the Cell Under Test
            if (abs_dx <= inner && abs_dy <= inner) {
                continue;
            }

            let sample_idx = u32(i32(y) + dy) * params.width + u32(i32(x) + dx);
            sum += sar_image[sample_idx];
            count++;
        }
    }

    let mean_clutter = sum / f32(count);
    let cut_value = sar_image[idx];

    // Detection decision: is the CUT significantly above the local clutter?
    if (cut_value > mean_clutter * params.threshold_factor) {
        detections[idx] = 1u;
    } else {
        detections[idx] = 0u;
    }
}
