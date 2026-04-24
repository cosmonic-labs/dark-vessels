use crate::types::CfarParams;

/// CPU implementation of the CA-CFAR algorithm (fallback when GPU unavailable).
/// Same algorithm as cfar.wgsl but runs on the CPU.
pub fn run_cfar_cpu(
    sar_image: &[f32],
    width: u32,
    height: u32,
    params: &CfarParams,
) -> Vec<u32> {
    let size = (width * height) as usize;
    let mut detections = vec![0u32; size];
    let margin = (params.guard_cells + params.training_cells) as i32;
    let inner = params.guard_cells as i32;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let ix = x as i32;
            let iy = y as i32;

            // Border pixels
            if ix < margin || iy < margin
                || ix >= width as i32 - margin
                || iy >= height as i32 - margin
            {
                continue;
            }

            // Cell-Averaging CFAR
            let mut sum: f32 = 0.0;
            let mut count: u32 = 0;

            for dy in -margin..=margin {
                for dx in -margin..=margin {
                    if dx.abs() <= inner && dy.abs() <= inner {
                        continue;
                    }
                    let sample_idx = (iy + dy) as u32 * width + (ix + dx) as u32;
                    sum += sar_image[sample_idx as usize];
                    count += 1;
                }
            }

            let mean_clutter = sum / count as f32;
            let cut_value = sar_image[idx];

            if cut_value > mean_clutter * params.threshold_factor {
                detections[idx] = 1;
            }
        }
    }

    detections
}
