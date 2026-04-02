use std::ptr::copy_nonoverlapping as memcpy;

use anyhow::{Result, anyhow};
use vulkanalia::prelude::v1_0::*;

use crate::dtx;
use crate::types::*;
use crate::vulkan::helpers::*;

/// Structure to hold texture data before uploading to GPU
pub struct LoadedTexture {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub name: String,
}

/// Load a DTX texture file
pub fn load_dtx_texture(path: &std::path::Path) -> Result<LoadedTexture> {
    let dtx = dtx::DtxFile::read_from_file(path)
        .map_err(|e| anyhow!("Failed to load DTX file {:?}: {}", path, e))?;

    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut pixels = dtx.pixels;

    // Apply chromakey (black → transparent) for textures that need it.
    if needs_chromakey(path) {
        apply_chromakey(&mut pixels);
    }

    Ok(LoadedTexture {
        pixels,
        width: dtx.width as u32,
        height: dtx.height as u32,
        name,
    })
}

/// Returns true for textures that use palette-index-0 / near-black as a
/// transparency key (cobwebs, fences, grates, chains, etc.).
fn needs_chromakey(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase().replace('\\', "/");
    // Match by filename or path fragment
    let patterns: &[&str] = &[
        "cobweb", "spiderweb", "web",
        "fence", "bars",
        "chain", "rope",
        "vines", "ivy",
        "railing",
        "lattice",
        "net",
        "chlinkdeluxe",
        "ladder_met",
    ];
    let fname = s.rsplit('/').next().unwrap_or(&s);
    patterns.iter().any(|p| fname.contains(p))
}

/// Turn near-black pixels (R+G+B ≤ 15) fully transparent.
fn apply_chromakey(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        let r = chunk[0] as u16;
        let g = chunk[1] as u16;
        let b = chunk[2] as u16;
        if r + g + b <= 15 {
            chunk[3] = 0;
        }
    }
}

/// Create a solid color texture as fallback
pub fn create_colored_texture(width: u32, height: u32, r: u8, g: u8, b: u8) -> LoadedTexture {
    let pixel_count = (width * height) as usize;
    let mut pixels = Vec::with_capacity(pixel_count * 4);
    for _ in 0..pixel_count {
        pixels.push(r);
        pixels.push(g);
        pixels.push(b);
        pixels.push(255);
    }
    LoadedTexture {
        pixels,
        width,
        height,
        name: "fallback".to_string(),
    }
}

/// Search for a DTX file by name in the textures folder
pub fn find_texture_file(
    textures_root: &std::path::Path,
    texture_name: &str,
) -> Option<std::path::PathBuf> {
    let clean_name = texture_name.replace(['\\', '/'], "").to_uppercase();

    let dtx_name = if clean_name.ends_with(".DTX") {
        clean_name
    } else {
        format!("{}.DTX", clean_name)
    };

    fn search_recursive(dir: &std::path::Path, target: &str) -> Option<std::path::PathBuf> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(found) = search_recursive(&path, target) {
                        return Some(found);
                    }
                } else if let Some(name) = path.file_name() {
                    if name.to_string_lossy().to_uppercase() == target {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    search_recursive(textures_root, &dtx_name)
}

/// Get texture dimensions without loading full pixel data
pub fn get_texture_dimensions(texture_name: &str) -> (u32, u32) {
    let textures_path = std::path::Path::new("REZ/TEXTURES");

    let variations = [
        texture_name.to_string(),
        texture_name.replace("TEXTURES\\", ""),
        texture_name.replace("textures\\", ""),
        texture_name
            .split('\\')
            .last()
            .unwrap_or(texture_name)
            .to_string(),
        texture_name
            .split('/')
            .last()
            .unwrap_or(texture_name)
            .to_string(),
    ];

    for var in &variations {
        if let Some(dtx_path) = find_texture_file(textures_path, var) {
            if let Ok(dtx) = dtx::DtxFile::read_from_file(&dtx_path) {
                return (dtx.width as u32, dtx.height as u32);
            }
        }
    }

    (256, 256)
}

pub unsafe fn create_texture_image(
    instance: &Instance,
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    let textures_path = std::path::Path::new("REZ/TEXTURES");

    println!("=== LOADING LEVEL TEXTURES ===");
    println!("Textures to load: {}", data.level_textures.len());

    let fallback = create_colored_texture(64, 64, 128, 128, 128);

    let mut loaded_count = 0;
    let mut failed_count = 0;

    for i in 0..data.level_textures.len() {
        let texture_name = data.level_textures[i].name.clone();

        let loaded = if let Some(dtx_path) = find_texture_file(textures_path, &texture_name) {
            match load_dtx_texture(&dtx_path) {
                Ok(tex) => {
                    loaded_count += 1;
                    tex
                }
                Err(e) => {
                    println!("  Failed to load {}: {}", texture_name, e);
                    failed_count += 1;
                    create_colored_texture(64, 64, 255, 0, 255)
                }
            }
        } else {
            let variations = [
                texture_name.clone(),
                texture_name.replace("TEXTURES\\", ""),
                texture_name.replace("textures\\", ""),
                texture_name
                    .split('\\')
                    .last()
                    .unwrap_or(&texture_name)
                    .to_string(),
                texture_name
                    .split('/')
                    .last()
                    .unwrap_or(&texture_name)
                    .to_string(),
            ];

            let mut found_tex = None;
            for var in &variations {
                if let Some(dtx_path) = find_texture_file(textures_path, var) {
                    if let Ok(tex) = load_dtx_texture(&dtx_path) {
                        found_tex = Some(tex);
                        break;
                    }
                }
            }

            if found_tex.is_none() {
                let direct = std::path::Path::new(&texture_name);
                if direct.exists() {
                    if let Ok(tex) = load_dtx_texture(direct) {
                        found_tex = Some(tex);
                    }
                }
                if found_tex.is_none() {
                    let skins_path = std::path::Path::new("REZ/SKINS");
                    let skin_name = texture_name
                        .split(['\\', '/'])
                        .last()
                        .unwrap_or(&texture_name);
                    if let Some(dtx_path) = find_texture_file(skins_path, skin_name) {
                        if let Ok(tex) = load_dtx_texture(&dtx_path) {
                            found_tex = Some(tex);
                        }
                    }
                }
            }

            if let Some(tex) = found_tex {
                loaded_count += 1;
                tex
            } else {
                failed_count += 1;
                let hash = texture_name
                    .bytes()
                    .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
                let r = ((hash * 17) % 200 + 55) as u8;
                let g = ((hash * 31) % 200 + 55) as u8;
                let b = ((hash * 47) % 200 + 55) as u8;
                create_colored_texture(64, 64, r, g, b)
            }
        };

        let (image, memory) = upload_texture_to_gpu(instance, device, data, &loaded)?;

        let view = create_image_view(
            device,
            image,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageAspectFlags::COLOR,
            1,
        )?;

        data.level_textures[i].image = image;
        data.level_textures[i].memory = memory;
        data.level_textures[i].view = view;
    }

    println!(
        "Loaded: {} textures, Failed: {} textures",
        loaded_count, failed_count
    );

    let (image, memory) = upload_texture_to_gpu(instance, device, data, &fallback)?;
    data.texture_image = image;
    data.texture_image_memory = memory;
    data.mip_levels = 1;

    Ok(())
}

/// Upload a texture to GPU memory
pub unsafe fn upload_texture_to_gpu(
    instance: &Instance,
    device: &Device,
    data: &mut AppData,
    texture: &LoadedTexture,
) -> Result<(vk::Image, vk::DeviceMemory)> {
    let width = texture.width;
    let height = texture.height;
    let size = (width * height * 4) as u64;

    let (staging_buffer, staging_buffer_memory) = create_buffer(
        instance,
        device,
        data,
        size,
        vk::BufferUsageFlags::TRANSFER_SRC,
        vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE,
    )?;

    let memory = device.map_memory(staging_buffer_memory, 0, size, vk::MemoryMapFlags::empty())?;
    memcpy(texture.pixels.as_ptr(), memory.cast(), texture.pixels.len());
    device.unmap_memory(staging_buffer_memory);

    let (image, image_memory) = create_image(
        instance,
        device,
        data,
        width,
        height,
        1,
        vk::SampleCountFlags::_1,
        vk::Format::R8G8B8A8_SRGB,
        vk::ImageTiling::OPTIMAL,
        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;

    transition_image_layout(
        device,
        data,
        image,
        vk::Format::R8G8B8A8_SRGB,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        1,
    )?;

    copy_buffer_to_image(device, data, staging_buffer, image, width, height)?;

    transition_image_layout(
        device,
        data,
        image,
        vk::Format::R8G8B8A8_SRGB,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        1,
    )?;

    device.destroy_buffer(staging_buffer, None);
    device.free_memory(staging_buffer_memory, None);

    Ok((image, image_memory))
}

pub unsafe fn generate_mipmaps(
    instance: &Instance,
    device: &Device,
    data: &AppData,
    image: vk::Image,
    format: vk::Format,
    width: u32,
    height: u32,
    mip_levels: u32,
) -> Result<()> {
    if !instance
        .get_physical_device_format_properties(data.physical_device, format)
        .optimal_tiling_features
        .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR)
    {
        return Err(anyhow!(
            "Texture image format does not support linear blitting!"
        ));
    }

    let command_buffer = begin_single_time_commands(device, data)?;

    let subresource = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);

    let mut barrier = vk::ImageMemoryBarrier::builder()
        .image(image)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .subresource_range(subresource);

    let mut mip_width = width;
    let mut mip_height = height;

    for i in 1..mip_levels {
        barrier.subresource_range.base_mip_level = i - 1;
        barrier.old_layout = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
        barrier.new_layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_READ;

        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[] as &[vk::MemoryBarrier],
            &[] as &[vk::BufferMemoryBarrier],
            &[barrier],
        );

        let src_subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(i - 1)
            .base_array_layer(0)
            .layer_count(1);

        let dst_subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(i)
            .base_array_layer(0)
            .layer_count(1);

        let blit = vk::ImageBlit::builder()
            .src_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: mip_width as i32,
                    y: mip_height as i32,
                    z: 1,
                },
            ])
            .src_subresource(src_subresource)
            .dst_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: (if mip_width > 1 { mip_width / 2 } else { 1 }) as i32,
                    y: (if mip_height > 1 { mip_height / 2 } else { 1 }) as i32,
                    z: 1,
                },
            ])
            .dst_subresource(dst_subresource);

        device.cmd_blit_image(
            command_buffer,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[blit],
            vk::Filter::LINEAR,
        );

        barrier.old_layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
        barrier.new_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_READ;
        barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;

        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[] as &[vk::MemoryBarrier],
            &[] as &[vk::BufferMemoryBarrier],
            &[barrier],
        );

        if mip_width > 1 {
            mip_width /= 2;
        }

        if mip_height > 1 {
            mip_height /= 2;
        }
    }

    barrier.subresource_range.base_mip_level = mip_levels - 1;
    barrier.old_layout = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
    barrier.new_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
    barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
    barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;

    device.cmd_pipeline_barrier(
        command_buffer,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::DependencyFlags::empty(),
        &[] as &[vk::MemoryBarrier],
        &[] as &[vk::BufferMemoryBarrier],
        &[barrier],
    );

    end_single_time_commands(device, data, command_buffer)?;

    Ok(())
}

pub unsafe fn create_texture_image_view(
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    data.texture_image_view = create_image_view(
        device,
        data.texture_image,
        vk::Format::R8G8B8A8_SRGB,
        vk::ImageAspectFlags::COLOR,
        1,
    )?;

    Ok(())
}

pub unsafe fn create_texture_sampler(device: &Device, data: &mut AppData) -> Result<()> {
    let info = vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::REPEAT)
        .address_mode_v(vk::SamplerAddressMode::REPEAT)
        .address_mode_w(vk::SamplerAddressMode::REPEAT)
        .anisotropy_enable(true)
        .max_anisotropy(16.0)
        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
        .unnormalized_coordinates(false)
        .compare_enable(false)
        .compare_op(vk::CompareOp::ALWAYS)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .min_lod(0.0)
        .max_lod(data.mip_levels as f32)
        .mip_lod_bias(0.0);

    data.texture_sampler = device.create_sampler(&info, None)?;

    Ok(())
}
