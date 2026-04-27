use std::mem::size_of;

use anyhow::Result;
use vulkanalia::prelude::v1_0::*;

use crate::types::*;

pub unsafe fn create_descriptor_pool(device: &Device, data: &mut AppData) -> Result<()> {
    let num_textures = data.level_textures.len().max(1);
    let total_sets = data.swapchain_images.len() * (1 + num_textures);

    let ubo_size = vk::DescriptorPoolSize::builder()
        .type_(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(total_sets as u32 * 2);

    let sampler_size = vk::DescriptorPoolSize::builder()
        .type_(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(total_sets as u32);

    let pool_sizes = &[ubo_size, sampler_size];
    let info = vk::DescriptorPoolCreateInfo::builder()
        .pool_sizes(pool_sizes)
        .max_sets(total_sets as u32);

    data.descriptor_pool = device.create_descriptor_pool(&info, None)?;

    Ok(())
}

pub unsafe fn create_descriptor_sets(device: &Device, data: &mut AppData) -> Result<()> {
    let swapchain_count = data.swapchain_images.len();

    // Allocate main descriptor sets (for UBO + fallback texture)
    let layouts = vec![data.descriptor_set_layout; swapchain_count];
    let info = vk::DescriptorSetAllocateInfo::builder()
        .descriptor_pool(data.descriptor_pool)
        .set_layouts(&layouts);

    data.descriptor_sets = device.allocate_descriptor_sets(&info)?;

    // Update main descriptor sets with UBO and fallback texture
    for i in 0..swapchain_count {
        let info = vk::DescriptorBufferInfo::builder()
            .buffer(data.uniform_buffers[i])
            .offset(0)
            .range(size_of::<UniformBufferObject>() as u64);

        let buffer_info = &[info];
        let ubo_write = vk::WriteDescriptorSet::builder()
            .dst_set(data.descriptor_sets[i])
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(buffer_info);

        let info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(data.texture_image_view)
            .sampler(data.texture_sampler);

        let image_info = &[info];
        let sampler_write = vk::WriteDescriptorSet::builder()
            .dst_set(data.descriptor_sets[i])
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(image_info);

        let light_buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(data.light_uniform_buffers[i])
            .offset(0)
            .range(size_of::<LightingUBO>() as u64);

        let light_buffer_info = &[light_buf_info];
        let light_ubo_write = vk::WriteDescriptorSet::builder()
            .dst_set(data.descriptor_sets[i])
            .dst_binding(2)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(light_buffer_info);

        device.update_descriptor_sets(
            &[ubo_write, sampler_write, light_ubo_write],
            &[] as &[vk::CopyDescriptorSet],
        );
    }

    // Allocate and update descriptor sets for each level texture
    for tex_idx in 0..data.level_textures.len() {
        let layouts = vec![data.descriptor_set_layout; swapchain_count];
        let info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(data.descriptor_pool)
            .set_layouts(&layouts);

        let tex_descriptor_sets = device.allocate_descriptor_sets(&info)?;

        for i in 0..swapchain_count {
            let buffer_info_data = vk::DescriptorBufferInfo::builder()
                .buffer(data.uniform_buffers[i])
                .offset(0)
                .range(size_of::<UniformBufferObject>() as u64);

            let buffer_info = &[buffer_info_data];
            let ubo_write = vk::WriteDescriptorSet::builder()
                .dst_set(tex_descriptor_sets[i])
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(buffer_info);

            let image_info_data = vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(data.level_textures[tex_idx].view)
                .sampler(data.texture_sampler);

            let image_info = &[image_info_data];
            let sampler_write = vk::WriteDescriptorSet::builder()
                .dst_set(tex_descriptor_sets[i])
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(image_info);

            let light_buf_info_data = vk::DescriptorBufferInfo::builder()
                .buffer(data.light_uniform_buffers[i])
                .offset(0)
                .range(size_of::<LightingUBO>() as u64);

            let light_buffer_info = &[light_buf_info_data];
            let light_ubo_write = vk::WriteDescriptorSet::builder()
                .dst_set(tex_descriptor_sets[i])
                .dst_binding(2)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(light_buffer_info);

            device.update_descriptor_sets(
                &[ubo_write, sampler_write, light_ubo_write],
                &[] as &[vk::CopyDescriptorSet],
            );
        }

        data.level_textures[tex_idx].descriptor_sets = tex_descriptor_sets;
    }

    Ok(())
}
