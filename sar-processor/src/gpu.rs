use crate::types::{BoundingBox, CfarParams, SarDetection};
use crate::wasi::webgpu::webgpu;

/// Run the CA-CFAR ship detection algorithm on the GPU via wasi:webgpu.
/// Returns a detection mask (1 = ship, 0 = clutter) for each pixel.
pub fn run_cfar_gpu(
    sar_image: &[f32],
    width: u32,
    height: u32,
    params: &CfarParams,
) -> Result<Vec<u32>, String> {
    let device = webgpu::get_gpu()
        .request_adapter(None)
        .ok_or("no GPU adapter available")?
        .request_device(None)
        .map_err(|e| format!("failed to get GPU device: {}", e.message))?;

    // Load the CFAR compute shader
    let shader_module = device.create_shader_module(&webgpu::GpuShaderModuleDescriptor {
        code: include_str!("cfar.wgsl").to_string(),
        compilation_hints: None,
        label: Some("cfar_shader".to_string()),
    });

    let image_size_bytes = (sar_image.len() * 4) as u64;
    let detection_size_bytes = (width * height * 4) as u64;
    let params_size_bytes = 32u64; // CfarParams struct: 5 u32/f32 + 3 padding = 8 * 4 = 32

    // Input SAR image buffer (storage, read-only in shader)
    let image_buffer = device.create_buffer(&webgpu::GpuBufferDescriptor {
        label: Some("sar_image_buffer".to_string()),
        size: image_size_bytes,
        usage: webgpu::GpuBufferUsage::storage()
            | webgpu::GpuBufferUsage::copy_dst(),
        mapped_at_creation: Some(true),
    });
    let image_bytes: &[u8] = bytemuck::cast_slice(sar_image);
    image_buffer
        .get_mapped_range_set_with_copy(image_bytes, None, None)
        .map_err(|e| format!("failed to write image buffer: {:?}", e.kind))?;
    image_buffer.unmap().map_err(|e| format!("unmap error: {:?}", e.kind))?;

    // Output detections buffer (storage, read-write in shader)
    let detection_buffer = device.create_buffer(&webgpu::GpuBufferDescriptor {
        label: Some("detection_buffer".to_string()),
        size: detection_size_bytes,
        usage: webgpu::GpuBufferUsage::storage()
            | webgpu::GpuBufferUsage::copy_src(),
        mapped_at_creation: None,
    });

    // Uniform buffer for CFAR parameters
    let params_data = CfarParamsGpu {
        width,
        height,
        guard_cells: params.guard_cells,
        training_cells: params.training_cells,
        threshold_factor: params.threshold_factor,
        _pad0: 0,
        _pad1: 0,
        _pad2: 0,
    };
    let params_buffer = device.create_buffer(&webgpu::GpuBufferDescriptor {
        label: Some("cfar_params_buffer".to_string()),
        size: params_size_bytes,
        usage: webgpu::GpuBufferUsage::uniform()
            | webgpu::GpuBufferUsage::copy_dst(),
        mapped_at_creation: Some(true),
    });
    let params_bytes: &[u8] = bytemuck::bytes_of(&params_data);
    params_buffer
        .get_mapped_range_set_with_copy(params_bytes, None, None)
        .map_err(|e| format!("failed to write params buffer: {:?}", e.kind))?;
    params_buffer.unmap().map_err(|e| format!("unmap error: {:?}", e.kind))?;

    // Staging buffer for readback
    let staging_buffer = device.create_buffer(&webgpu::GpuBufferDescriptor {
        label: Some("staging_buffer".to_string()),
        size: detection_size_bytes,
        usage: webgpu::GpuBufferUsage::map_read()
            | webgpu::GpuBufferUsage::copy_dst(),
        mapped_at_creation: None,
    });

    // Create compute pipeline
    let pipeline = device.create_compute_pipeline(webgpu::GpuComputePipelineDescriptor {
        label: Some("cfar_pipeline".to_string()),
        layout: webgpu::GpuLayoutMode::Auto,
        compute: webgpu::GpuProgrammableStage {
            module: &shader_module,
            entry_point: Some("main".to_string()),
            constants: None,
        },
    });

    // Create bind group
    let bind_group_layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&webgpu::GpuBindGroupDescriptor {
        label: Some("cfar_bind_group".to_string()),
        layout: &bind_group_layout,
        entries: vec![
            webgpu::GpuBindGroupEntry {
                binding: 0,
                resource: webgpu::GpuBindingResource::GpuBufferBinding(
                    webgpu::GpuBufferBinding {
                        buffer: &image_buffer,
                        offset: Some(0),
                        size: None,
                    },
                ),
            },
            webgpu::GpuBindGroupEntry {
                binding: 1,
                resource: webgpu::GpuBindingResource::GpuBufferBinding(
                    webgpu::GpuBufferBinding {
                        buffer: &detection_buffer,
                        offset: Some(0),
                        size: None,
                    },
                ),
            },
            webgpu::GpuBindGroupEntry {
                binding: 2,
                resource: webgpu::GpuBindingResource::GpuBufferBinding(
                    webgpu::GpuBufferBinding {
                        buffer: &params_buffer,
                        offset: Some(0),
                        size: None,
                    },
                ),
            },
        ],
    });

    // Dispatch compute shader
    let encoder = device.create_command_encoder(Some(&webgpu::GpuCommandEncoderDescriptor {
        label: Some("cfar_encoder".to_string()),
    }));
    {
        let cpass = encoder.begin_compute_pass(None);
        cpass.set_pipeline(&pipeline);
        cpass
            .set_bind_group(0, Some(&bind_group), None, None, None)
            .map_err(|e| format!("set_bind_group error: {:?}", e.kind))?;
        cpass.insert_debug_marker("CA-CFAR ship detection");

        // Dispatch workgroups: ceil(width/16) x ceil(height/16)
        let wg_x = (width + 15) / 16;
        let wg_y = (height + 15) / 16;
        cpass.dispatch_workgroups(wg_x, Some(wg_y), Some(1));
        cpass.end();
    }

    // Copy detection results to staging buffer
    encoder.copy_buffer_to_buffer(&detection_buffer, 0, &staging_buffer, 0, detection_size_bytes);

    // Submit and wait
    device.queue().submit(&[&encoder.finish(None)]);

    // Map staging buffer and read results
    staging_buffer
        .map_async(webgpu::GpuMapMode::read(), Some(0), None)
        .map_err(|e| format!("map_async error: {:?}", e.kind))?;

    let data = staging_buffer
        .get_mapped_range_get_with_copy(None, None)
        .map_err(|e| format!("get_mapped_range error: {:?}", e.kind))?;

    let result: Vec<u32> = bytemuck::cast_slice(&data).to_vec();

    staging_buffer.unmap().map_err(|e| format!("unmap error: {:?}", e.kind))?;

    Ok(result)
}

/// Extract ship detections from the binary mask using connected component labeling.
/// Converts pixel coordinates to lat/lon using the bounding box.
pub fn extract_detections(
    mask: &[u32],
    width: u32,
    height: u32,
    bbox: &BoundingBox,
    sar_image: &[f32],
) -> Vec<SarDetection> {
    let mut visited = vec![false; mask.len()];
    let mut detections = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if mask[idx] == 0 || visited[idx] {
                continue;
            }

            // Flood-fill to find connected component
            let mut stack = vec![(x, y)];
            let mut pixels: Vec<(u32, u32)> = Vec::new();
            let mut max_intensity: f32 = 0.0;

            while let Some((px, py)) = stack.pop() {
                let pidx = (py * width + px) as usize;
                if visited[pidx] || mask[pidx] == 0 {
                    continue;
                }
                visited[pidx] = true;
                pixels.push((px, py));
                let intensity = sar_image[pidx];
                if intensity > max_intensity {
                    max_intensity = intensity;
                }

                // 4-connected neighbors
                if px > 0 { stack.push((px - 1, py)); }
                if px + 1 < width { stack.push((px + 1, py)); }
                if py > 0 { stack.push((px, py - 1)); }
                if py + 1 < height { stack.push((px, py + 1)); }
            }

            if pixels.is_empty() {
                continue;
            }

            // Compute centroid
            let cx: f64 = pixels.iter().map(|(px, _)| *px as f64).sum::<f64>() / pixels.len() as f64;
            let cy: f64 = pixels.iter().map(|(_, py)| *py as f64).sum::<f64>() / pixels.len() as f64;

            // Convert pixel to lat/lon
            let lat = bbox.min_lat + (cy / height as f64) * (bbox.max_lat - bbox.min_lat);
            let lon = bbox.min_lon + (cx / width as f64) * (bbox.max_lon - bbox.min_lon);

            // Convert intensity to dB
            let intensity_db = 10.0 * log10_approx(max_intensity);

            // Estimate RCS from intensity and number of pixels
            let rcs = pixels.len() as f32 * max_intensity * 2.0;

            detections.push(SarDetection {
                lat,
                lon,
                intensity_db,
                rcs,
                pixel_x: cx as u32,
                pixel_y: cy as u32,
            });
        }
    }

    detections
}

fn log10_approx(x: f32) -> f32 {
    if x <= 0.0 {
        return -30.0;
    }
    let bits = x.to_bits() as f32;
    let log2 = bits * 1.1920928955078125e-7 - 126.94269504;
    log2 * 0.30103 // log10(2)
}

/// GPU-side CFAR parameters (matches WGSL struct layout with padding)
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CfarParamsGpu {
    width: u32,
    height: u32,
    guard_cells: u32,
    training_cells: u32,
    threshold_factor: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}
