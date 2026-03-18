use vk_bindings::*;
use vk_tutorial_samples::*;

use std::io::Read;
use rustKPC::dat::DatFile;
use rustKPC::lights::{Light, extract_lights_from_objects};

fn make_version(
    variant: u32,
    major: u32,
    minor: u32,
    patch: u32
) -> u32
{
    ((variant) << 29) |
    ((major) << 22) |
    ((minor) << 12) |
    (patch)
}

//
// Swapchain creation
//

struct SwapchainResult
{
    swapchain: VkSwapchainKHR,
    width: u32,
    height: u32
}

unsafe fn create_swapchain(
    chosen_phys_device: VkPhysicalDevice,
    surface: VkSurfaceKHR,
    device: VkDevice,
    old_swapchain: VkSwapchainKHR,
    width: u32,
    height: u32,
    format: VkFormat,
    chosen_graphics_queue_family: u32,
    chosen_present_queue_family: u32
) -> SwapchainResult
{
    let mut surface_capabilities = VkSurfaceCapabilitiesKHR::default();
    unsafe
    {
        vkGetPhysicalDeviceSurfaceCapabilitiesKHR(
            chosen_phys_device,
            surface,
            &mut surface_capabilities
        )
    };

    // Query surface formats
    let mut surface_format_count: u32 = 0;
    unsafe
    {
        vkGetPhysicalDeviceSurfaceFormatsKHR(
            chosen_phys_device,
            surface,
            &mut surface_format_count,
            core::ptr::null_mut()
        )
    };

    let mut surface_formats = vec![VkSurfaceFormatKHR::default(); surface_format_count as usize];
    unsafe
    {
        vkGetPhysicalDeviceSurfaceFormatsKHR(
            chosen_phys_device,
            surface,
            &mut surface_format_count,
            surface_formats.as_mut_ptr()
        )
    };

    let mut chosen_surface_format = None;
    for surface_format in surface_formats.iter()
    {
        let mut format_properties = VkFormatProperties::default();
        unsafe
        {
            vkGetPhysicalDeviceFormatProperties(
                chosen_phys_device,
                surface_format.format,
                &mut format_properties
            );
        }

        if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT as VkFormatFeatureFlags == 0
        {
            continue;
        }

        if surface_format.format == format &&
           surface_format.colorSpace == VK_COLOR_SPACE_SRGB_NONLINEAR_KHR
        {
            chosen_surface_format = Some(*surface_format);
        }
    }

    let chosen_surface_format = chosen_surface_format.expect("Could not find suitable surface format.");

    // Query present modes
    let mut present_mode_count: u32 = 0;
    unsafe
    {
        vkGetPhysicalDeviceSurfacePresentModesKHR(
            chosen_phys_device,
            surface,
            &mut present_mode_count,
            core::ptr::null_mut()
        )
    };

    let mut present_modes = vec![VkPresentModeKHR::default(); present_mode_count as usize];
    unsafe
    {
        vkGetPhysicalDeviceSurfacePresentModesKHR(
            chosen_phys_device,
            surface,
            &mut present_mode_count,
            present_modes.as_mut_ptr()
        )
    };

    let preferred_present_mode = VK_PRESENT_MODE_FIFO_KHR;

    let mut chosen_present_mode = VK_PRESENT_MODE_FIFO_KHR;
    for present_mode in present_modes.iter()
    {
        if *present_mode == preferred_present_mode
        {
            chosen_present_mode = *present_mode;
        }
    }

    // Setting concurrent or exclusive sharing mode based on whether we are multiqueue.
    let empty_array = [];
    let queue_family_array = [
        chosen_graphics_queue_family,
        chosen_present_queue_family
    ];

    let image_sharing_mode;
    let queue_families;
    if chosen_graphics_queue_family == chosen_present_queue_family
    {
        image_sharing_mode = VK_SHARING_MODE_EXCLUSIVE;
        queue_families = &empty_array[..];
    }
    else
    {
        image_sharing_mode = VK_SHARING_MODE_CONCURRENT;
        queue_families = &queue_family_array[..];
    }

    let swapchain_image_count = surface_capabilities.minImageCount;

    let min_width = surface_capabilities.minImageExtent.width;
    let max_width = surface_capabilities.maxImageExtent.width;

    let min_height = surface_capabilities.minImageExtent.height;
    let max_height = surface_capabilities.maxImageExtent.height;

    let swapchain_create_info = VkSwapchainCreateInfoKHR {
        sType: VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR,
        flags: 0x0,
        pNext: core::ptr::null(),
        surface: surface,
        minImageCount: swapchain_image_count,
        imageFormat: chosen_surface_format.format,
        imageColorSpace: chosen_surface_format.colorSpace,
        imageExtent: VkExtent2D {
            width: min_width.max(max_width.min(width)),
            height: min_height.max(max_height.min(height))
        },
        imageArrayLayers: 1,
        imageUsage: VK_IMAGE_USAGE_STORAGE_BIT as VkImageUsageFlags,
        imageSharingMode: image_sharing_mode,
        queueFamilyIndexCount: queue_families.len() as u32,
        pQueueFamilyIndices: queue_families.as_ptr(),
        preTransform: surface_capabilities.currentTransform,
        compositeAlpha: VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR,
        presentMode: chosen_present_mode,
        clipped: VK_TRUE,
        oldSwapchain: old_swapchain
    };

    println!("Creating swapchain.");
    let mut swapchain = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateSwapchainKHR(
            device,
            &swapchain_create_info,
            core::ptr::null_mut(),
            &mut swapchain
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create swapchain. Error: {:?}.", result);
    }

    if old_swapchain != core::ptr::null_mut()
    {
        println!("Deleting old swapchain.");
        unsafe
        {
            vkDestroySwapchainKHR(
                device,
                old_swapchain,
                core::ptr::null_mut()
            );
        }
    }

    SwapchainResult {
        swapchain,
        width: width,
        height: height
    }
}

//
// Getting swapchain images and framebuffer creation with render targets
//

unsafe fn create_framebuffers_and_render_targets(
    device: VkDevice,
    chosen_phys_device: VkPhysicalDevice,
    phys_device_mem_properties: &VkPhysicalDeviceMemoryProperties,
    width: u32,
    height: u32,
    format: VkFormat,
    render_pass: VkRenderPass,
    swapchain: VkSwapchainKHR,
    swapchain_imgs: &mut Vec<VkImage>,
    swapchain_img_views: &mut Vec<VkImageView>,
    color_buffers: &mut Vec<VkImage>,
    color_buffer_memories: &mut Vec<VkDeviceMemory>,
    color_buffer_views: &mut Vec<VkImageView>,
    depth_buffers: &mut Vec<VkImage>,
    depth_buffer_memories: &mut Vec<VkDeviceMemory>,
    depth_buffer_views: &mut Vec<VkImageView>,
    framebuffers: &mut Vec<VkFramebuffer>
)
{
    let mut swapchain_img_count: u32 = 0;
    let result = unsafe
    {
        vkGetSwapchainImagesKHR(
            device,
            swapchain,
            &mut swapchain_img_count,
            core::ptr::null_mut()
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to get swapchain images. Error: {:?}.", result);
    }

    if swapchain_imgs.len() < swapchain_img_count as usize
    {
        swapchain_imgs.resize(swapchain_img_count as usize, core::ptr::null_mut());
    }

    let result = unsafe
    {
        vkGetSwapchainImagesKHR(
            device,
            swapchain,
            &mut swapchain_img_count,
            swapchain_imgs.as_mut_ptr()
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to get swapchain images. Error: {:?}.", result);
    }

    swapchain_img_views.reserve(swapchain_imgs.len());
    for (i, swapchain_img) in swapchain_imgs.iter().enumerate()
    {
        let img_view_create_info = VkImageViewCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            image: *swapchain_img,
            viewType: VK_IMAGE_VIEW_TYPE_2D,
            format: format,
            components: VkComponentMapping {
                r: VK_COMPONENT_SWIZZLE_IDENTITY,
                g: VK_COMPONENT_SWIZZLE_IDENTITY,
                b: VK_COMPONENT_SWIZZLE_IDENTITY,
                a: VK_COMPONENT_SWIZZLE_IDENTITY
            },
            subresourceRange: VkImageSubresourceRange {
                aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                baseMipLevel: 0,
                levelCount: 1,
                baseArrayLayer: 0,
                layerCount: 1
            }
        };

        println!("Creating framebuffer image view.");
        let mut image_view = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImageView(
                device,
                &img_view_create_info,
                core::ptr::null_mut(),
                &mut image_view
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create framebuffer image view {:?}. Error: {:?}.", i, result);
        }

        swapchain_img_views.push(image_view);
    }

    let mut format_properties = VkFormatProperties::default();
    unsafe
    {
        vkGetPhysicalDeviceFormatProperties(
            chosen_phys_device,
            VK_FORMAT_R32G32B32A32_SFLOAT,
            &mut format_properties
        );
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_COLOR_ATTACHMENT_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R32G32B32A32_SFLOAT with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_COLOR_ATTACHMENT_BIT.");
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R32G32B32A32_SFLOAT with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT.");
    }

    color_buffers.reserve(swapchain_imgs.len());
    color_buffer_memories.reserve(swapchain_imgs.len());
    color_buffer_views.reserve(swapchain_imgs.len());
    for i in 0..swapchain_imgs.len()
    {
        let image_create_info = VkImageCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            imageType: VK_IMAGE_TYPE_2D,
            format: VK_FORMAT_R32G32B32A32_SFLOAT,
            extent: VkExtent3D {
                width: width as u32,
                height: height as u32,
                depth: 1
            },
            mipLevels: 1,
            arrayLayers: 1,
            samples: VK_SAMPLE_COUNT_1_BIT,
            tiling: VK_IMAGE_TILING_OPTIMAL,
            usage: (VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT |
                    VK_IMAGE_USAGE_STORAGE_BIT) as VkImageUsageFlags,
            sharingMode: VK_SHARING_MODE_EXCLUSIVE,
            queueFamilyIndexCount: 0,
            pQueueFamilyIndices: core::ptr::null(),
            initialLayout: VK_IMAGE_LAYOUT_UNDEFINED
        };

        println!("Creating color image.");
        let mut color_image = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImage(
                device,
                &image_create_info,
                core::ptr::null_mut(),
                &mut color_image
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create color image {}. Error: {}", i, result);
        }

        color_buffers.push(color_image);

        let mut mem_requirements = VkMemoryRequirements::default();
        unsafe
        {
            vkGetImageMemoryRequirements(
                device,
                color_image,
                &mut mem_requirements
            );
        }

        let color_buffer_mem_props = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT as VkMemoryPropertyFlags;
        let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
        for i in 0..phys_device_mem_properties.memoryTypeCount
        {
            if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
                (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & color_buffer_mem_props) ==
                color_buffer_mem_props
            {
                chosen_memory_type = i;
                break;
            }
        }

        if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
        {
            panic!("Could not find memory type.");
        }

        let image_alloc_info = VkMemoryAllocateInfo {
            sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            allocationSize: mem_requirements.size,
            memoryTypeIndex: chosen_memory_type
        };

        println!("Color image size: {}", mem_requirements.size);
        println!("Color image align: {}", mem_requirements.alignment);

        println!("Allocating color image memory");
        let mut color_image_memory = core::ptr::null_mut();
        let result = unsafe
        {
            vkAllocateMemory(
                device,
                &image_alloc_info,
                core::ptr::null(),
                &mut color_image_memory
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Could not allocate memory for color image {}. Error: {}", i, result);
        }

        let result = unsafe
        {
            vkBindImageMemory(
                device,
                color_image,
                color_image_memory,
                0
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to bind memory to color image {}. Error: {}", i, result);
        }

        color_buffer_memories.push(color_image_memory);

        let image_view_create_info = VkImageViewCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            image: color_image,
            viewType: VK_IMAGE_VIEW_TYPE_2D,
            format: VK_FORMAT_R32G32B32A32_SFLOAT,
            components: VkComponentMapping {
                r: VK_COMPONENT_SWIZZLE_IDENTITY,
                g: VK_COMPONENT_SWIZZLE_IDENTITY,
                b: VK_COMPONENT_SWIZZLE_IDENTITY,
                a: VK_COMPONENT_SWIZZLE_IDENTITY
            },
            subresourceRange: VkImageSubresourceRange {
                aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                baseMipLevel: 0,
                levelCount: 1,
                baseArrayLayer: 0,
                layerCount: 1
            }
        };

        println!("Creating color image view.");
        let mut color_image_view = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImageView(device,
                &image_view_create_info,
                core::ptr::null_mut(),
                &mut color_image_view
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create color image view {}. Error: {}", i, result);
        }

        color_buffer_views.push(color_image_view);
    }

    let mut format_properties = VkFormatProperties::default();
    unsafe
    {
        vkGetPhysicalDeviceFormatProperties(
            chosen_phys_device,
            VK_FORMAT_D32_SFLOAT,
            &mut format_properties
        );
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_DEPTH_STENCIL_ATTACHMENT_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_D32_SFLOAT with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_DEPTH_STENCIL_ATTACHMENT_BIT.");
    }

    depth_buffers.reserve(swapchain_imgs.len());
    depth_buffer_memories.reserve(swapchain_imgs.len());
    depth_buffer_views.reserve(swapchain_imgs.len());
    for i in 0..swapchain_imgs.len()
    {
        let image_create_info = VkImageCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            imageType: VK_IMAGE_TYPE_2D,
            format: VK_FORMAT_D32_SFLOAT,
            extent: VkExtent3D {
                width: width as u32,
                height: height as u32,
                depth: 1
            },
            mipLevels: 1,
            arrayLayers: 1,
            samples: VK_SAMPLE_COUNT_1_BIT,
            tiling: VK_IMAGE_TILING_OPTIMAL,
            usage: VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT as VkImageUsageFlags,
            sharingMode: VK_SHARING_MODE_EXCLUSIVE,
            queueFamilyIndexCount: 0,
            pQueueFamilyIndices: core::ptr::null(),
            initialLayout: VK_IMAGE_LAYOUT_UNDEFINED
        };

        println!("Creating depth image.");
        let mut depth_image = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImage(
                device,
                &image_create_info,
                core::ptr::null_mut(),
                &mut depth_image
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create depth image {}. Error: {}", i, result);
        }

        depth_buffers.push(depth_image);

        let mut mem_requirements = VkMemoryRequirements::default();
        unsafe
        {
            vkGetImageMemoryRequirements(
                device,
                depth_image,
                &mut mem_requirements
            );
        }

        let depth_buffer_mem_props = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT as VkMemoryPropertyFlags;
        let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
        for i in 0..phys_device_mem_properties.memoryTypeCount
        {
            if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
                (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & depth_buffer_mem_props) ==
                    depth_buffer_mem_props
            {
                chosen_memory_type = i;
                break;
            }
        }

        if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
        {
            panic!("Could not find memory type.");
        }

        let image_alloc_info = VkMemoryAllocateInfo {
            sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            allocationSize: mem_requirements.size,
            memoryTypeIndex: chosen_memory_type
        };

        println!("Depth image size: {}", mem_requirements.size);
        println!("Depth image align: {}", mem_requirements.alignment);

        println!("Allocating depth image memory");
        let mut depth_image_memory = core::ptr::null_mut();
        let result = unsafe
        {
            vkAllocateMemory(
                device,
                &image_alloc_info,
                core::ptr::null(),
                &mut depth_image_memory
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Could not allocate memory for depth image {}. Error: {}", i, result);
        }

        let result = unsafe
        {
            vkBindImageMemory(
                device,
                depth_image,
                depth_image_memory,
                0
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to bind memory to depth image {}. Error: {}", i, result);
        }

        depth_buffer_memories.push(depth_image_memory);

        let image_view_create_info = VkImageViewCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            image: depth_image,
            viewType: VK_IMAGE_VIEW_TYPE_2D,
            format: VK_FORMAT_D32_SFLOAT,
            components: VkComponentMapping {
                r: VK_COMPONENT_SWIZZLE_IDENTITY,
                g: VK_COMPONENT_SWIZZLE_IDENTITY,
                b: VK_COMPONENT_SWIZZLE_IDENTITY,
                a: VK_COMPONENT_SWIZZLE_IDENTITY
            },
            subresourceRange: VkImageSubresourceRange {
                aspectMask: VK_IMAGE_ASPECT_DEPTH_BIT as VkImageAspectFlags,
                baseMipLevel: 0,
                levelCount: 1,
                baseArrayLayer: 0,
                layerCount: 1
            }
        };

        println!("Creating depth image view.");
        let mut depth_image_view = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImageView(
                device,
                &image_view_create_info,
                core::ptr::null_mut(),
                &mut depth_image_view
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create depth image view {}. Error: {}", i, result);
        }

        depth_buffer_views.push(depth_image_view);
    }

    framebuffers.reserve(swapchain_imgs.len());
    for (i, (color_buffer_view, depth_buffer_view)) in color_buffer_views.iter().zip(depth_buffer_views.iter()).enumerate()
    {
        let attachments: [VkImageView; 2] = [
            *color_buffer_view,
            *depth_buffer_view
        ];

        let create_info = VkFramebufferCreateInfo {
            sType: VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            renderPass: render_pass,
            attachmentCount: attachments.len() as u32,
            pAttachments: attachments.as_ptr(),
            width: width,
            height: height,
            layers: 1
        };

        println!("Creating framebuffer.");
        let mut new_framebuffer = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateFramebuffer(
                device,
                &create_info,
                core::ptr::null_mut(),
                &mut new_framebuffer
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create framebuffer {:?}. Error: {:?}.", i, result);
        }

        framebuffers.push(new_framebuffer);
    }
}

unsafe fn destroy_framebuffers_and_render_targets(
    device: VkDevice,
    swapchain_img_views: &mut Vec<VkImageView>,
    color_buffers: &mut Vec<VkImage>,
    color_buffer_memories: &mut Vec<VkDeviceMemory>,
    color_buffer_views: &mut Vec<VkImageView>,
    depth_buffers: &mut Vec<VkImage>,
    depth_buffer_memories: &mut Vec<VkDeviceMemory>,
    depth_buffer_views: &mut Vec<VkImageView>,
    framebuffers: &mut Vec<VkFramebuffer>
)
{
    for swapchain_framebuffer in framebuffers.iter()
    {
        println!("Deleting framebuffer.");
        unsafe
        {
            vkDestroyFramebuffer(
                device,
                *swapchain_framebuffer,
                core::ptr::null_mut()
            );
        }
    }

    framebuffers.clear();

    for depth_buffer_view in depth_buffer_views.iter()
    {
        println!("Deleting depth image views.");
        unsafe
        {
            vkDestroyImageView(
                device,
                *depth_buffer_view,
                core::ptr::null_mut()
            );
        }
    }

    depth_buffer_views.clear();

    for depth_buffer in depth_buffers.iter()
    {
        println!("Deleting depth image");
        unsafe
        {
            vkDestroyImage(
                device,
                *depth_buffer,
                core::ptr::null_mut()
            );
        }
    }

    depth_buffers.clear();

    for depth_buffer_memory in depth_buffer_memories.iter()
    {
        println!("Deleting depth image device memory");
        unsafe
        {
            vkFreeMemory(
                device,
                *depth_buffer_memory,
                core::ptr::null_mut()
            );
        }
    }

    depth_buffer_memories.clear();

    for color_buffer_view in color_buffer_views.iter()
    {
        println!("Deleting color image views.");
        unsafe
        {
            vkDestroyImageView(
                device,
                *color_buffer_view,
                core::ptr::null_mut()
            );
        }
    }

    color_buffer_views.clear();

    for color_buffer in color_buffers.iter()
    {
        println!("Deleting color image");
        unsafe
        {
            vkDestroyImage(
                device,
                *color_buffer,
                core::ptr::null_mut()
            );
        }
    }

    color_buffers.clear();

    for color_buffer_memory in color_buffer_memories.iter()
    {
        println!("Deleting color image device memory");
        unsafe
        {
            vkFreeMemory(
                device,
                *color_buffer_memory,
                core::ptr::null_mut()
            );
        }
    }

    color_buffer_memories.clear();

    for swapchain_img_view in swapchain_img_views.iter()
    {
        println!("Deleting swapchain image views.");
        unsafe
        {
            vkDestroyImageView(
                device,
                *swapchain_img_view,
                core::ptr::null_mut()
            );
        }
    }

    swapchain_img_views.clear();
}

//
// Updating HDR Color buffers and Swapchain images in postproc descriptor sets
//

unsafe fn update_postprocess_descriptor_sets(
    device: VkDevice,
    avg_luminance_descriptor_sets: &mut Vec<VkDescriptorSet>,
    postprocessing_descriptor_sets: &mut Vec<VkDescriptorSet>,
    color_buffer_views: &Vec<VkImageView>,
    swapchain_img_views: &Vec<VkImageView>
)
{
    for (i, (swapchain_img_view, (color_buffer_view, (avg_luminance_descriptor_set, postprocess_descriptor_set)))) in swapchain_img_views.iter().zip(color_buffer_views.iter().zip(avg_luminance_descriptor_sets.iter().zip(postprocessing_descriptor_sets.iter()))).enumerate()
    {
        let rendered_img_descriptor_write = [
            VkDescriptorImageInfo {
                sampler: std::ptr::null_mut(),
                imageView: *color_buffer_view,
                imageLayout: VK_IMAGE_LAYOUT_GENERAL
            }
        ];

        let swapchain_img_descriptor_write = [
            VkDescriptorImageInfo {
                sampler: std::ptr::null_mut(),
                imageView: *swapchain_img_view,
                imageLayout: VK_IMAGE_LAYOUT_GENERAL
            }
        ];

        let descriptor_set_writes = [
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: core::ptr::null(),
                dstSet: *avg_luminance_descriptor_set,
                dstBinding: 0,
                dstArrayElement: 0,
                descriptorCount: rendered_img_descriptor_write.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
                pImageInfo: rendered_img_descriptor_write.as_ptr(),
                pBufferInfo: core::ptr::null(),
                pTexelBufferView: core::ptr::null()
            },
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: core::ptr::null(),
                dstSet: *postprocess_descriptor_set,
                dstBinding: 0,
                dstArrayElement: 0,
                descriptorCount: rendered_img_descriptor_write.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
                pImageInfo: rendered_img_descriptor_write.as_ptr(),
                pBufferInfo: core::ptr::null(),
                pTexelBufferView: core::ptr::null()
            },
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: core::ptr::null(),
                dstSet: *postprocess_descriptor_set,
                dstBinding: 1,
                dstArrayElement: 0,
                descriptorCount: swapchain_img_descriptor_write.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
                pImageInfo: swapchain_img_descriptor_write.as_ptr(),
                pBufferInfo: core::ptr::null(),
                pTexelBufferView: core::ptr::null()
            }

        ];

        println!("Updating color buffer and swapchain image in avg luminance and postprocess descriptor set {:?}.", i);
        unsafe
        {
            vkUpdateDescriptorSets(
                device,
                descriptor_set_writes.len() as u32,
                descriptor_set_writes.as_ptr(),
                0,
                core::ptr::null()
            );
        }
    }
}

fn main()
{
    let cl_params = parse_command_line_args();

    // Creating SDL2 window
    let width = 800;
    let height = 600;

    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video.window(
        "Tutorial",
        width,
        height
    ).vulkan().resizable().build().unwrap();

    // Querying platform specific WSI extensions from SDL2
    let instance_extensions = window
        .vulkan_instance_extensions()
        .unwrap()
        .iter()
        .map(|&v| v.as_ptr() as *const i8)
        .collect::<Vec<*const i8>>();

    //
    // Layers
    //

    let std_validation_layer = b"VK_LAYER_KHRONOS_validation\0";
    let layers = [std_validation_layer.as_ptr() as *const i8];
    let no_layers = [];

    let layer_slice;
    if cl_params.enable_validation
    {
        let mut available_layer_count = 0;
        let mut available_layers = Vec::new();
        unsafe
        {
            vkEnumerateInstanceLayerProperties(
                &mut available_layer_count,
                core::ptr::null_mut()
            );
        }

        available_layers.resize(available_layer_count as usize, VkLayerProperties::default());
        unsafe
        {
            vkEnumerateInstanceLayerProperties(
                &mut available_layer_count,
                available_layers.as_mut_ptr()
            );
        }

        for layer in layers.iter()
        {
            let layer = unsafe { core::ffi::CStr::from_ptr(*layer) };
            let mut found = false;
            for available_layer in available_layers.iter()
            {
                let available_layer = unsafe
                {
                    core::ffi::CStr::from_ptr(
                        available_layer.layerName.as_ptr()
                    )
                };

                if layer == available_layer
                {
                    found = true;
                }
            }

            if !found
            {
                println!("Layer {:?} is not supported.", layer);
            }
        }

        layer_slice = &layers[..];
    }
    else
    {
        layer_slice = &no_layers[..];
    }

    //
    // Physical device selection
    //

    let phys_device_index;
    if cl_params.gpu_index_set
    {
        phys_device_index = cl_params.gpu_index;
    }
    else
    {
        phys_device_index = 0;
    }

    //
    // Instance creation
    //

    let app_name_bytes = b"vk rust\0";
    let app_name = unsafe
    {
        core::ffi::CStr::from_bytes_with_nul_unchecked(
            app_name_bytes
        )
    };
    let engine_name_bytes = b"Tutorial engine\0";
    let engine_name = unsafe
    {
        core::ffi::CStr::from_bytes_with_nul_unchecked(
            engine_name_bytes
        )
    };

    let application_info = VkApplicationInfo {
        sType: VK_STRUCTURE_TYPE_APPLICATION_INFO,
        pNext: core::ptr::null(),
        pApplicationName: app_name.as_ptr(),
        applicationVersion: make_version(0, 0, 1, 0),
        pEngineName: engine_name.as_ptr(),
        engineVersion: make_version(0, 0, 1, 0),
        apiVersion: make_version(0, 1, 1, 0)
    };

    let create_info = VkInstanceCreateInfo {
        sType: VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        pApplicationInfo: &application_info,
        enabledExtensionCount: instance_extensions.len() as u32,
        ppEnabledExtensionNames: instance_extensions.as_ptr(),
        enabledLayerCount: layer_slice.len() as u32,
        ppEnabledLayerNames: layer_slice.as_ptr()
    };

    println!("Creating instance.");
    let mut instance = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateInstance(
            &create_info,
            core::ptr::null(),
            &mut instance
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create instance. Error: {:?}.", result);
    }

    //
    // Surface creation
    //

    println!("Creating surface.");
    let surface = window
        .vulkan_create_surface(instance as sdl2::video::VkInstance)
        .expect("Failed to create surface.")  as VkSurfaceKHR;

    //
    // Enumerating physical devices
    //

    let mut phys_device_count: u32 = 0;
    let result = unsafe
    {
        vkEnumeratePhysicalDevices(
            instance,
            &mut phys_device_count,
            core::ptr::null_mut()
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to enumerate physical devices. Error: {:?}.", result);
    }

    if phys_device_count == 0
    {
        panic!("No vulkan capable device found.");
    }

    let mut phys_devices = vec![core::ptr::null_mut(); phys_device_count as usize];
    let result = unsafe
    {
        vkEnumeratePhysicalDevices(
            instance,
            &mut phys_device_count,
            phys_devices.as_mut_ptr()
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to enumerate physical devices. Error: {:?}.", result);
    }

    if phys_devices.len() <= phys_device_index
    {
        panic!("Device index {:?} was given, but only {:?} devices are available", phys_device_index, phys_devices.len());
    }

    let chosen_phys_device = phys_devices[phys_device_index];

    //
    // Checking physical device capabilities
    //

    // Getting physical device properties
    let mut phys_device_subgroup_properties = VkPhysicalDeviceSubgroupProperties::default();
    phys_device_subgroup_properties.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_SUBGROUP_PROPERTIES;

    let mut phys_device_properties2 = VkPhysicalDeviceProperties2::default();
    phys_device_properties2.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2;
    phys_device_properties2.pNext = &mut phys_device_subgroup_properties as *mut _ as *mut core::ffi::c_void;

    unsafe
    {
        vkGetPhysicalDeviceProperties2(
            chosen_phys_device,
            &mut phys_device_properties2
        );
    }

    let phys_device_properties = &phys_device_properties2.properties;

    let device_name = unsafe
    {
        core::ffi::CStr::from_ptr(
            phys_device_properties.deviceName.as_ptr()
        )
    };
    println!("Chosen device name: {:?}", device_name);

    // Checking physical device limits
    // This one is actually unnecessary, because the minimum will always be at least 128,
    // but for larger workgroups you may want to check this.
    if phys_device_properties.limits.maxComputeWorkGroupInvocations < 64
    {
        panic!("maxComputeWorkGroupInvocations must be at least 64. Actual value: {:?}", phys_device_properties.limits.maxComputeWorkGroupInvocations);
    }

    if phys_device_subgroup_properties.subgroupSize < 32
    {
        panic!("subgroupSize must be at least 32. Actual value: {:?}", phys_device_subgroup_properties.subgroupSize);
    }

    if phys_device_subgroup_properties.supportedOperations | VK_SUBGROUP_FEATURE_ARITHMETIC_BIT as VkSubgroupFeatureFlags == 0
    {
        panic!("Subgroup operation VK_SUBGROUP_FEATURE_ARITHMETIC_BIT is required.");
    }

    // Checking device extensions
    let mut device_extension_count: u32 = 0;
    unsafe
    {
        vkEnumerateDeviceExtensionProperties(
            chosen_phys_device,
            core::ptr::null(),
            &mut device_extension_count,
            core::ptr::null_mut()
        );
    }

    let mut device_extensions = vec![VkExtensionProperties::default(); device_extension_count as usize];
    unsafe
    {
        vkEnumerateDeviceExtensionProperties(
            chosen_phys_device,
            core::ptr::null(),
            &mut device_extension_count,
            device_extensions.as_mut_ptr()
        );
    }

    let mut ext_swapchain_found = false;
    {
        let extension_name = unsafe
        {
            core::ffi::CStr::from_bytes_with_nul_unchecked(
                VK_KHR_SWAPCHAIN_EXTENSION_NAME
            )
        };

        for ext_properties in device_extensions.iter()
        {
            let current_ext_name = unsafe
            {
                core::ffi::CStr::from_ptr(
                    ext_properties.extensionName.as_ptr()
                )
            };

            if current_ext_name == extension_name
            {
                ext_swapchain_found = true;
            }
        }
    }

    // Checking queues
    let mut queue_family_count: u32 = 0;
    unsafe
    {
        vkGetPhysicalDeviceQueueFamilyProperties(
            chosen_phys_device,
            &mut queue_family_count,
            core::ptr::null_mut()
        );
    }

    let mut queue_families = vec![VkQueueFamilyProperties::default(); queue_family_count as usize];
    unsafe
    {
        vkGetPhysicalDeviceQueueFamilyProperties(
            chosen_phys_device,
            &mut queue_family_count,
            queue_families.as_mut_ptr()
        );
    }

    let mut chosen_graphics_queue_family: i32 = -1;
    let mut chosen_graphics_queue_index: u32 = 0;
    let mut chosen_present_queue_family: i32 = -1;
    let mut chosen_present_queue_index: u32 = 0;

    for i in 0..queue_families.len()
    {
        let queue_family_index = i as i32;
        let queue_family = &queue_families[i];

        // Checking for present support
        let mut present_supported = false;
        {
            let mut present_support: VkBool32 = VK_FALSE;
            let result = unsafe
            {
                vkGetPhysicalDeviceSurfaceSupportKHR(
                    chosen_phys_device,
                    queue_family_index as u32,
                    surface,
                    &mut present_support
                )
            };

            if result == VK_SUCCESS && present_support != VK_FALSE
            {
                present_supported = true;
            }
        }

        if queue_family.queueFlags & VK_QUEUE_GRAPHICS_BIT as VkQueueFlags != 0
        {
            chosen_graphics_queue_family = queue_family_index;
            chosen_graphics_queue_index = 0;

            if present_supported && chosen_present_queue_family == -1
            {
                chosen_present_queue_family = queue_family_index;
                chosen_present_queue_index = 0;

                if queue_family.queueCount > 1
                {
                    chosen_present_queue_index = 1;
                }
            }
        }

        if queue_family.queueFlags & VK_QUEUE_GRAPHICS_BIT as VkQueueFlags == 0
        {
            if present_supported
            {
                chosen_present_queue_family = queue_family_index;
                chosen_present_queue_index = 0;
            }
        }
    }

    if !(ext_swapchain_found && chosen_graphics_queue_family != -1 &&
         chosen_present_queue_family != -1)
    {
        panic!("Chosen physical device is not suitable.");
    }

    // Getting physical device features
    let mut phys_device_features = VkPhysicalDeviceFeatures::default();
    unsafe
    {
        vkGetPhysicalDeviceFeatures(
            chosen_phys_device,
            &mut phys_device_features
        );
    }

    if phys_device_features.shaderUniformBufferArrayDynamicIndexing != VK_TRUE
    {
        panic!("shaderUniformBufferArrayDynamicIndexing feature is not supported.");
    }

    if phys_device_features.shaderSampledImageArrayDynamicIndexing != VK_TRUE
    {
        panic!("shaderSampledImageArrayDynamicIndexing feature is not supported.");
    }

    if phys_device_features.shaderStorageImageWriteWithoutFormat != VK_TRUE
    {
        panic!("shaderStorageImageWriteWithoutFormat feature is not supported.");
    }

    let chosen_graphics_queue_family = chosen_graphics_queue_family as u32;
    let chosen_present_queue_family = chosen_present_queue_family as u32;

    //
    // Device mem properties
    //

    let mut phys_device_mem_properties = VkPhysicalDeviceMemoryProperties::default();
    unsafe
    {
        vkGetPhysicalDeviceMemoryProperties(
            chosen_phys_device,
            &mut phys_device_mem_properties
        );
    }

    print_memory_properties(&phys_device_mem_properties);

    //
    // Device creation
    //

    let queue_priorities: [f32; 2] = [1.0; 2];

    let queue_create_info_count: u32;
    let mut queue_create_infos = [VkDeviceQueueCreateInfo::default(); 2];

    if chosen_graphics_queue_family == chosen_present_queue_family
    {
        let family_queue_count;
        if chosen_graphics_queue_index == chosen_present_queue_index
        {
            family_queue_count = 1
        }
        else
        {
            family_queue_count = 2
        }

        queue_create_infos[0] = VkDeviceQueueCreateInfo {
            sType: VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            queueFamilyIndex: chosen_graphics_queue_family,
            queueCount: family_queue_count,
            pQueuePriorities: queue_priorities.as_ptr()
        };

        queue_create_info_count = 1;
    }
    else
    {
        queue_create_infos[0] = VkDeviceQueueCreateInfo {
            sType: VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            queueFamilyIndex: chosen_graphics_queue_family,
            queueCount: 1,
            pQueuePriorities: queue_priorities.as_ptr()
        };

        queue_create_infos[1] = VkDeviceQueueCreateInfo {
            sType: VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            queueFamilyIndex: chosen_present_queue_family,
            queueCount: 1,
            pQueuePriorities: queue_priorities.as_ptr()
        };

        queue_create_info_count = 2;
    }

    let device_extensions = [VK_KHR_SWAPCHAIN_EXTENSION_NAME.as_ptr() as *const core::ffi::c_char];
    let mut phys_device_features = VkPhysicalDeviceFeatures::default();

    // Enabling requested features
    phys_device_features.shaderUniformBufferArrayDynamicIndexing = VK_TRUE;
    phys_device_features.shaderSampledImageArrayDynamicIndexing = VK_TRUE;
    phys_device_features.shaderStorageImageWriteWithoutFormat = VK_TRUE;

    let device_create_info = VkDeviceCreateInfo {
        sType: VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        queueCreateInfoCount: queue_create_info_count,
        pQueueCreateInfos: queue_create_infos.as_ptr(),
        enabledLayerCount: 0,
        ppEnabledLayerNames: core::ptr::null(),
        enabledExtensionCount: device_extensions.len() as u32,
        ppEnabledExtensionNames: device_extensions.as_ptr(),
        pEnabledFeatures: &phys_device_features
    };

    println!("Creating device.");
    let mut device = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateDevice(
            chosen_phys_device,
            &device_create_info,
            core::ptr::null_mut(),
            &mut device
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create vulkan device. Error: {:?}.", result);
    }

    let mut graphics_queue = core::ptr::null_mut();
    unsafe
    {
        vkGetDeviceQueue(
            device,
            chosen_graphics_queue_family,
            chosen_graphics_queue_index,
            &mut graphics_queue
        );
    }

    let mut present_queue = core::ptr::null_mut();
    unsafe
    {
        vkGetDeviceQueue(
            device,
            chosen_present_queue_family,
            chosen_present_queue_index,
            &mut present_queue
        );
    }

    //
    // Swapchain creation
    //

    let format = VK_FORMAT_B8G8R8A8_UNORM;
     let SwapchainResult {
        mut swapchain,
        mut width,
        mut height
    } = unsafe
    {
        create_swapchain(
            chosen_phys_device,
            surface,
            device,
            core::ptr::null_mut(),
            width,
            height,
            format,
            chosen_graphics_queue_family,
            chosen_present_queue_family
        )
    };

    //
    // RenderPass creation
    //

    let mut attachment_descs = Vec::new();

    let attachment_description = VkAttachmentDescription {
        flags: 0x0,
        format: VK_FORMAT_R32G32B32A32_SFLOAT,
        samples: VK_SAMPLE_COUNT_1_BIT,
        loadOp: VK_ATTACHMENT_LOAD_OP_CLEAR,
        storeOp: VK_ATTACHMENT_STORE_OP_STORE,
        stencilLoadOp: VK_ATTACHMENT_LOAD_OP_DONT_CARE,
        stencilStoreOp: VK_ATTACHMENT_STORE_OP_DONT_CARE,
        initialLayout: VK_IMAGE_LAYOUT_UNDEFINED,
        finalLayout: VK_IMAGE_LAYOUT_GENERAL
    };

    attachment_descs.push(attachment_description);

    let depth_attachment_description = VkAttachmentDescription {
        flags: 0x0,
        format: VK_FORMAT_D32_SFLOAT,
        samples: VK_SAMPLE_COUNT_1_BIT,
        loadOp: VK_ATTACHMENT_LOAD_OP_CLEAR,
        storeOp: VK_ATTACHMENT_STORE_OP_DONT_CARE,
        stencilLoadOp: VK_ATTACHMENT_LOAD_OP_DONT_CARE,
        stencilStoreOp: VK_ATTACHMENT_STORE_OP_DONT_CARE,
        initialLayout: VK_IMAGE_LAYOUT_UNDEFINED,
        finalLayout: VK_IMAGE_LAYOUT_DEPTH_STENCIL_ATTACHMENT_OPTIMAL
    };

    attachment_descs.push(depth_attachment_description);

    let mut color_attachment_refs = Vec::new();

    let new_attachment_ref = VkAttachmentReference {
        attachment: 0,
        layout: VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL
    };

    color_attachment_refs.push(new_attachment_ref);

    let depth_attachment_ref = VkAttachmentReference {
        attachment: 1,
        layout: VK_IMAGE_LAYOUT_DEPTH_STENCIL_ATTACHMENT_OPTIMAL
    };

    let mut subpass_descs = Vec::new();

    let subpass_description = VkSubpassDescription {
        flags: 0x0,
        pipelineBindPoint: VK_PIPELINE_BIND_POINT_GRAPHICS,
        inputAttachmentCount: 0,
        pInputAttachments: core::ptr::null(),
        colorAttachmentCount: color_attachment_refs.len() as u32,
        pColorAttachments: color_attachment_refs.as_ptr(),
        pResolveAttachments: core::ptr::null(),
        pDepthStencilAttachment: &depth_attachment_ref,
        preserveAttachmentCount: 0,
        pPreserveAttachments: core::ptr::null()
    };

    subpass_descs.push(subpass_description);

    let render_pass_create_info = VkRenderPassCreateInfo {
        sType: VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        attachmentCount: attachment_descs.len() as u32,
        pAttachments: attachment_descs.as_ptr(),
        subpassCount: subpass_descs.len() as u32,
        pSubpasses: subpass_descs.as_ptr(),
        dependencyCount: 0,
        pDependencies: std::ptr::null()
    };

    println!("Creating render pass.");
    let mut render_pass = core::ptr::null_mut();
    let result = unsafe { vkCreateRenderPass(device, &render_pass_create_info, core::ptr::null_mut(), &mut render_pass) };

    if result != VK_SUCCESS
    {
        panic!("Failed to create render pass. Error: {:?}.", result);
    }

    //
    // Getting swapchain images and framebuffer creation
    //

    let mut swapchain_imgs = Vec::new();
    let mut swapchain_img_views = Vec::new();
    let mut color_buffers = Vec::new();
    let mut color_buffer_memories = Vec::new();
    let mut color_buffer_views = Vec::new();
    let mut depth_buffers = Vec::new();
    let mut depth_buffer_memories = Vec::new();
    let mut depth_buffer_views = Vec::new();
    let mut framebuffers = Vec::new();
    unsafe
    {
        create_framebuffers_and_render_targets(
            device,
            chosen_phys_device,
            &phys_device_mem_properties,
            width,
            height,
            format,
            render_pass,
            swapchain,
            &mut swapchain_imgs,
            &mut swapchain_img_views,
            &mut color_buffers,
            &mut color_buffer_memories,
            &mut color_buffer_views,
            &mut depth_buffers,
            &mut depth_buffer_memories,
            &mut depth_buffer_views,
            &mut framebuffers
        );
    }

    let frame_count = swapchain_imgs.len();

    //
    // Average luminance image
    //

    let mut format_properties = VkFormatProperties::default();
    unsafe
    {
        vkGetPhysicalDeviceFormatProperties(
            chosen_phys_device,
            VK_FORMAT_R32_SFLOAT,
            &mut format_properties
        );
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R32_SFLOAT with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT.");
    }

    let mut avg_luminance_images = Vec::with_capacity(frame_count);
    let mut avg_luminance_image_memories = Vec::with_capacity(frame_count);
    let mut avg_luminance_image_views = Vec::with_capacity(frame_count);
    let avg_luminance_image_dim = 8;
    for i in 0..frame_count
    {
        let image_create_info = VkImageCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            imageType: VK_IMAGE_TYPE_2D,
            format: VK_FORMAT_R32_SFLOAT,
            extent: VkExtent3D {
                width: avg_luminance_image_dim as u32,
                height: avg_luminance_image_dim as u32,
                depth: 1
            },
            mipLevels: 1,
            arrayLayers: 1,
            samples: VK_SAMPLE_COUNT_1_BIT,
            tiling: VK_IMAGE_TILING_OPTIMAL,
            usage: VK_IMAGE_USAGE_STORAGE_BIT as VkImageUsageFlags,
            sharingMode: VK_SHARING_MODE_EXCLUSIVE,
            queueFamilyIndexCount: 0,
            pQueueFamilyIndices: core::ptr::null(),
            initialLayout: VK_IMAGE_LAYOUT_UNDEFINED
        };

        println!("Creating avg luminance image.");
        let mut avg_luminance_image = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImage(
                device,
                &image_create_info,
                core::ptr::null_mut(),
                &mut avg_luminance_image
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create avg luminance image {}. Error: {}", i, result);
        }

        avg_luminance_images.push(avg_luminance_image);

        let mut mem_requirements = VkMemoryRequirements::default();
        unsafe
        {
            vkGetImageMemoryRequirements(
                device,
                avg_luminance_image,
                &mut mem_requirements
            );
        }

        let avg_luminance_image_mem_props = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT as VkMemoryPropertyFlags;
        let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
        for i in 0..phys_device_mem_properties.memoryTypeCount
        {
            if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
                (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & avg_luminance_image_mem_props) ==
                    avg_luminance_image_mem_props
            {
                chosen_memory_type = i;
                break;
            }
        }

        if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
        {
            panic!("Could not find memory type.");
        }

        let image_alloc_info = VkMemoryAllocateInfo {
            sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            allocationSize: mem_requirements.size,
            memoryTypeIndex: chosen_memory_type
        };

        println!("Avg luminance image size: {}", mem_requirements.size);
        println!("Avg luminance image align: {}", mem_requirements.alignment);

        println!("Allocating avg luminance image memory");
        let mut avg_luminance_image_memory = core::ptr::null_mut();
        let result = unsafe
        {
            vkAllocateMemory(
                device,
                &image_alloc_info,
                core::ptr::null(),
                &mut avg_luminance_image_memory
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Could not allocate memory for avg luminance image {}. Error: {}", i, result);
        }

        let result = unsafe
        {
            vkBindImageMemory(
                device,
                avg_luminance_image,
                avg_luminance_image_memory,
                0
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to bind memory to avg luminance image {}. Error: {}", i, result);
        }

        avg_luminance_image_memories.push(avg_luminance_image_memory);

        //
        // Average luminance image view
        //

        let image_view_create_info = VkImageViewCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            image: avg_luminance_image,
            viewType: VK_IMAGE_VIEW_TYPE_2D,
            format: VK_FORMAT_R32_SFLOAT,
            components: VkComponentMapping {
                r: VK_COMPONENT_SWIZZLE_IDENTITY,
                g: VK_COMPONENT_SWIZZLE_IDENTITY,
                b: VK_COMPONENT_SWIZZLE_IDENTITY,
                a: VK_COMPONENT_SWIZZLE_IDENTITY
            },
            subresourceRange: VkImageSubresourceRange {
                aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                baseMipLevel: 0,
                levelCount: 1,
                baseArrayLayer: 0,
                layerCount: 1
            }
        };

        println!("Creating avg luminance image view.");
        let mut avg_luminance_image_view = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImageView(
                device,
                &image_view_create_info,
                core::ptr::null_mut(),
                &mut avg_luminance_image_view
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create avg luminance image view {}. Error: {}", i, result);
        }

        avg_luminance_image_views.push(avg_luminance_image_view);
    }

    //
    // Postprocessing data
    //

    let avg_luminance_begin = 0;
    let avg_luminance_size = frame_count * core::mem::size_of::<f32>();

    let min_sbo_offset_alignment = phys_device_properties.limits.minStorageBufferOffsetAlignment as usize;

    let avg_luminance_align_rem = avg_luminance_size % min_sbo_offset_alignment;
    let avg_luminance_padding = if avg_luminance_size != 0 {min_sbo_offset_alignment - avg_luminance_align_rem} else {0};

    let padded_avg_luminance_size = avg_luminance_size + avg_luminance_padding;

    let atomic_cnt_begin = avg_luminance_begin + padded_avg_luminance_size;
    let atomic_cnt_size = frame_count * core::mem::size_of::<u32>();

    let postprocessing_buffer_size = padded_avg_luminance_size + atomic_cnt_size;

    //
    // Postprocessing buffer
    //

    // Create buffer

    let postprocessing_buffer_create_info = VkBufferCreateInfo {
        sType: VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        size: postprocessing_buffer_size as VkDeviceSize,
        usage: (VK_BUFFER_USAGE_STORAGE_BUFFER_BIT |
                VK_BUFFER_USAGE_TRANSFER_DST_BIT) as VkBufferUsageFlags,
        sharingMode: VK_SHARING_MODE_EXCLUSIVE,
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: core::ptr::null()
    };

    println!("Creating postprocessing buffer.");
    let mut postprocessing_buffer = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateBuffer(
            device,
            &postprocessing_buffer_create_info,
            core::ptr::null(),
            &mut postprocessing_buffer
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create postprocessing buffer. Error: {}.", result);
    }

    // Create memory

    let mut mem_requirements = VkMemoryRequirements::default();
    unsafe
    {
        vkGetBufferMemoryRequirements(
            device,
            postprocessing_buffer,
            &mut mem_requirements
        );
    }

    let postprocessing_buffer_mem_props = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT as VkMemoryPropertyFlags;
    let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
    for i in 0..phys_device_mem_properties.memoryTypeCount
    {
        if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
            (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & postprocessing_buffer_mem_props) ==
            postprocessing_buffer_mem_props
        {
            chosen_memory_type = i;
            break;
        }
    }

    if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
    {
        panic!("Could not find memory type.");
    }

    let postprocessing_buffer_alloc_info = VkMemoryAllocateInfo {
        sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: core::ptr::null(),
        allocationSize: mem_requirements.size,
        memoryTypeIndex: chosen_memory_type
    };

    println!("Postprocessing buffer size: {}", mem_requirements.size);
    println!("Postprocessing buffer align: {}", mem_requirements.alignment);

    println!("Allocating postprocessing buffer memory.");
    let mut postprocessing_buffer_memory = core::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateMemory(
            device,
            &postprocessing_buffer_alloc_info,
            core::ptr::null(),
            &mut postprocessing_buffer_memory
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Could not allocate memory. Error: {}.", result);
    }

    // Bind buffer to memory

    println!("Binding postprocessing buffer memory.");
    let result = unsafe
    {
        vkBindBufferMemory(
            device,
            postprocessing_buffer,
            postprocessing_buffer_memory,
            0
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to bind memory to postprocessing buffer. Error: {}.", result);
    }

    //
    // Average luminance image and buffer initialization
    //

    {
        let cmd_pool_create_info = VkCommandPoolCreateInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            queueFamilyIndex: chosen_graphics_queue_family
        };

        println!("Creating avg luminance init command pool.");
        let mut cmd_pool = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateCommandPool(
                device,
                &cmd_pool_create_info,
                core::ptr::null_mut(),
                &mut cmd_pool
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create avg luminance init command pool. Error: {}.", result);
        }

        println!("Allocating avg luminance init command buffers.");
        let cmd_buffer_alloc_info = VkCommandBufferAllocateInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            commandPool: cmd_pool,
            level: VK_COMMAND_BUFFER_LEVEL_PRIMARY,
            commandBufferCount: 1
        };

        let mut avg_luminance_init_cmd_buffer = core::ptr::null_mut();
        let result = unsafe
        {
            vkAllocateCommandBuffers(
                device,
                &cmd_buffer_alloc_info,
                &mut avg_luminance_init_cmd_buffer
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create avg luminance init command buffer. Error: {}.", result);
        }

        let cmd_buffer_begin_info = VkCommandBufferBeginInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
            pNext: core::ptr::null(),
            flags: VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT as VkCommandBufferUsageFlags,
            pInheritanceInfo: core::ptr::null()
        };

        let result = unsafe
        {
            vkBeginCommandBuffer(
                avg_luminance_init_cmd_buffer,
                &cmd_buffer_begin_info
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to start recording the comand buffer. Error: {}.", result);
        }

        //
        // Perform layout transition and fill buffer with zeros
        //

        let mut avg_luminance_barriers = Vec::with_capacity(frame_count);

        for avg_luminance_image in avg_luminance_images.iter()
        {
            avg_luminance_barriers.push(
                VkImageMemoryBarrier {
                    sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                    pNext: std::ptr::null(),
                    srcAccessMask: 0,
                    dstAccessMask: VK_ACCESS_SHADER_WRITE_BIT as VkAccessFlags,
                    oldLayout: VK_IMAGE_LAYOUT_UNDEFINED,
                    newLayout: VK_IMAGE_LAYOUT_GENERAL,
                    srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                    dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                    image: *avg_luminance_image,
                    subresourceRange: VkImageSubresourceRange {
                        aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                        baseMipLevel: 0,
                        levelCount: 1,
                        baseArrayLayer: 0,
                        layerCount: 1
                    }
                }
            );
        }

        unsafe
        {
            vkCmdPipelineBarrier(
                avg_luminance_init_cmd_buffer,
                VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT as VkPipelineStageFlags,
                VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT as VkPipelineStageFlags,
                0,
                0,
                core::ptr::null(),
                0,
                core::ptr::null(),
                avg_luminance_barriers.len() as u32,
                avg_luminance_barriers.as_ptr()
            );
        }

        unsafe
        {
            vkCmdFillBuffer(
                avg_luminance_init_cmd_buffer,
                postprocessing_buffer,
                0,
                postprocessing_buffer_size as VkDeviceSize,
                0
            );
        }

        let result = unsafe
        {
            vkEndCommandBuffer(
                avg_luminance_init_cmd_buffer
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to end recording the comand buffer. Error: {}.", result);
        }

        let cmd_buffer = [
            avg_luminance_init_cmd_buffer
        ];

        let submit_info = VkSubmitInfo {
            sType: VK_STRUCTURE_TYPE_SUBMIT_INFO,
            pNext: core::ptr::null(),
            waitSemaphoreCount: 0,
            pWaitSemaphores: core::ptr::null(),
            pWaitDstStageMask: core::ptr::null(),
            commandBufferCount: cmd_buffer.len() as u32,
            pCommandBuffers: cmd_buffer.as_ptr(),
            signalSemaphoreCount: 0,
            pSignalSemaphores: core::ptr::null()
        };

        let result = unsafe
        {
            vkQueueSubmit(
                graphics_queue,
                1,
                &submit_info,
                core::ptr::null_mut()
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to submit avg luminance init commands: {:?}.", result);
        }

        //
        // Cleanup
        //

        let _result = unsafe
        {
            vkQueueWaitIdle(graphics_queue)
        };

        println!("Deleting avg luminance init command pool.");
        unsafe
        {
            vkDestroyCommandPool(
                device,
                cmd_pool,
                core::ptr::null_mut()
            );
        }
    }

    //
    // Shader modules
    //

    // Vertex shader

    let mut file = std::fs::File::open(
        "./shaders/06_3d_normal.vert.spv"
    ).expect("Could not open shader source");

    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode).expect("Failed to read vertex shader source");

    let shader_module_create_info = VkShaderModuleCreateInfo {
        sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        codeSize: bytecode.len(),
        pCode: bytecode.as_ptr() as *const u32
    };

    println!("Creating model vertex shader module.");
    let mut model_vertex_shader_module = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateShaderModule(
            device,
            &shader_module_create_info,
            core::ptr::null_mut(),
            &mut model_vertex_shader_module
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create vertex shader. Error: {}", result);
    }

    // Fragment shader

    let mut file = std::fs::File::open(
        "./shaders/08_sphere_light_hdr.frag.spv"
    ).expect("Could not open shader source");

    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode).expect("Failed to read fragment shader source");

    let shader_module_create_info = VkShaderModuleCreateInfo {
        sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        codeSize: bytecode.len(),
        pCode: bytecode.as_ptr() as *const u32
    };

    println!("Creating model fragment shader module.");
    let mut model_fragment_shader_module = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateShaderModule(
            device,
            &shader_module_create_info,
            core::ptr::null_mut(),
            &mut model_fragment_shader_module
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create fragment shader. Error: {}", result);
    }

    // Skydome vertex shader

    let mut file = std::fs::File::open(
        "./shaders/07_skydome.vert.spv"
    ).expect("Could not open shader source");

    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode).expect("Failed to read shader source");

    let shader_module_create_info = VkShaderModuleCreateInfo {
        sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        codeSize: bytecode.len(),
        pCode: bytecode.as_ptr() as *const u32
    };

    println!("Creating skydome vertex shader module.");
    let mut skydome_vertex_shader_module = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateShaderModule(
            device,
            &shader_module_create_info,
            std::ptr::null_mut(),
            &mut skydome_vertex_shader_module
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create skydome vertex shader. Error: {}.", result);
    }

    // Skydome fragment shader

    let mut file = std::fs::File::open(
        "./shaders/07_skydome_hdr.frag.spv"
    ).expect("Could not open shader source");

    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode).expect("Failed to read shader source");

    let shader_module_create_info = VkShaderModuleCreateInfo {
        sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        codeSize: bytecode.len(),
        pCode: bytecode.as_ptr() as *const u32
    };

    println!("Creating skydome fragment shader module.");
    let mut skydome_fragment_shader_module = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateShaderModule(
            device,
            &shader_module_create_info,
            std::ptr::null_mut(),
            &mut skydome_fragment_shader_module
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create skydome fragment shader. Error: {}.", result);
    }

    // Environment preinteg shader

    let mut file = std::fs::File::open(
        "./shaders/00_env_preinteg.comp.spv"
    ).expect("Could not open shader source");

    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode).expect("Failed to read shader source");

    let shader_module_create_info = VkShaderModuleCreateInfo {
        sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        codeSize: bytecode.len(),
        pCode: bytecode.as_ptr() as *const u32
    };

    println!("Creating env preinteg shader module.");
    let mut env_preinteg_shader_module = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateShaderModule(
            device,
            &shader_module_create_info,
            std::ptr::null_mut(),
            &mut env_preinteg_shader_module
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create env preinteg shader. Error: {}.", result);
    }

    // Dfg preinteg shader

    let mut file = std::fs::File::open(
        "./shaders/01_dfg_preinteg.comp.spv"
    ).expect("Could not open shader source");

    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode).expect("Failed to read shader source");

    let shader_module_create_info = VkShaderModuleCreateInfo {
        sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        codeSize: bytecode.len(),
        pCode: bytecode.as_ptr() as *const u32
    };

    println!("Creating dfg preinteg shader module.");
    let mut dfg_preinteg_shader_module = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateShaderModule(
            device,
            &shader_module_create_info,
            std::ptr::null_mut(),
            &mut dfg_preinteg_shader_module
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create dfg preinteg shader. Error: {}.", result);
    }

    // Average luminance shader

    let mut file = std::fs::File::open(
        "./shaders/02_avg_luminance.comp.spv"
    ).expect("Could not open shader source");

    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode).expect("Failed to read shader source");

    let shader_module_create_info = VkShaderModuleCreateInfo {
        sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        codeSize: bytecode.len(),
        pCode: bytecode.as_ptr() as *const u32
    };

    println!("Creating avg luminance shader module.");
    let mut avg_luminance_shader_module = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateShaderModule(
            device,
            &shader_module_create_info,
            std::ptr::null_mut(),
            &mut avg_luminance_shader_module
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create avg luminance shader. Error: {}.", result);
    }

    // Postprocessing shader

    let mut file = std::fs::File::open(
        "./shaders/03_postprocess.comp.spv"
    ).expect("Could not open shader source");

    let mut bytecode = Vec::new();
    file.read_to_end(&mut bytecode).expect("Failed to read shader source");

    let shader_module_create_info = VkShaderModuleCreateInfo {
        sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        codeSize: bytecode.len(),
        pCode: bytecode.as_ptr() as *const u32
    };

    println!("Creating postprocessing shader module.");
    let mut postprocessing_shader_module = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateShaderModule(
            device,
            &shader_module_create_info,
            std::ptr::null_mut(),
            &mut postprocessing_shader_module
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create postprocessing shader. Error: {}.", result);
    }

    //
    // Descriptor set layout
    //

    // Rendering

    let max_ubo_descriptor_count = 8;
    let max_tex2d_descriptor_count = 3;
    let max_texcube_descriptor_count = 2;

    let layout_bindings = [
        VkDescriptorSetLayoutBinding {
            binding: 0,
            descriptorType: VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
            descriptorCount: max_ubo_descriptor_count,
            stageFlags: VK_SHADER_STAGE_VERTEX_BIT as VkShaderStageFlags,
            pImmutableSamplers: core::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 1,
            descriptorType: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
            descriptorCount: max_tex2d_descriptor_count,
            stageFlags: VK_SHADER_STAGE_FRAGMENT_BIT as VkShaderStageFlags,
            pImmutableSamplers: core::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 2,
            descriptorType: VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
            descriptorCount: max_ubo_descriptor_count,
            stageFlags: VK_SHADER_STAGE_FRAGMENT_BIT as VkShaderStageFlags,
            pImmutableSamplers: core::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 3,
            descriptorType: VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
            descriptorCount: max_ubo_descriptor_count,
            stageFlags: VK_SHADER_STAGE_FRAGMENT_BIT as VkShaderStageFlags,
            pImmutableSamplers: core::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 4,
            descriptorType: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
            descriptorCount: max_texcube_descriptor_count,
            stageFlags: VK_SHADER_STAGE_FRAGMENT_BIT as VkShaderStageFlags,
            pImmutableSamplers: core::ptr::null()
        }
    ];

    let descriptor_set_layout_create_info = VkDescriptorSetLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        bindingCount: layout_bindings.len() as u32,
        pBindings: layout_bindings.as_ptr()
    };

    println!("Creating descriptor set layout.");
    let mut descriptor_set_layout = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateDescriptorSetLayout(
            device,
            &descriptor_set_layout_create_info,
            core::ptr::null_mut(),
            &mut descriptor_set_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create descriptor set layout. Error: {}.", result);
    }

    // Environment map preintegration

    const MAX_ENV_MIP_LVL_COUNT: usize = 8;

    let compute_layout_bindings = [
        VkDescriptorSetLayoutBinding {
            binding: 0,
            descriptorType: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 1,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
            descriptorCount: MAX_ENV_MIP_LVL_COUNT as u32,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        }
    ];

    let descriptor_set_layout_create_info = VkDescriptorSetLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        bindingCount: compute_layout_bindings.len() as u32,
        pBindings: compute_layout_bindings.as_ptr()
    };

    println!("Creating env preinteg descriptor set layout.");
    let mut env_preinteg_descriptor_set_layout = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateDescriptorSetLayout(
            device,
            &descriptor_set_layout_create_info,
            std::ptr::null_mut(),
            &mut env_preinteg_descriptor_set_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create env preinteg descriptor set layout. Error: {}.", result);
    }

    // Dfg preintegration

    let compute_layout_bindings = [
        VkDescriptorSetLayoutBinding {
            binding: 0,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        }
    ];

    let descriptor_set_layout_create_info = VkDescriptorSetLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        bindingCount: compute_layout_bindings.len() as u32,
        pBindings: compute_layout_bindings.as_ptr()
    };

    println!("Creating dfg preinteg descriptor set layout.");
    let mut dfg_preinteg_descriptor_set_layout = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateDescriptorSetLayout(
            device,
            &descriptor_set_layout_create_info,
            std::ptr::null_mut(),
            &mut dfg_preinteg_descriptor_set_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create dfg preinteg descriptor set layout. Error: {}.", result);
    }

    // Average luminance

    let compute_layout_bindings = [
        VkDescriptorSetLayoutBinding {
            binding: 0,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 1,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 2,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 3,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        }
    ];

    let descriptor_set_layout_create_info = VkDescriptorSetLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        bindingCount: compute_layout_bindings.len() as u32,
        pBindings: compute_layout_bindings.as_ptr()
    };

    println!("Creating avg luminance descriptor set layout.");
    let mut avg_luminance_descriptor_set_layout = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateDescriptorSetLayout(
            device,
            &descriptor_set_layout_create_info,
            std::ptr::null_mut(),
            &mut avg_luminance_descriptor_set_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create avg luminance descriptor set layout. Error: {}.", result);
    }

    // Postprocessing

    let compute_layout_bindings = [
        VkDescriptorSetLayoutBinding {
            binding: 0,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 1,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        },
        VkDescriptorSetLayoutBinding {
            binding: 2,
            descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
            descriptorCount: 1,
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            pImmutableSamplers: std::ptr::null()
        }
    ];

    let descriptor_set_layout_create_info = VkDescriptorSetLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        bindingCount: compute_layout_bindings.len() as u32,
        pBindings: compute_layout_bindings.as_ptr()
    };

    println!("Creating postprocessing descriptor set layout.");
    let mut postprocessing_descriptor_set_layout = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateDescriptorSetLayout(
            device,
            &descriptor_set_layout_create_info,
            std::ptr::null_mut(),
            &mut postprocessing_descriptor_set_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create postprocessing descriptor set layout. Error: {}.", result);
    }

    //
    // Pipeline layout
    //

    // Rendering

    let descriptor_set_layouts = [
        descriptor_set_layout
    ];

    // Object ID + Frame ID
    let vertex_push_constant_size = 2 * core::mem::size_of::<u32>() as u32;
    // Object ID + Frame ID + Texture ID
    let fragment_push_constant_size = 3 * core::mem::size_of::<u32>() as u32;

    let push_constant_ranges = [
        VkPushConstantRange {
            stageFlags: VK_SHADER_STAGE_VERTEX_BIT as VkShaderStageFlags,
            offset: 0,
            size: vertex_push_constant_size,
        },
        VkPushConstantRange {
            stageFlags: VK_SHADER_STAGE_FRAGMENT_BIT as VkShaderStageFlags,
            offset: 0,
            size: fragment_push_constant_size,
        }
    ];

    let pipeline_layout_create_info = VkPipelineLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        setLayoutCount: descriptor_set_layouts.len() as u32,
        pSetLayouts: descriptor_set_layouts.as_ptr(),
        pushConstantRangeCount: push_constant_ranges.len() as u32,
        pPushConstantRanges: push_constant_ranges.as_ptr()
    };

    println!("Creating pipeline layout.");
    let mut pipeline_layout = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreatePipelineLayout(
            device,
            &pipeline_layout_create_info,
            core::ptr::null_mut(),
            &mut pipeline_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create pipeline layout. Error: {}", result);
    }

    // Environment preintegration

    let descriptor_set_layouts = [
        env_preinteg_descriptor_set_layout
    ];

    // Mip level + roughness
    let env_compute_push_constant_size = (std::mem::size_of::<u32>() + std::mem::size_of::<f32>()) as u32;

    let push_constant_ranges = [
        VkPushConstantRange {
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            offset: 0,
            size: env_compute_push_constant_size
        }
    ];

    let compute_pipeline_layout_create_info = VkPipelineLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        setLayoutCount: descriptor_set_layouts.len() as u32,
        pSetLayouts: descriptor_set_layouts.as_ptr(),
        pushConstantRangeCount: push_constant_ranges.len() as u32,
        pPushConstantRanges: push_constant_ranges.as_ptr()
    };

    println!("Creating env preinteg pipeline layout.");
    let mut env_compute_pipeline_layout = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreatePipelineLayout(
            device,
            &compute_pipeline_layout_create_info,
            std::ptr::null_mut(),
            &mut env_compute_pipeline_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create env preinteg pipeline layout. Error: {}.", result);
    }

    // Dfg preintegration

    let descriptor_set_layouts = [
        dfg_preinteg_descriptor_set_layout
    ];

    let compute_pipeline_layout_create_info = VkPipelineLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        setLayoutCount: descriptor_set_layouts.len() as u32,
        pSetLayouts: descriptor_set_layouts.as_ptr(),
        pushConstantRangeCount: 0,
        pPushConstantRanges: std::ptr::null_mut()
    };

    println!("Creating dfg preinteg pipeline layout.");
    let mut dfg_compute_pipeline_layout = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreatePipelineLayout(
            device,
            &compute_pipeline_layout_create_info,
            std::ptr::null_mut(),
            &mut dfg_compute_pipeline_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create dfg preinteg pipeline layout. Error: {}.", result);
    }

    // Average luminance

    let descriptor_set_layouts = [
        avg_luminance_descriptor_set_layout
    ];

    // Current frame index + previous frame index
    let avg_luminance_constant_size = (2 * core::mem::size_of::<u32>()) as u32;

    let push_constant_ranges = [
        VkPushConstantRange {
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            offset: 0,
            size: avg_luminance_constant_size
        }
    ];

    let compute_pipeline_layout_create_info = VkPipelineLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        setLayoutCount: descriptor_set_layouts.len() as u32,
        pSetLayouts: descriptor_set_layouts.as_ptr(),
        pushConstantRangeCount: push_constant_ranges.len() as u32,
        pPushConstantRanges: push_constant_ranges.as_ptr()
    };

    println!("Creating avg luminance pipeline layout.");
    let mut avg_luminance_pipeline_layout = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreatePipelineLayout(
            device,
            &compute_pipeline_layout_create_info,
            std::ptr::null_mut(),
            &mut avg_luminance_pipeline_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create avg luminance pipeline layout. Error: {}.", result);
    }

    // Postprocessing

    let descriptor_set_layouts = [
        postprocessing_descriptor_set_layout
    ];

    // Current frame index + exposure value + manual exposure flag
    let postprocessing_constant_size = (std::mem::size_of::<f32>() + 2 * core::mem::size_of::<u32>()) as u32;

    let push_constant_ranges = [
        VkPushConstantRange {
            stageFlags: VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
            offset: 0,
            size: postprocessing_constant_size
        }
    ];

    let compute_pipeline_layout_create_info = VkPipelineLayoutCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        setLayoutCount: descriptor_set_layouts.len() as u32,
        pSetLayouts: descriptor_set_layouts.as_ptr(),
        pushConstantRangeCount: push_constant_ranges.len() as u32,
        pPushConstantRanges: push_constant_ranges.as_ptr()
    };

    println!("Creating postprocessing pipeline layout.");
    let mut postprocessing_pipeline_layout = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreatePipelineLayout(
            device,
            &compute_pipeline_layout_create_info,
            std::ptr::null_mut(),
            &mut postprocessing_pipeline_layout
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create postprocessing pipeline layout. Error: {}.", result);
    }

    //
    // Pipeline state
    //

    // Graphics pipelines

    // Model pipeline state

    let model_shader_stage_info = [
        VkPipelineShaderStageCreateInfo {
            sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            pSpecializationInfo: core::ptr::null(),
            stage: VK_SHADER_STAGE_VERTEX_BIT,
            module: model_vertex_shader_module,
            pName: b"main\0".as_ptr() as *const i8
        },
        VkPipelineShaderStageCreateInfo {
            sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            pSpecializationInfo: core::ptr::null(),
            stage: VK_SHADER_STAGE_FRAGMENT_BIT,
            module: model_fragment_shader_module,
            pName: b"main\0".as_ptr() as *const i8
        }
    ];

    let vertex_bindings = [
        VkVertexInputBindingDescription {
            binding: 0,
            stride: 8 * core::mem::size_of::<f32>() as u32,
            inputRate: VK_VERTEX_INPUT_RATE_VERTEX,
        }
    ];

    let vertex_attributes = [
        VkVertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: VK_FORMAT_R32G32B32_SFLOAT,
            offset: 0,
        },
        VkVertexInputAttributeDescription {
            location: 1,
            binding: 0,
            format: VK_FORMAT_R32G32B32_SFLOAT,
            offset: 3 * core::mem::size_of::<f32>() as u32,
        },
        VkVertexInputAttributeDescription {
            location: 2,
            binding: 0,
            format: VK_FORMAT_R32G32_SFLOAT,
            offset: 6 * core::mem::size_of::<f32>() as u32,
        }
    ];

    let vertex_input_state = VkPipelineVertexInputStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        vertexBindingDescriptionCount: vertex_bindings.len() as u32,
        pVertexBindingDescriptions: vertex_bindings.as_ptr(),
        vertexAttributeDescriptionCount: vertex_attributes.len() as u32,
        pVertexAttributeDescriptions: vertex_attributes.as_ptr()
    };

    let input_assembly_state = VkPipelineInputAssemblyStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        topology: VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
        primitiveRestartEnable: VK_FALSE
    };

    let viewport_state = VkPipelineViewportStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        viewportCount: 1,
        pViewports: core::ptr::null(),
        scissorCount: 1,
        pScissors: core::ptr::null()
    };

    let rasterization_state = VkPipelineRasterizationStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        depthClampEnable: VK_FALSE,
        rasterizerDiscardEnable: VK_FALSE,
        polygonMode: VK_POLYGON_MODE_FILL,
        cullMode: VK_CULL_MODE_NONE as VkCullModeFlags,
        frontFace: VK_FRONT_FACE_COUNTER_CLOCKWISE,
        depthBiasEnable: VK_FALSE,
        depthBiasConstantFactor: 0.0,
        depthBiasClamp: 0.0,
        depthBiasSlopeFactor: 0.0,
        lineWidth: 1.0
    };

    let multisample_state = VkPipelineMultisampleStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        rasterizationSamples: VK_SAMPLE_COUNT_1_BIT,
        sampleShadingEnable: VK_FALSE,
        minSampleShading: 1.0,
        pSampleMask: core::ptr::null(),
        alphaToCoverageEnable: VK_FALSE,
        alphaToOneEnable: VK_FALSE
    };

    let model_depth_stencil_state = VkPipelineDepthStencilStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        depthTestEnable: VK_TRUE,
        depthWriteEnable: VK_TRUE,
        depthCompareOp: VK_COMPARE_OP_LESS,
        depthBoundsTestEnable: VK_FALSE,
        stencilTestEnable: VK_FALSE,
        front: VkStencilOpState {
            failOp: VK_STENCIL_OP_KEEP,
            passOp: VK_STENCIL_OP_KEEP,
            depthFailOp: VK_STENCIL_OP_KEEP,
            compareOp: VK_COMPARE_OP_NEVER,
            compareMask: 0,
            writeMask: 0,
            reference: 0
        },
        back: VkStencilOpState {
            failOp: VK_STENCIL_OP_KEEP,
            passOp: VK_STENCIL_OP_KEEP,
            depthFailOp: VK_STENCIL_OP_KEEP,
            compareOp: VK_COMPARE_OP_NEVER,
            compareMask: 0,
            writeMask: 0,
            reference: 0
        },
        minDepthBounds: 0.0,
        maxDepthBounds: 1.0
    };

    let color_blend_attachment_state = VkPipelineColorBlendAttachmentState {
        blendEnable: VK_FALSE,
        srcColorBlendFactor: VK_BLEND_FACTOR_ZERO,
        dstColorBlendFactor: VK_BLEND_FACTOR_ZERO,
        colorBlendOp: VK_BLEND_OP_ADD,
        srcAlphaBlendFactor: VK_BLEND_FACTOR_ZERO,
        dstAlphaBlendFactor: VK_BLEND_FACTOR_ZERO,
        alphaBlendOp: VK_BLEND_OP_ADD,
        colorWriteMask: (VK_COLOR_COMPONENT_R_BIT |
                         VK_COLOR_COMPONENT_G_BIT |
                         VK_COLOR_COMPONENT_B_BIT |
                         VK_COLOR_COMPONENT_A_BIT) as VkColorComponentFlags
    };

    let color_blend_state = VkPipelineColorBlendStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        logicOpEnable: VK_FALSE,
        logicOp: VK_LOGIC_OP_CLEAR,
        attachmentCount: 1,
        pAttachments: &color_blend_attachment_state,
        blendConstants: [0.0, 0.0, 0.0, 0.0]
    };

    // Dynamic state

    let dynamic_state_array = [VK_DYNAMIC_STATE_VIEWPORT, VK_DYNAMIC_STATE_SCISSOR];

    let dynamic_state = VkPipelineDynamicStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        dynamicStateCount: dynamic_state_array.len() as u32,
        pDynamicStates: dynamic_state_array.as_ptr(),
    };

    // Skydome pipeline state

    let skydome_shader_stage_info = [
        VkPipelineShaderStageCreateInfo {
            sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            pSpecializationInfo: core::ptr::null(),
            stage: VK_SHADER_STAGE_VERTEX_BIT,
            module: skydome_vertex_shader_module,
            pName: b"main\0".as_ptr() as *const i8
        },
        VkPipelineShaderStageCreateInfo {
            sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            pSpecializationInfo: core::ptr::null(),
            stage: VK_SHADER_STAGE_FRAGMENT_BIT,
            module: skydome_fragment_shader_module,
            pName: b"main\0".as_ptr() as *const i8
        }
    ];

    let skydome_depth_stencil_state = VkPipelineDepthStencilStateCreateInfo {
        sType: VK_STRUCTURE_TYPE_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        depthTestEnable: VK_FALSE,
        depthWriteEnable: VK_FALSE,
        depthCompareOp: VK_COMPARE_OP_LESS,
        depthBoundsTestEnable: VK_FALSE,
        stencilTestEnable: VK_FALSE,
        front: VkStencilOpState {
            failOp: VK_STENCIL_OP_KEEP,
            passOp: VK_STENCIL_OP_KEEP,
            depthFailOp: VK_STENCIL_OP_KEEP,
            compareOp: VK_COMPARE_OP_NEVER,
            compareMask: 0,
            writeMask: 0,
            reference: 0
        },
        back: VkStencilOpState {
            failOp: VK_STENCIL_OP_KEEP,
            passOp: VK_STENCIL_OP_KEEP,
            depthFailOp: VK_STENCIL_OP_KEEP,
            compareOp: VK_COMPARE_OP_NEVER,
            compareMask: 0,
            writeMask: 0,
            reference: 0
        },
        minDepthBounds: 0.0,
        maxDepthBounds: 1.0
    };

    // Creation

    let pipeline_create_infos = [
        VkGraphicsPipelineCreateInfo {
            sType: VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            stageCount: model_shader_stage_info.len() as u32,
            pStages: model_shader_stage_info.as_ptr(),
            pVertexInputState: &vertex_input_state,
            pInputAssemblyState: &input_assembly_state,
            pTessellationState: core::ptr::null(),
            pViewportState: &viewport_state,
            pRasterizationState: &rasterization_state,
            pMultisampleState: &multisample_state,
            pDepthStencilState: &model_depth_stencil_state,
            pColorBlendState: &color_blend_state,
            pDynamicState: &dynamic_state,
            layout: pipeline_layout,
            renderPass: render_pass,
            subpass: 0,
            basePipelineHandle: core::ptr::null_mut(),
            basePipelineIndex: -1
        },
        VkGraphicsPipelineCreateInfo {
            sType: VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            stageCount: skydome_shader_stage_info.len() as u32,
            pStages: skydome_shader_stage_info.as_ptr(),
            pVertexInputState: &vertex_input_state,
            pInputAssemblyState: &input_assembly_state,
            pTessellationState: core::ptr::null(),
            pViewportState: &viewport_state,
            pRasterizationState: &rasterization_state,
            pMultisampleState: &multisample_state,
            pDepthStencilState: &skydome_depth_stencil_state,
            pColorBlendState: &color_blend_state,
            pDynamicState: &dynamic_state,
            layout: pipeline_layout,
            renderPass: render_pass,
            subpass: 0,
            basePipelineHandle: core::ptr::null_mut(),
            basePipelineIndex: -1
        }
    ];

    println!("Creating graphics pipelines.");
    let mut graphics_pipelines = [std::ptr::null_mut(); 2];
    let result = unsafe
    {
        vkCreateGraphicsPipelines(
            device,
            core::ptr::null_mut(),
            pipeline_create_infos.len() as u32,
            pipeline_create_infos.as_ptr(),
            core::ptr::null_mut(),
            graphics_pipelines.as_mut_ptr()
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create graphics pipelines. Error: {}", result);
    }

    let model_pipeline = graphics_pipelines[0];
    let skydome_pipeline = graphics_pipelines[1];

    // Compute pipelines

    let compute_pipeline_create_infos = [
        VkComputePipelineCreateInfo {
            sType: VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
            pNext: std::ptr::null(),
            flags: 0x0,
            stage: VkPipelineShaderStageCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0x0,
                pSpecializationInfo: std::ptr::null(),
                stage: VK_SHADER_STAGE_COMPUTE_BIT,
                module: env_preinteg_shader_module,
                pName: b"main\0".as_ptr() as *const i8
            },
            layout: env_compute_pipeline_layout,
            basePipelineHandle: std::ptr::null_mut(),
            basePipelineIndex: -1
        },
        VkComputePipelineCreateInfo {
            sType: VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
            pNext: std::ptr::null(),
            flags: 0x0,
            stage: VkPipelineShaderStageCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0x0,
                pSpecializationInfo: std::ptr::null(),
                stage: VK_SHADER_STAGE_COMPUTE_BIT,
                module: dfg_preinteg_shader_module,
                pName: b"main\0".as_ptr() as *const i8
            },
            layout: dfg_compute_pipeline_layout,
            basePipelineHandle: std::ptr::null_mut(),
            basePipelineIndex: -1
        },
        VkComputePipelineCreateInfo {
            sType: VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
            pNext: std::ptr::null(),
            flags: 0x0,
            stage: VkPipelineShaderStageCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0x0,
                pSpecializationInfo: std::ptr::null(),
                stage: VK_SHADER_STAGE_COMPUTE_BIT,
                module: avg_luminance_shader_module,
                pName: b"main\0".as_ptr() as *const i8
            },
            layout: avg_luminance_pipeline_layout,
            basePipelineHandle: std::ptr::null_mut(),
            basePipelineIndex: -1
        },
        VkComputePipelineCreateInfo {
            sType: VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
            pNext: std::ptr::null(),
            flags: 0x0,
            stage: VkPipelineShaderStageCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0x0,
                pSpecializationInfo: std::ptr::null(),
                stage: VK_SHADER_STAGE_COMPUTE_BIT,
                module: postprocessing_shader_module,
                pName: b"main\0".as_ptr() as *const i8
            },
            layout: postprocessing_pipeline_layout,
            basePipelineHandle: std::ptr::null_mut(),
            basePipelineIndex: -1
        }
    ];

    println!("Creating compute pipelines.");
    let mut compute_pipelines = [std::ptr::null_mut(); 4];
    let result = unsafe
    {
        vkCreateComputePipelines(
            device,
            std::ptr::null_mut(),
            compute_pipeline_create_infos.len() as u32,
            compute_pipeline_create_infos.as_ptr(),
            std::ptr::null_mut(),
            compute_pipelines.as_mut_ptr()
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create compute pipelines. Error: {}.", result);
    }

    let env_compute_pipeline = compute_pipelines[0];
    let dfg_compute_pipeline = compute_pipelines[1];
    let avg_luminance_pipeline = compute_pipelines[2];
    let postprocessing_pipeline = compute_pipelines[3];

    //
    // Vertex and Index data
    //

    let tri_and_quad_vertices = create_tri_and_quad_vertices_with_normals();
    let tri_and_quad_indices = create_tri_and_quad_indices();

    let cube_vertices = create_cube_vertices_with_normals();
    let cube_indices = create_cube_indices();

    let sphere_vertices = create_sphere_vertices_with_normals(16, 16);
    let sphere_indices = create_sphere_indices(16, 16);

    // Vertex and Index buffer size

    let tri_and_quad_vertex_data_size = tri_and_quad_vertices.len() * core::mem::size_of::<f32>();
    let tri_and_quad_index_data_size = tri_and_quad_indices.len() * core::mem::size_of::<u32>();

    let cube_vertex_data_size = cube_vertices.len() * core::mem::size_of::<f32>();
    let cube_index_data_size = cube_indices.len() * core::mem::size_of::<u32>();

    let sphere_vertex_data_size = sphere_vertices.len() * core::mem::size_of::<f32>();
    let sphere_index_data_size = sphere_indices.len() * core::mem::size_of::<u32>();

    let vertex_data_size = tri_and_quad_vertex_data_size + cube_vertex_data_size + sphere_vertex_data_size;
    let index_data_size = tri_and_quad_index_data_size + cube_index_data_size + sphere_index_data_size;

    let vertex_buf_mem_props = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT as VkMemoryPropertyFlags;
    let index_buf_mem_props = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT as VkMemoryPropertyFlags;

    //
    // Image data
    //

    let image_format = VK_FORMAT_R8G8B8A8_SRGB;
    let bytes_per_pixel = 4 * core::mem::size_of::<u8>();
    let image_texel_block_size = 4;

    let image_mem_props = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT as VkMemoryPropertyFlags;

    let dfg_img_width = 128;
    let dfg_img_height = 128;
    let dfg_image_format = VK_FORMAT_R8G8_UNORM;

    struct ImageData
    {
        bytes_per_pixel: usize,
        width: usize,
        height: usize,
        data: Vec<u8>
    }

    impl ImageData
    {
        fn get_data_size(&self) -> usize
        {
            self.width * self.height * self.bytes_per_pixel
        }

        fn get_pixel(&mut self, x: usize, y: usize) -> &mut [u8]
        {
            let begin = (y * self.width + x) * self.bytes_per_pixel;
            let end = (y * self.width + x + 1) * self.bytes_per_pixel;

            &mut self.data[begin..end]
        }
    }

    let mut image_data_array = [
        ImageData {
            bytes_per_pixel: bytes_per_pixel,
            width: 4,
            height: 4,
            data: Vec::new()
        },
        ImageData {
            bytes_per_pixel: bytes_per_pixel,
            width: 8,
            height: 8,
            data: Vec::new()
        }
    ];

    // Pixel data

    image_data_array[0].data = vec![0xFF; image_data_array[0].get_data_size()];
    for i in 0..image_data_array[0].height
    {
        for j in 0..image_data_array[0].width
        {
            let pixel_data = image_data_array[0].get_pixel(i, j);

            pixel_data[0] = if i % 2 == 0 { 0xA0 } else { 0xFF };
            pixel_data[1] = if j % 2 == 0 { 0xA0 } else { 0xFF };
            pixel_data[2] = 0xA0;
            pixel_data[3] = 0xFF;
        }
    }

    image_data_array[1].data = vec![0xFF; image_data_array[1].get_data_size()];
    for i in 0..image_data_array[1].height
    {
        for j in 0..image_data_array[1].width
        {
            let pixel_data = image_data_array[1].get_pixel(i, j);

            pixel_data[0] = if i % 2 == 0 { 0xC0 } else { 0xFF };
            pixel_data[1] = if j % 2 == 0 { 0xC0 } else { 0xFF };
            pixel_data[2] = 0xFF;
            pixel_data[3] = 0xFF;
        }
    }

    //
    // Cube data
    //

    let cube_image_format = VK_FORMAT_R32G32B32A32_SFLOAT;
    let cube_img_component_count = 4;
    let cube_img_bytes_per_pixel = cube_img_component_count * std::mem::size_of::<f32>();
    let cube_image_texel_block_size = 16;

    let env_img_width = 128;
    let env_img_height = 128;

    let skydome_img_width = 512;
    let skydome_img_height = 512;

    // Pixel data

    let neg_z_slice_id = 5;
    let pos_z_slice_id = 4;
    let neg_y_slice_id = 3;
    let pos_y_slice_id = 2;
    let neg_x_slice_id = 1;
    let pos_x_slice_id = 0;

    let skydome_img_slice_float_array_size = skydome_img_width * skydome_img_height * cube_img_component_count;

    // Negative Z
    let mut skydome_image_data_neg_z: Vec<f32> = vec![0.0; skydome_img_slice_float_array_size];
    for i in 0..skydome_img_height
    {
        for j in 0..skydome_img_width
        {
            let skydome_coord_x = (skydome_img_height - j) as f32 / skydome_img_height as f32;
            let skydome_coord_y = (skydome_img_width - i) as f32 / skydome_img_width as f32;
            let skydome_coord_z = 0.0;

            let pixel_data_begin = (i * skydome_img_width + j) * cube_img_component_count;
            let pixel_data_end = (i * skydome_img_width + j + 1) * cube_img_component_count;
            let pixel_data = &mut skydome_image_data_neg_z[pixel_data_begin..pixel_data_end];

            let result = create_skydome(skydome_coord_x, skydome_coord_y, skydome_coord_z);

            pixel_data[0] = result[0];
            pixel_data[1] = result[1];
            pixel_data[2] = result[2];
        }
    }

    // Positive Z
    let mut skydome_image_data_pos_z: Vec<f32> = vec![0.0; skydome_img_slice_float_array_size];
    for i in 0..skydome_img_height
    {
        for j in 0..skydome_img_width
        {
            let skydome_coord_x = (j + 1) as f32 / skydome_img_width as f32;
            let skydome_coord_y = (skydome_img_height - i) as f32 / skydome_img_height as f32;
            let skydome_coord_z = 1.0;

            let pixel_data_begin = (i * skydome_img_width + j) * cube_img_component_count;
            let pixel_data_end = (i * skydome_img_width + j + 1) * cube_img_component_count;
            let pixel_data = &mut skydome_image_data_pos_z[pixel_data_begin..pixel_data_end];

            let result = create_skydome(skydome_coord_x, skydome_coord_y, skydome_coord_z);

            pixel_data[0] = result[0];
            pixel_data[1] = result[1];
            pixel_data[2] = result[2];
        }
    }

    // Negative Y
    let mut skydome_image_data_neg_y: Vec<f32> = vec![0.0; skydome_img_slice_float_array_size];
    for i in 0..skydome_img_height
    {
        for j in 0..skydome_img_width
        {
            let skydome_coord_x = (j + 1) as f32 / skydome_img_width as f32;
            let skydome_coord_y = 0.0;
            let skydome_coord_z = (skydome_img_height - i) as f32 / skydome_img_height as f32;

            let pixel_data_begin = (i * skydome_img_width + j) * cube_img_component_count;
            let pixel_data_end = (i * skydome_img_width + j + 1) * cube_img_component_count;
            let pixel_data = &mut skydome_image_data_neg_y[pixel_data_begin..pixel_data_end];

            let result = create_skydome(skydome_coord_x, skydome_coord_y, skydome_coord_z);

            pixel_data[0] = result[0];
            pixel_data[1] = result[1];
            pixel_data[2] = result[2];
        }
    }

    // Positive Y
    let mut skydome_image_data_pos_y: Vec<f32> = vec![0.0; skydome_img_slice_float_array_size];
    for i in 0..skydome_img_height
    {
        for j in 0..skydome_img_width
        {
            let skydome_coord_x = (j + 1) as f32 / skydome_img_width as f32;
            let skydome_coord_y = 1.0;
            let skydome_coord_z = (i + 1) as f32 / skydome_img_height as f32;

            let pixel_data_begin = (i * skydome_img_width + j) * cube_img_component_count;
            let pixel_data_end = (i * skydome_img_width + j + 1) * cube_img_component_count;
            let pixel_data = &mut skydome_image_data_pos_y[pixel_data_begin..pixel_data_end];

            let result = create_skydome(skydome_coord_x, skydome_coord_y, skydome_coord_z);

            pixel_data[0] = result[0];
            pixel_data[1] = result[1];
            pixel_data[2] = result[2];
        }
    }

    // Negative X
    let mut skydome_image_data_neg_x: Vec<f32> = vec![0.0; skydome_img_slice_float_array_size];
    for i in 0..skydome_img_height
    {
        for j in 0..skydome_img_width
        {
            let skydome_coord_x = 0.0;
            let skydome_coord_y = (skydome_img_height - i) as f32 / skydome_img_height as f32;
            let skydome_coord_z = (j + 1) as f32 / skydome_img_width as f32;

            let pixel_data_begin = (i * skydome_img_width + j) * cube_img_component_count;
            let pixel_data_end = (i * skydome_img_width + j + 1) * cube_img_component_count;
            let pixel_data = &mut skydome_image_data_neg_x[pixel_data_begin..pixel_data_end];

            let result = create_skydome(skydome_coord_x, skydome_coord_y, skydome_coord_z);

            pixel_data[0] = result[0];
            pixel_data[1] = result[1];
            pixel_data[2] = result[2];
        }
    }

    // Positive X
    let mut skydome_image_data_pos_x: Vec<f32> = vec![0.0; skydome_img_slice_float_array_size];
    for i in 0..skydome_img_height
    {
        for j in 0..skydome_img_width
        {
            let skydome_coord_x = 1.0;
            let skydome_coord_y = (skydome_img_height - i) as f32 / skydome_img_height as f32;
            let skydome_coord_z = (skydome_img_width - j) as f32 / skydome_img_width as f32;

            let pixel_data_begin = (i * skydome_img_width + j) * cube_img_component_count;
            let pixel_data_end = (i * skydome_img_width + j + 1) * cube_img_component_count;
            let pixel_data = &mut skydome_image_data_pos_x[pixel_data_begin..pixel_data_end];

            let result = create_skydome(skydome_coord_x, skydome_coord_y, skydome_coord_z);

            pixel_data[0] = result[0];
            pixel_data[1] = result[1];
            pixel_data[2] = result[2];
        }
    }

    //
    // Staging buffer size
    //

    let geometry_data_end = vertex_data_size + index_data_size;

    // Padding image offset to image texel block size

    let image_align_remainder = geometry_data_end % image_texel_block_size;
    let image_padding = if image_align_remainder == 0 {0} else {image_texel_block_size - image_align_remainder};

    let image_data_offset = geometry_data_end + image_padding;
    let image_data_total_size = image_data_array.iter().fold(0, |sum, image| {sum + image.get_data_size()});

    let image_data_end = image_data_offset + image_data_total_size;

    // Padding image offset to image texel block size

    let skydome_image_align_remainder = image_data_end % cube_image_texel_block_size;
    let skydome_image_padding = if skydome_image_align_remainder == 0 {0} else {cube_image_texel_block_size - skydome_image_align_remainder};

    let skydome_image_data_offset = image_data_end + skydome_image_padding;
    let skydome_single_image_data_size = skydome_img_width * skydome_img_height * cube_img_bytes_per_pixel;
    let skydome_image_data_size = 6 * skydome_single_image_data_size;

    let staging_buffer_size = skydome_image_data_offset + skydome_image_data_size;

    let staging_buf_mem_props = (VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) as VkMemoryPropertyFlags;

    //
    // Uniform data
    //

    #[repr(C, align(16))]
    #[derive(Copy, Clone)]
    struct VsCameraData
    {
        projection_matrix: [f32; 16],
        view_matrix: [f32; 16]
    }

    #[repr(C, align(16))]
    #[derive(Copy, Clone)]
    struct TransformData
    {
        model_matrix: [f32; 16]
    }

    #[repr(C, align(16))]
    #[derive(Copy, Clone)]
    struct FsCameraData
    {
        camera_x: f32,
        camera_y: f32,
        camera_z: f32,
        std140_padding_0: f32
    }

    #[repr(C, align(16))]
    #[derive(Copy, Clone)]
    struct MaterialData
    {
        albedo: [f32; 4],
        roughness: f32,
        metalness: f32,
        reflectiveness: f32,
        std140_padding_0: f32,
        emissive: [f32; 4]
    }

    #[repr(C, align(16))]
    #[derive(Copy, Clone)]
    struct LightCountData
    {
        light_count: u32,
        std140_padding_0: f32,
        std140_padding_1: f32,
        std140_padding_2: f32,
    }

    #[repr(C, align(16))]
    #[derive(Copy, Clone)]
    struct LightData
    {
        position: [f32; 4],
        intensity: [f32; 4]
    }

    let min_ubo_offset_alignment = phys_device_properties.limits.minUniformBufferOffsetAlignment as usize;

    let max_object_count = 64;

    // Per frame UBO transform region size

    let transform_data_size = core::mem::size_of::<VsCameraData>() + max_object_count * core::mem::size_of::<TransformData>();

    let transform_data_align_rem = transform_data_size % min_ubo_offset_alignment;
    let transform_data_padding;
    if transform_data_align_rem != 0
    {
        transform_data_padding = min_ubo_offset_alignment - transform_data_align_rem;
    }
    else
    {
        transform_data_padding = 0;
    }

    let padded_transform_data_size = transform_data_size + transform_data_padding;

    // Per frame UBO material region size

    let material_data_size = core::mem::size_of::<FsCameraData>() + max_object_count * core::mem::size_of::<MaterialData>();

    let material_data_align_rem = material_data_size % min_ubo_offset_alignment;
    let material_data_padding;
    if material_data_align_rem != 0
    {
        material_data_padding = min_ubo_offset_alignment - material_data_align_rem;
    }
    else
    {
        material_data_padding = 0;
    };

    let padded_material_data_size = material_data_size + material_data_padding;

    let max_light_count = 64;

    // Per frame UBO light region size

    let light_data_size = core::mem::size_of::<LightCountData>() + max_light_count * core::mem::size_of::<LightData>();

    let light_data_align_rem = light_data_size % min_ubo_offset_alignment;
    let light_data_padding;
    if light_data_align_rem != 0
    {
        light_data_padding = min_ubo_offset_alignment - light_data_align_rem
    }
    else
    {
        light_data_padding = 0;
    }

    let padded_light_data_size = light_data_size + light_data_padding;

    // Total UBO size

    let total_transform_data_size = frame_count * padded_transform_data_size;
    let total_material_data_size = frame_count * padded_material_data_size;
    let total_light_data_size = frame_count * padded_light_data_size;
    let uniform_buffer_size = total_transform_data_size + total_material_data_size + total_light_data_size;

    let uniform_buf_mem_props = (VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) as VkMemoryPropertyFlags;

    //
    // Vertex buffer
    //

    // Create buffer

    let vertex_buffer_create_info = VkBufferCreateInfo {
        sType: VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        size: vertex_data_size as VkDeviceSize,
        usage: (VK_BUFFER_USAGE_VERTEX_BUFFER_BIT |
                VK_BUFFER_USAGE_TRANSFER_DST_BIT) as VkBufferUsageFlags,
        sharingMode: VK_SHARING_MODE_EXCLUSIVE,
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: core::ptr::null()
    };

    println!("Creating vertex buffer.");
    let mut vertex_buffer = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateBuffer(
            device,
            &vertex_buffer_create_info,
            core::ptr::null(),
            &mut vertex_buffer
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create vertex buffer. Error: {}.", result);
    }

    // Create memory

    let mut mem_requirements = VkMemoryRequirements::default();
    unsafe
    {
        vkGetBufferMemoryRequirements(
            device,
            vertex_buffer,
            &mut mem_requirements
        );
    }

    let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
    for i in 0..phys_device_mem_properties.memoryTypeCount
    {
        if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
            (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & vertex_buf_mem_props) == vertex_buf_mem_props
        {
            chosen_memory_type = i;
            break;
        }
    }

    if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
    {
        panic!("Could not find memory type.");
    }

    let vertex_buffer_alloc_info = VkMemoryAllocateInfo {
        sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: core::ptr::null(),
        allocationSize: mem_requirements.size,
        memoryTypeIndex: chosen_memory_type
    };

    println!("Vertex buffer size: {}", mem_requirements.size);
    println!("Vertex buffer align: {}", mem_requirements.alignment);

    println!("Allocating vertex buffer memory.");
    let mut vertex_buffer_memory = core::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateMemory(
            device,
            &vertex_buffer_alloc_info,
            core::ptr::null(),
            &mut vertex_buffer_memory
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Could not allocate memory. Error: {}.", result);
    }

    // Bind buffer to memory

    println!("Binding vertex buffer memory.");
    let result = unsafe
    {
        vkBindBufferMemory(
            device,
            vertex_buffer,
            vertex_buffer_memory,
            0
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to bind memory to vertex buffer. Error: {}.", result);
    }

    //
    // Index buffer
    //

    // Create buffer

    let index_buffer_create_info = VkBufferCreateInfo {
        sType: VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        size: index_data_size as VkDeviceSize,
        usage: (VK_BUFFER_USAGE_INDEX_BUFFER_BIT |
                VK_BUFFER_USAGE_TRANSFER_DST_BIT) as VkBufferUsageFlags,
        sharingMode: VK_SHARING_MODE_EXCLUSIVE,
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: core::ptr::null()
    };

    println!("Creating index buffer.");
    let mut index_buffer = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateBuffer(
            device,
            &index_buffer_create_info,
            core::ptr::null(),
            &mut index_buffer
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create index buffer. Error: {}.", result);
    }

    // Create memory

    let mut mem_requirements = VkMemoryRequirements::default();
    unsafe
    {
        vkGetBufferMemoryRequirements(
            device,
            index_buffer,
            &mut mem_requirements
        );
    }

    let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
    for i in 0..phys_device_mem_properties.memoryTypeCount
    {
        if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
            (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & index_buf_mem_props) == index_buf_mem_props
        {
            chosen_memory_type = i;
            break;
        }
    }

    if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
    {
        panic!("Could not find memory type.");
    }

    let index_buffer_alloc_info = VkMemoryAllocateInfo {
        sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: core::ptr::null(),
        allocationSize: mem_requirements.size,
        memoryTypeIndex: chosen_memory_type
    };

    println!("Index buffer size: {}", mem_requirements.size);
    println!("Index buffer align: {}", mem_requirements.alignment);

    println!("Allocating index buffer memory.");
    let mut index_buffer_memory = core::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateMemory(
            device,
            &index_buffer_alloc_info,
            core::ptr::null(),
            &mut index_buffer_memory
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Could not allocate memory. Error: {}.", result);
    }

    // Bind buffer to memory

    println!("Binding index buffer memory.");
    let result = unsafe
    {
        vkBindBufferMemory(
            device,
            index_buffer,
            index_buffer_memory,
            0
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to bind memory to index buffer. Error: {}.", result);
    }

    //
    // Textures
    //

    let mut format_properties = VkFormatProperties::default();
    unsafe
    {
        vkGetPhysicalDeviceFormatProperties(
            chosen_phys_device,
            image_format,
            &mut format_properties
        );
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R8G8B8A8_SRGB with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT.");
    }

    let mut images = Vec::with_capacity(image_data_array.len());
    let mut image_memories = Vec::with_capacity(image_data_array.len());
    let mut image_views = Vec::with_capacity(image_data_array.len());

    for image_data in image_data_array.iter()
    {
        //
        // Image
        //

        // Create image

        let image_create_info = VkImageCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            imageType: VK_IMAGE_TYPE_2D,
            format: image_format,
            extent: VkExtent3D {
                width: image_data.width as u32,
                height: image_data.height as u32,
                depth: 1
            },
            mipLevels: 1,
            arrayLayers: 1,
            samples: VK_SAMPLE_COUNT_1_BIT,
            tiling: VK_IMAGE_TILING_OPTIMAL,
            usage: (VK_IMAGE_USAGE_SAMPLED_BIT |
                    VK_IMAGE_USAGE_TRANSFER_DST_BIT) as VkImageUsageFlags,
            sharingMode: VK_SHARING_MODE_EXCLUSIVE,
            queueFamilyIndexCount: 0,
            pQueueFamilyIndices: core::ptr::null(),
            initialLayout: VK_IMAGE_LAYOUT_UNDEFINED
        };

        println!("Creating image.");
        let mut image = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImage(
                device,
                &image_create_info,
                core::ptr::null_mut(),
                &mut image
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create image. Error: {}", result);
        }

        images.push(image);

        // Create memory

        let mut mem_requirements = VkMemoryRequirements::default();
        unsafe
        {
            vkGetImageMemoryRequirements(
                device,
                image,
                &mut mem_requirements
            );
        }

        let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
        for i in 0..phys_device_mem_properties.memoryTypeCount
        {
            if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
                (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & image_mem_props) == image_mem_props
            {
                chosen_memory_type = i;
                break;
            }
        }

        if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
        {
            panic!("Could not find memory type.");
        }

        let image_alloc_info = VkMemoryAllocateInfo {
            sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            allocationSize: mem_requirements.size,
            memoryTypeIndex: chosen_memory_type
        };

        println!("Image size: {}", mem_requirements.size);
        println!("Image align: {}", mem_requirements.alignment);

        println!("Allocating image memory");
        let mut image_memory = core::ptr::null_mut();
        let result = unsafe
        {
            vkAllocateMemory(
                device,
                &image_alloc_info,
                core::ptr::null(),
                &mut image_memory
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Could not allocate memory. Error: {}", result);
        }

        image_memories.push(image_memory);

        // Bind image to memory

        let result = unsafe
        {
            vkBindImageMemory(
                device,
                image,
                image_memory,
                0
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to bind memory to image. Error: {}", result);
        }

        //
        // Image view
        //

        let image_view_create_info = VkImageViewCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            image: image,
            viewType: VK_IMAGE_VIEW_TYPE_2D,
            format: image_format,
            components: VkComponentMapping {
                r: VK_COMPONENT_SWIZZLE_IDENTITY,
                g: VK_COMPONENT_SWIZZLE_IDENTITY,
                b: VK_COMPONENT_SWIZZLE_IDENTITY,
                a: VK_COMPONENT_SWIZZLE_IDENTITY
            },
            subresourceRange: VkImageSubresourceRange {
                aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                baseMipLevel: 0,
                levelCount: 1,
                baseArrayLayer: 0,
                layerCount: 1
            }
        };

        println!("Creating image view.");
        let mut image_view = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImageView(
                device,
                &image_view_create_info,
                core::ptr::null_mut(),
                &mut image_view
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create image view. Error: {}", result);
        }

        image_views.push(image_view);
    }

    //
    // DFG image
    //

    let mut format_properties = VkFormatProperties::default();
    unsafe
    {
        vkGetPhysicalDeviceFormatProperties(
            chosen_phys_device,
            dfg_image_format,
            &mut format_properties
        );
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R8G8_UNORM with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT.");
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R8G8_UNORM with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT.");
    }

    let image_create_info = VkImageCreateInfo {
        sType: VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        imageType: VK_IMAGE_TYPE_2D,
        format: dfg_image_format,
        extent: VkExtent3D {
            width: dfg_img_width as u32,
            height: dfg_img_height as u32,
            depth: 1
        },
        mipLevels: 1,
        arrayLayers: 1,
        samples: VK_SAMPLE_COUNT_1_BIT,
        tiling: VK_IMAGE_TILING_OPTIMAL,
        usage: (VK_IMAGE_USAGE_SAMPLED_BIT |
                VK_IMAGE_USAGE_STORAGE_BIT) as VkImageUsageFlags,
        sharingMode: VK_SHARING_MODE_EXCLUSIVE,
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: std::ptr::null(),
        initialLayout: VK_IMAGE_LAYOUT_UNDEFINED
    };

    println!("Creating dfg image.");
    let mut dfg_image = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateImage(
            device,
            &image_create_info,
            std::ptr::null_mut(),
            &mut dfg_image
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create dfg image. Error: {}", result);
    }

    let mut mem_requirements = VkMemoryRequirements::default();
    unsafe
    {
        vkGetImageMemoryRequirements(
            device,
            dfg_image,
            &mut mem_requirements
        );
    }

    let type_filter = mem_requirements.memoryTypeBits;

    let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
    for i in 0..phys_device_mem_properties.memoryTypeCount
    {
        if type_filter & (1 << i) != 0 &&
            (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & image_mem_props) == image_mem_props
        {
            chosen_memory_type = i;
            break;
        }
    }

    if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
    {
        panic!("Could not find memory type.");
    }

    let image_alloc_info = VkMemoryAllocateInfo {
        sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: std::ptr::null(),
        allocationSize: mem_requirements.size,
        memoryTypeIndex: chosen_memory_type
    };

    println!("Dfg image size: {}", mem_requirements.size);
    println!("Dfg image align: {}", mem_requirements.alignment);

    println!("Allocating dfg image memory");
    let mut dfg_image_memory = std::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateMemory(
            device,
            &image_alloc_info,
            std::ptr::null(),
            &mut dfg_image_memory
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Could not allocate memory. Error: {}", result);
    }

    let result = unsafe
    {
        vkBindImageMemory(
            device,
            dfg_image,
            dfg_image_memory,
            0
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to bind memory to dfg image. Error: {}", result);
    }

    //
    // DFG image view
    //

    let image_view_create_info = VkImageViewCreateInfo {
        sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        image: dfg_image,
        viewType: VK_IMAGE_VIEW_TYPE_2D,
        format: dfg_image_format,
        components: VkComponentMapping {
            r: VK_COMPONENT_SWIZZLE_IDENTITY,
            g: VK_COMPONENT_SWIZZLE_IDENTITY,
            b: VK_COMPONENT_SWIZZLE_IDENTITY,
            a: VK_COMPONENT_SWIZZLE_IDENTITY
        },
        subresourceRange: VkImageSubresourceRange {
            aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
            baseMipLevel: 0,
            levelCount: 1,
            baseArrayLayer: 0,
            layerCount: 1
        }
    };

    println!("Creating dfg image view.");
    let mut dfg_image_view = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateImageView(
            device,
            &image_view_create_info,
            std::ptr::null_mut(),
            &mut dfg_image_view
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create image view. Error: {}", result);
    }

    //
    // Skydome texture
    //

    let mut format_properties = VkFormatProperties::default();
    unsafe
    {
        vkGetPhysicalDeviceFormatProperties(
            chosen_phys_device,
            cube_image_format,
            &mut format_properties
        );
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R32G32B32A32_SFLOAT with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT.");
    }

    let image_create_info = VkImageCreateInfo {
        sType: VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: VK_IMAGE_CREATE_CUBE_COMPATIBLE_BIT as VkImageCreateFlags,
        imageType: VK_IMAGE_TYPE_2D,
        format: cube_image_format,
        extent: VkExtent3D {
            width: skydome_img_width as u32,
            height: skydome_img_height as u32,
            depth: 1
        },
        mipLevels: 1,
        arrayLayers: 6,
        samples: VK_SAMPLE_COUNT_1_BIT,
        tiling: VK_IMAGE_TILING_OPTIMAL,
        usage: (VK_IMAGE_USAGE_SAMPLED_BIT |
                VK_IMAGE_USAGE_TRANSFER_DST_BIT) as VkImageUsageFlags,
        sharingMode: VK_SHARING_MODE_EXCLUSIVE,
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: std::ptr::null(),
        initialLayout: VK_IMAGE_LAYOUT_UNDEFINED
    };

    println!("Creating skydome image.");
    let mut skydome_image = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateImage(
            device,
            &image_create_info,
            std::ptr::null_mut(),
            &mut skydome_image
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create skydome image. Error: {}", result);
    }

    let mut mem_requirements = VkMemoryRequirements::default();
    unsafe
    {
        vkGetImageMemoryRequirements(
            device,
            skydome_image,
            &mut mem_requirements
        );
    }

    let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
    for i in 0..phys_device_mem_properties.memoryTypeCount
    {
        if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
            (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & image_mem_props) == image_mem_props
        {
            chosen_memory_type = i;
            break;
        }
    }

    if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
    {
        panic!("Could not find memory type.");
    }

    let image_alloc_info = VkMemoryAllocateInfo {
        sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: std::ptr::null(),
        allocationSize: mem_requirements.size,
        memoryTypeIndex: chosen_memory_type
    };

    println!("Skydome image size: {}", mem_requirements.size);
    println!("Skydome image align: {}", mem_requirements.alignment);

    println!("Allocating skydome image memory");
    let mut skydome_image_memory = std::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateMemory(
            device,
            &image_alloc_info,
            std::ptr::null(),
            &mut skydome_image_memory
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Could not allocate memory. Error: {}", result);
    }

    let result = unsafe
    {
        vkBindImageMemory(
            device,
            skydome_image,
            skydome_image_memory,
            0
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to bind memory to skydome image. Error: {}", result);
    }

    //
    // Skydome image view
    //

    let image_view_create_info = VkImageViewCreateInfo {
        sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        image: skydome_image,
        viewType: VK_IMAGE_VIEW_TYPE_CUBE,
        format: cube_image_format,
        components: VkComponentMapping {
            r: VK_COMPONENT_SWIZZLE_IDENTITY,
            g: VK_COMPONENT_SWIZZLE_IDENTITY,
            b: VK_COMPONENT_SWIZZLE_IDENTITY,
            a: VK_COMPONENT_SWIZZLE_IDENTITY
        },
        subresourceRange: VkImageSubresourceRange {
            aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
            baseMipLevel: 0,
            levelCount: 1,
            baseArrayLayer: 0,
            layerCount: 6
        }
    };

    println!("Creating skydome image view.");
    let mut skydome_image_view = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateImageView(
            device,
            &image_view_create_info,
            std::ptr::null_mut(),
            &mut skydome_image_view
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create skydome image view. Error: {}", result);
    }

    //
    // Environment texture
    //

    let mut format_properties = VkFormatProperties::default();
    unsafe
    {
        vkGetPhysicalDeviceFormatProperties(
            chosen_phys_device,
            cube_image_format,
            &mut format_properties
        );
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R32G32B32A32_SFLOAT with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT.");
    }

    if format_properties.optimalTilingFeatures & VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT as VkFormatFeatureFlags == 0
    {
        panic!("Image format VK_FORMAT_R32G32B32A32_SFLOAT with VK_IMAGE_TILING_OPTIMAL does not support usage flags VK_FORMAT_FEATURE_STORAGE_IMAGE_BIT.");
    }

    let image_create_info = VkImageCreateInfo {
        sType: VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: VK_IMAGE_CREATE_CUBE_COMPATIBLE_BIT as VkImageCreateFlags,
        imageType: VK_IMAGE_TYPE_2D,
        format: cube_image_format,
        extent: VkExtent3D {
            width: env_img_width as u32,
            height: env_img_height as u32,
            depth: 1
        },
        mipLevels: MAX_ENV_MIP_LVL_COUNT as u32,
        arrayLayers: 6,
        samples: VK_SAMPLE_COUNT_1_BIT,
        tiling: VK_IMAGE_TILING_OPTIMAL,
        usage: (VK_IMAGE_USAGE_SAMPLED_BIT |
                VK_IMAGE_USAGE_STORAGE_BIT) as VkImageUsageFlags,
        sharingMode: VK_SHARING_MODE_EXCLUSIVE,
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: std::ptr::null(),
        initialLayout: VK_IMAGE_LAYOUT_UNDEFINED
    };

    println!("Creating environment image.");
    let mut env_image = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateImage(
            device,
            &image_create_info,
            std::ptr::null_mut(),
            &mut env_image
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create environment image. Error: {}", result);
    }

    let mut mem_requirements = VkMemoryRequirements::default();
    unsafe
    {
        vkGetImageMemoryRequirements(
            device,
            env_image,
            &mut mem_requirements
        );
    }

    let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
    for i in 0..phys_device_mem_properties.memoryTypeCount
    {
        if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
            (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & image_mem_props) == image_mem_props
        {
            chosen_memory_type = i;
            break;
        }
    }

    if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
    {
        panic!("Could not find memory type.");
    }

    let image_alloc_info = VkMemoryAllocateInfo {
        sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: std::ptr::null(),
        allocationSize: mem_requirements.size,
        memoryTypeIndex: chosen_memory_type
    };

    println!("Environment image size: {}", mem_requirements.size);
    println!("Environment image align: {}", mem_requirements.alignment);

    println!("Allocating environment image memory");
    let mut env_image_memory = std::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateMemory(
            device,
            &image_alloc_info,
            std::ptr::null(),
            &mut env_image_memory
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Could not allocate memory. Error: {}", result);
    }

    let result = unsafe
    {
        vkBindImageMemory(
            device,
            env_image,
            env_image_memory,
            0
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to bind memory to environment image. Error: {}", result);
    }

    //
    // Environment image view
    //

    // Read view

    let image_view_create_info = VkImageViewCreateInfo {
        sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        image: env_image,
        viewType: VK_IMAGE_VIEW_TYPE_CUBE,
        format: cube_image_format,
        components: VkComponentMapping {
            r: VK_COMPONENT_SWIZZLE_IDENTITY,
            g: VK_COMPONENT_SWIZZLE_IDENTITY,
            b: VK_COMPONENT_SWIZZLE_IDENTITY,
            a: VK_COMPONENT_SWIZZLE_IDENTITY
        },
        subresourceRange: VkImageSubresourceRange {
            aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
            baseMipLevel: 0,
            levelCount: MAX_ENV_MIP_LVL_COUNT as u32,
            baseArrayLayer: 0,
            layerCount: 6
        }
    };

    println!("Creating environment image view.");
    let mut env_image_view = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateImageView(
            device,
            &image_view_create_info,
            std::ptr::null_mut(),
            &mut env_image_view
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create environment image view. Error: {}", result);
    }

    // Write views

    let mut env_image_write_views = [std::ptr::null_mut(); MAX_ENV_MIP_LVL_COUNT];
    for i in 0..MAX_ENV_MIP_LVL_COUNT
    {
        let image_view_create_info = VkImageViewCreateInfo {
            sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
            pNext: std::ptr::null(),
            flags: 0x0,
            image: env_image,
            viewType: VK_IMAGE_VIEW_TYPE_CUBE,
            format: cube_image_format,
            components: VkComponentMapping {
                r: VK_COMPONENT_SWIZZLE_IDENTITY,
                g: VK_COMPONENT_SWIZZLE_IDENTITY,
                b: VK_COMPONENT_SWIZZLE_IDENTITY,
                a: VK_COMPONENT_SWIZZLE_IDENTITY
            },
            subresourceRange: VkImageSubresourceRange {
                aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                baseMipLevel: i as u32,
                levelCount: 1,
                baseArrayLayer: 0,
                layerCount: 6
            }
        };

        println!("Creating environment image view.");
        let mut env_image_write_view = std::ptr::null_mut();
        let result = unsafe
        {
            vkCreateImageView(
                device,
                &image_view_create_info,
                std::ptr::null_mut(),
                &mut env_image_write_view
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create environment image view. Error: {}", result);
        }

        env_image_write_views[i] = env_image_write_view;
    }

    //
    // Staging buffer
    //

    // Create buffer

    let staging_buffer_create_info = VkBufferCreateInfo {
        sType: VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        size: staging_buffer_size as VkDeviceSize,
        usage: VK_BUFFER_USAGE_TRANSFER_SRC_BIT as VkBufferUsageFlags,
        sharingMode: VK_SHARING_MODE_EXCLUSIVE,
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: core::ptr::null()
    };

    println!("Creating staging buffer.");
    let mut staging_buffer = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateBuffer(
            device,
            &staging_buffer_create_info,
            core::ptr::null(),
            &mut staging_buffer
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create staging buffer. Error: {}.", result);
    }

    // Create memory

    let mut mem_requirements = VkMemoryRequirements::default();
    unsafe
    {
        vkGetBufferMemoryRequirements(
            device,
            staging_buffer,
            &mut mem_requirements
        );
    }

    let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
    for i in 0..phys_device_mem_properties.memoryTypeCount
    {
        if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
            (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & staging_buf_mem_props) == staging_buf_mem_props
        {
            chosen_memory_type = i;
            break;
        }
    }

    if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
    {
        panic!("Could not find memory type.");
    }

    let staging_buffer_alloc_info = VkMemoryAllocateInfo {
        sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: core::ptr::null(),
        allocationSize: mem_requirements.size,
        memoryTypeIndex: chosen_memory_type
    };

    println!("Staging buffer size: {}", mem_requirements.size);
    println!("Staging buffer align: {}", mem_requirements.alignment);

    println!("Allocating staging buffer memory.");
    let mut staging_buffer_memory = core::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateMemory(
            device,
            &staging_buffer_alloc_info,
            core::ptr::null(),
            &mut staging_buffer_memory
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Could not allocate memory. Error: {}.", result);
    }

    // Bind buffer to memory

    println!("Binding staging buffer memory.");
    let result = unsafe
    {
        vkBindBufferMemory(
            device,
            staging_buffer,
            staging_buffer_memory,
            0
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to bind memory to staging buffer. Error: {}.", result);
    }

    //
    // Uploading to Staging buffer
    //

    let vertex_data_offset = 0;
    let index_data_offset = vertex_data_size as u64;

    unsafe
    {
        let mut data = core::ptr::null_mut();
        let result = vkMapMemory(
            device,
            staging_buffer_memory,
            0,
            staging_buffer_size as VkDeviceSize,
            0,
            &mut data
        );

        if result != VK_SUCCESS
        {
            panic!("Failed to map memory. Error: {}.", result);
        }

        //
        // Copy vertex and index data to staging buffer
        //

        // Triangle and quad vertex data

        let vertex_data_offset = vertex_data_offset as isize;
        let vertex_data_void = data.offset(vertex_data_offset);
        let vertex_data_typed: *mut f32 = core::mem::transmute(vertex_data_void);
        core::ptr::copy_nonoverlapping::<f32>(
            tri_and_quad_vertices.as_ptr(),
            vertex_data_typed,
            tri_and_quad_vertices.len()
        );

        // Cube vertex data

        let vertex_data_offset = vertex_data_offset + tri_and_quad_vertex_data_size as isize;
        let vertex_data_void = data.offset(vertex_data_offset);
        let vertex_data_typed: *mut f32 = core::mem::transmute(vertex_data_void);
        core::ptr::copy_nonoverlapping::<f32>(
            cube_vertices.as_ptr(),
            vertex_data_typed,
            cube_vertices.len()
        );

        // Sphere vertex data

        let vertex_data_offset = vertex_data_offset + cube_vertex_data_size as isize;
        let vertex_data_void = data.offset(vertex_data_offset);
        let vertex_data_typed: *mut f32 = core::mem::transmute(vertex_data_void);
        core::ptr::copy_nonoverlapping::<f32>(
            sphere_vertices.as_ptr(),
            vertex_data_typed,
            sphere_vertices.len()
        );

        // Triangle and quad index data

        let index_data_offset = index_data_offset as isize;
        let index_data_void = data.offset(index_data_offset);
        let index_data_typed: *mut u32 = core::mem::transmute(index_data_void);
        core::ptr::copy_nonoverlapping::<u32>(
            tri_and_quad_indices.as_ptr(),
            index_data_typed,
            tri_and_quad_indices.len()
        );

        // Cube index data

        let index_data_offset = index_data_offset + tri_and_quad_index_data_size as isize;
        let index_data_void = data.offset(index_data_offset);
        let index_data_typed: *mut u32 = core::mem::transmute(index_data_void);
        core::ptr::copy_nonoverlapping::<u32>(
            cube_indices.as_ptr(),
            index_data_typed,
            cube_indices.len()
        );

        // Sphere index data

        let index_data_offset = index_data_offset + cube_index_data_size as isize;
        let index_data_void = data.offset(index_data_offset);
        let index_data_typed: *mut u32 = core::mem::transmute(index_data_void);
        core::ptr::copy_nonoverlapping::<u32>(
            sphere_indices.as_ptr(),
            index_data_typed,
            sphere_indices.len()
        );

        //
        // Copy image data to staging buffer
        //

        let mut current_image_data_offset = image_data_offset;
        for image_data in image_data_array.iter()
        {
            let staging_image_data_void = data.offset(current_image_data_offset as isize);
            let staging_image_data_typed: *mut u8 = core::mem::transmute(staging_image_data_void);
            core::ptr::copy_nonoverlapping::<u8>(
                image_data.data.as_ptr(),
                staging_image_data_typed,
                image_data.data.len()
            );

            current_image_data_offset += image_data.data.len();
        }

        //
        // Copy skydome image data to staging buffer
        //

        let skydome_offset = skydome_image_data_offset as isize;
        let skydome_image_data_neg_z_dest: *mut f32 = std::mem::transmute(data.offset(skydome_offset + neg_z_slice_id * skydome_single_image_data_size as isize));
        std::ptr::copy_nonoverlapping::<f32>(
            skydome_image_data_neg_z.as_ptr(),
            skydome_image_data_neg_z_dest,
            skydome_image_data_neg_z.len()
        );

        let skydome_image_data_pos_z_dest: *mut f32 = std::mem::transmute(data.offset(skydome_offset + pos_z_slice_id * skydome_single_image_data_size as isize));
        std::ptr::copy_nonoverlapping::<f32>(
            skydome_image_data_pos_z.as_ptr(),
            skydome_image_data_pos_z_dest,
            skydome_image_data_pos_z.len()
        );

        let skydome_image_data_neg_y_dest: *mut f32 = std::mem::transmute(data.offset(skydome_offset + neg_y_slice_id * skydome_single_image_data_size as isize));
        std::ptr::copy_nonoverlapping::<f32>(
            skydome_image_data_neg_y.as_ptr(),
            skydome_image_data_neg_y_dest,
            skydome_image_data_neg_y.len()
        );

        let skydome_image_data_pos_y_dest: *mut f32 = std::mem::transmute(data.offset(skydome_offset + pos_y_slice_id * skydome_single_image_data_size as isize));
        std::ptr::copy_nonoverlapping::<f32>(
            skydome_image_data_pos_y.as_ptr(),
            skydome_image_data_pos_y_dest,
            skydome_image_data_pos_y.len()
        );

        let skydome_image_data_neg_x_dest: *mut f32 = std::mem::transmute(data.offset(skydome_offset + neg_x_slice_id * skydome_single_image_data_size as isize));
        std::ptr::copy_nonoverlapping::<f32>(
            skydome_image_data_neg_x.as_ptr(),
            skydome_image_data_neg_x_dest,
            skydome_image_data_neg_x.len()
        );

        let skydome_image_data_pos_x_dest: *mut f32 = std::mem::transmute(data.offset(skydome_offset + pos_x_slice_id * skydome_single_image_data_size as isize));
        std::ptr::copy_nonoverlapping::<f32>(
            skydome_image_data_pos_x.as_ptr(),
            skydome_image_data_pos_x_dest,
            skydome_image_data_pos_x.len()
        );

        vkUnmapMemory(
            device,
            staging_buffer_memory
        );
    }

    //
    // Memory transfer
    //

    {
        let cmd_pool_create_info = VkCommandPoolCreateInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            queueFamilyIndex: chosen_graphics_queue_family
        };

        println!("Creating transfer command pool.");
        let mut cmd_pool = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateCommandPool(
                device,
                &cmd_pool_create_info,
                core::ptr::null_mut(),
                &mut cmd_pool
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create transfer command pool. Error: {}.", result);
        }

        println!("Allocating transfer command buffers.");
        let cmd_buffer_alloc_info = VkCommandBufferAllocateInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            commandPool: cmd_pool,
            level: VK_COMMAND_BUFFER_LEVEL_PRIMARY,
            commandBufferCount: 1
        };

        let mut transfer_cmd_buffer = core::ptr::null_mut();
        let result = unsafe
        {
            vkAllocateCommandBuffers(
                device,
                &cmd_buffer_alloc_info,
                &mut transfer_cmd_buffer
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create transfer command buffer. Error: {}.", result);
        }

        let cmd_buffer_begin_info = VkCommandBufferBeginInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
            pNext: core::ptr::null(),
            flags: VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT as VkCommandBufferUsageFlags,
            pInheritanceInfo: core::ptr::null()
        };

        let result = unsafe
        {
            vkBeginCommandBuffer(
                transfer_cmd_buffer,
                &cmd_buffer_begin_info
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to start recording the comand buffer. Error: {}.", result);
        }

        //
        // Copy vertex and index data from staging buffer to device local buffer
        //

        let copy_region = [
            VkBufferCopy {
                srcOffset: vertex_data_offset,
                dstOffset: 0,
                size: vertex_data_size as u64
            }
        ];
        unsafe
        {
            vkCmdCopyBuffer(
                transfer_cmd_buffer,
                staging_buffer,
                vertex_buffer,
                copy_region.len() as u32,
                copy_region.as_ptr()
            );
        }

        let copy_region = [
            VkBufferCopy {
                srcOffset: index_data_offset,
                dstOffset: 0,
                size: index_data_size as u64
        }
        ];
        unsafe
        {
            vkCmdCopyBuffer(
                transfer_cmd_buffer,
                staging_buffer,
                index_buffer,
                copy_region.len() as u32,
                copy_region.as_ptr()
            );
        }

        //
        // Copy image data from staging buffer to image
        //

        let mut transfer_dst_barriers = Vec::with_capacity(images.len() + 1);
        for image in images.iter()
        {
            transfer_dst_barriers.push(
                VkImageMemoryBarrier {
                    sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                    pNext: core::ptr::null(),
                    srcAccessMask: VK_ACCESS_HOST_WRITE_BIT as VkAccessFlags,
                    dstAccessMask: VK_ACCESS_TRANSFER_WRITE_BIT as VkAccessFlags,
                    oldLayout: VK_IMAGE_LAYOUT_UNDEFINED,
                    newLayout: VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                    srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                    dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                    image: *image,
                    subresourceRange: VkImageSubresourceRange {
                        aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                        baseMipLevel: 0,
                        levelCount: 1,
                        baseArrayLayer: 0,
                        layerCount: 1
                    }
                }
            );
        }

        transfer_dst_barriers.push(
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: VK_ACCESS_HOST_WRITE_BIT as VkAccessFlags,
                dstAccessMask: VK_ACCESS_TRANSFER_WRITE_BIT as VkAccessFlags,
                oldLayout: VK_IMAGE_LAYOUT_UNDEFINED,
                newLayout: VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: skydome_image,
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: 1,
                    baseArrayLayer: 0,
                    layerCount: 6
                }
            }
        );

        unsafe
        {
            vkCmdPipelineBarrier(
                transfer_cmd_buffer,
                VK_PIPELINE_STAGE_HOST_BIT as VkPipelineStageFlags,
                VK_PIPELINE_STAGE_TRANSFER_BIT as VkPipelineStageFlags,
                0,
                0,
                core::ptr::null(),
                0,
                core::ptr::null(),
                transfer_dst_barriers.len() as u32,
                transfer_dst_barriers.as_ptr()
            );
        }

        let mut current_image_data_offset = image_data_offset;
        for (image_data, image) in image_data_array.iter().zip(images.iter())
        {
            let copy_region = [
                VkBufferImageCopy {
                    bufferOffset: current_image_data_offset as VkDeviceSize,
                    bufferRowLength: 0,
                    bufferImageHeight: 0,
                    imageSubresource: VkImageSubresourceLayers {
                        aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                        mipLevel: 0,
                        baseArrayLayer: 0,
                        layerCount: 1
                    },
                    imageOffset: VkOffset3D {
                        x: 0,
                        y: 0,
                        z: 0
                    },
                    imageExtent: VkExtent3D {
                        width: image_data.width as u32,
                        height: image_data.height as u32,
                        depth: 1
                    }
                }
            ];

            unsafe
            {
                vkCmdCopyBufferToImage(
                    transfer_cmd_buffer,
                    staging_buffer,
                    *image,
                    VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                    copy_region.len() as u32,
                    copy_region.as_ptr()
                );
            }

            current_image_data_offset += image_data.data.len();
        }

        let copy_region = [
            VkBufferImageCopy {
                bufferOffset: skydome_image_data_offset as VkDeviceSize,
                bufferRowLength: 0,
                bufferImageHeight: 0,
                imageSubresource: VkImageSubresourceLayers {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    mipLevel: 0,
                    baseArrayLayer: 0,
                    layerCount: 6
                },
                imageOffset: VkOffset3D {
                    x: 0,
                    y: 0,
                    z: 0
                },
                imageExtent: VkExtent3D {
                    width: skydome_img_width as u32,
                    height: skydome_img_height as u32,
                    depth: 1
                }
            }
        ];

        unsafe
        {
            vkCmdCopyBufferToImage(
                transfer_cmd_buffer,
                staging_buffer,
                skydome_image,
                VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                copy_region.len() as u32,
                copy_region.as_ptr()
            );
        }

        let mut sampler_src_barriers = Vec::with_capacity(images.len() + 1);
        for image in images.iter()
        {
            sampler_src_barriers.push(
                VkImageMemoryBarrier {
                    sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                    pNext: core::ptr::null(),
                    srcAccessMask: VK_ACCESS_TRANSFER_WRITE_BIT as VkAccessFlags,
                    dstAccessMask: VK_ACCESS_SHADER_READ_BIT as VkAccessFlags,
                    oldLayout: VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                    newLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
                    srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                    dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                    image: *image,
                    subresourceRange: VkImageSubresourceRange {
                        aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                        baseMipLevel: 0,
                        levelCount: 1,
                        baseArrayLayer: 0,
                        layerCount: 1
                    }
                }
            );
        }

        sampler_src_barriers.push(
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: VK_ACCESS_TRANSFER_WRITE_BIT as VkAccessFlags,
                dstAccessMask: VK_ACCESS_SHADER_READ_BIT as VkAccessFlags,
                oldLayout: VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                newLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: skydome_image,
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: 1,
                    baseArrayLayer: 0,
                    layerCount: 6
                }
            }
        );

        unsafe
        {
            vkCmdPipelineBarrier(
                transfer_cmd_buffer,
                VK_PIPELINE_STAGE_TRANSFER_BIT as VkPipelineStageFlags,
                VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT as VkPipelineStageFlags,
                0,
                0,
                core::ptr::null(),
                0,
                core::ptr::null(),
                sampler_src_barriers.len() as u32,
                sampler_src_barriers.as_ptr()
            );
        }

        let result = unsafe
        {
            vkEndCommandBuffer(
                transfer_cmd_buffer
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to end recording the comand buffer. Error: {}.", result);
        }

        let cmd_buffer = [transfer_cmd_buffer];

        let submit_info = VkSubmitInfo {
            sType: VK_STRUCTURE_TYPE_SUBMIT_INFO,
            pNext: core::ptr::null(),
            waitSemaphoreCount: 0,
            pWaitSemaphores: core::ptr::null(),
            pWaitDstStageMask: core::ptr::null(),
            commandBufferCount: cmd_buffer.len() as u32,
            pCommandBuffers: cmd_buffer.as_ptr(),
            signalSemaphoreCount: 0,
            pSignalSemaphores: core::ptr::null()
        };

        let result = unsafe
        {
            vkQueueSubmit(
                graphics_queue,
                1,
                &submit_info,
                core::ptr::null_mut()
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to submit transfer commands: {:?}.", result);
        }

        //
        // Cleanup
        //

        let _result = unsafe
        {
            vkQueueWaitIdle(
                graphics_queue
            )
        };

        println!("Deleting transfer command pool.");
        unsafe
        {
            vkDestroyCommandPool(
                device,
                cmd_pool,
                core::ptr::null_mut()
            );
        }
    }

    //
    // Sampler
    //

    let sampler_create_info = VkSamplerCreateInfo {
        sType: VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        magFilter: VK_FILTER_NEAREST,
        minFilter: VK_FILTER_NEAREST,
        mipmapMode: VK_SAMPLER_MIPMAP_MODE_NEAREST,
        addressModeU: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        addressModeV: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        addressModeW: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        mipLodBias: 0.0,
        anisotropyEnable: VK_FALSE,
        maxAnisotropy: 0.0,
        compareEnable: VK_FALSE,
        compareOp: VK_COMPARE_OP_NEVER,
        minLod: 0.0,
        maxLod: 0.0,
        borderColor: VK_BORDER_COLOR_INT_OPAQUE_BLACK,
        unnormalizedCoordinates: VK_FALSE
    };

    println!("Creating sampler.");
    let mut sampler = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateSampler(
            device,
            &sampler_create_info,
            core::ptr::null_mut(),
            &mut sampler
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create sampler. Error: {}", result);
    }

    //
    // Cube sampler
    //

    let sampler_create_info = VkSamplerCreateInfo {
        sType: VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
        pNext: std::ptr::null(),
        flags: 0x0,
        magFilter: VK_FILTER_LINEAR,
        minFilter: VK_FILTER_LINEAR,
        mipmapMode: VK_SAMPLER_MIPMAP_MODE_LINEAR,
        addressModeU: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        addressModeV: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        addressModeW: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        mipLodBias: 0.0,
        anisotropyEnable: VK_FALSE,
        maxAnisotropy: 0.0,
        compareEnable: VK_FALSE,
        compareOp: VK_COMPARE_OP_NEVER,
        minLod: 0.0,
        maxLod: MAX_ENV_MIP_LVL_COUNT as f32,
        borderColor: VK_BORDER_COLOR_INT_OPAQUE_BLACK,
        unnormalizedCoordinates: VK_FALSE
    };

    println!("Creating cube sampler.");
    let mut cube_sampler = std::ptr::null_mut();
    let result = unsafe
    {
        vkCreateSampler(
            device,
            &sampler_create_info,
            std::ptr::null_mut(),
            &mut cube_sampler
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create cube sampler. Error: {}", result);
    }

    //
    // DFG sampler
    //

    let sampler_create_info = VkSamplerCreateInfo {
        sType: VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        magFilter: VK_FILTER_LINEAR,
        minFilter: VK_FILTER_LINEAR,
        mipmapMode: VK_SAMPLER_MIPMAP_MODE_NEAREST,
        addressModeU: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        addressModeV: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        addressModeW: VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        mipLodBias: 0.0,
        anisotropyEnable: VK_FALSE,
        maxAnisotropy: 0.0,
        compareEnable: VK_FALSE,
        compareOp: VK_COMPARE_OP_NEVER,
        minLod: 0.0,
        maxLod: 0.0,
        borderColor: VK_BORDER_COLOR_INT_OPAQUE_BLACK,
        unnormalizedCoordinates: VK_FALSE
    };

    println!("Creating dfg sampler.");
    let mut dfg_sampler = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateSampler(
            device,
            &sampler_create_info,
            core::ptr::null_mut(),
            &mut dfg_sampler
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create dfg sampler. Error: {}", result);
    }

    //
    // Preintegration
    //

    {
        //
        // Preinteg descriptor pool & descriptor set
        //

        let sampler_descriptor_size = [
            VkDescriptorPoolSize {
                type_: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
                descriptorCount: 1
            },
            VkDescriptorPoolSize {
                type_: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
                descriptorCount: 1 + MAX_ENV_MIP_LVL_COUNT as u32
            }
        ];

        let descriptor_pool_create_info = VkDescriptorPoolCreateInfo {
            sType: VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
            pNext: std::ptr::null(),
            flags: 0x0,
            maxSets: 2,
            poolSizeCount: sampler_descriptor_size.len() as u32,
            pPoolSizes: sampler_descriptor_size.as_ptr()
        };

        println!("Creating preinteg descriptor pool.");
        let mut descriptor_pool = std::ptr::null_mut();
        let result = unsafe
        {
            vkCreateDescriptorPool(
                device,
                &descriptor_pool_create_info,
                std::ptr::null_mut(),
                &mut descriptor_pool
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create preinteg descriptor pool. Error: {}", result);
        }

        let descriptor_set_layouts = [
            env_preinteg_descriptor_set_layout,
            dfg_preinteg_descriptor_set_layout
        ];

        let descriptor_set_alloc_info = VkDescriptorSetAllocateInfo {
            sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
            pNext: std::ptr::null(),
            descriptorPool: descriptor_pool,
            descriptorSetCount: descriptor_set_layouts.len() as u32,
            pSetLayouts: descriptor_set_layouts.as_ptr()
        };

        println!("Allocating preinteg descriptor sets.");
        let mut compute_descriptor_sets = [std::ptr::null_mut(); 2];
        let result = unsafe
        {
            vkAllocateDescriptorSets(
                device,
                &descriptor_set_alloc_info,
                compute_descriptor_sets.as_mut_ptr()
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to allocate preinteg descriptor sets. Error: {}", result);
        }

        let env_preinteg_descriptor_set = compute_descriptor_sets[0];
        let dfg_preinteg_descriptor_set = compute_descriptor_sets[1];

        // Writing descriptor set.

        let sampler_descriptor_info_input = [
            VkDescriptorImageInfo {
                sampler: cube_sampler,
                imageView: skydome_image_view,
                imageLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL
            }
        ];

        let mut storage_img_info_env_outputs = Vec::with_capacity(MAX_ENV_MIP_LVL_COUNT as usize);

        for i in 0..MAX_ENV_MIP_LVL_COUNT
        {
            let descriptor_info = VkDescriptorImageInfo {
                sampler: std::ptr::null_mut(),
                imageView: env_image_write_views[i],
                imageLayout: VK_IMAGE_LAYOUT_GENERAL
            };
            storage_img_info_env_outputs.push(descriptor_info);
        }

        let storage_img_info_dfg_output = [
            VkDescriptorImageInfo {
                sampler: std::ptr::null_mut(),
                imageView: dfg_image_view,
                imageLayout: VK_IMAGE_LAYOUT_GENERAL
            }
        ];

        let descriptor_set_writes = [
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: std::ptr::null(),
                dstSet: env_preinteg_descriptor_set,
                dstBinding: 0,
                dstArrayElement: 0,
                descriptorCount: sampler_descriptor_info_input.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
                pImageInfo: sampler_descriptor_info_input.as_ptr(),
                pBufferInfo: std::ptr::null(),
                pTexelBufferView: std::ptr::null()
            },
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: std::ptr::null(),
                dstSet: env_preinteg_descriptor_set,
                dstBinding: 1,
                dstArrayElement: 0,
                descriptorCount: storage_img_info_env_outputs.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
                pImageInfo: storage_img_info_env_outputs.as_ptr(),
                pBufferInfo: std::ptr::null(),
                pTexelBufferView: std::ptr::null()
            },
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: std::ptr::null(),
                dstSet: dfg_preinteg_descriptor_set,
                dstBinding: 0,
                dstArrayElement: 0,
                descriptorCount: storage_img_info_dfg_output.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
                pImageInfo: storage_img_info_dfg_output.as_ptr(),
                pBufferInfo: std::ptr::null(),
                pTexelBufferView: std::ptr::null()
            }
        ];

        println!("Updating env preinteg descriptor sets.");
        unsafe
        {
            vkUpdateDescriptorSets(
                device,
                descriptor_set_writes.len() as u32,
                descriptor_set_writes.as_ptr(),
                0,
                std::ptr::null()
            );
        }

        //
        // Preinteg command pool
        //

        let cmd_pool_create_info = VkCommandPoolCreateInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            queueFamilyIndex: chosen_graphics_queue_family
        };

        println!("Creating preinteg command pool.");
        let mut cmd_pool = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateCommandPool(
                device,
                &cmd_pool_create_info,
                core::ptr::null_mut(),
                &mut cmd_pool
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create preinteg command pool. Error: {}.", result);
        }

        println!("Allocating preinteg command buffers.");
        let cmd_buffer_alloc_info = VkCommandBufferAllocateInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            commandPool: cmd_pool,
            level: VK_COMMAND_BUFFER_LEVEL_PRIMARY,
            commandBufferCount: 1
        };

        let mut preinteg_cmd_buffer = core::ptr::null_mut();
        let result = unsafe
        {
            vkAllocateCommandBuffers(
                device,
                &cmd_buffer_alloc_info,
                &mut preinteg_cmd_buffer
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create preinteg command buffer. Error: {}.", result);
        }

        let cmd_buffer_begin_info = VkCommandBufferBeginInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
            pNext: core::ptr::null(),
            flags: VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT as VkCommandBufferUsageFlags,
            pInheritanceInfo: core::ptr::null()
        };

        let result = unsafe
        {
            vkBeginCommandBuffer(
                preinteg_cmd_buffer,
                &cmd_buffer_begin_info
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to start recording the comand buffer. Error: {}.", result);
        }

        //
        // DFG and Env map preinteg
        //

        let general_barriers = [
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: 0x0 as VkAccessFlags,
                dstAccessMask: VK_ACCESS_SHADER_WRITE_BIT as VkAccessFlags,
                oldLayout: VK_IMAGE_LAYOUT_UNDEFINED,
                newLayout: VK_IMAGE_LAYOUT_GENERAL,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: env_image,
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: MAX_ENV_MIP_LVL_COUNT as u32,
                    baseArrayLayer: 0,
                    layerCount: 6
                }
            },
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: 0x0 as VkAccessFlags,
                dstAccessMask: VK_ACCESS_SHADER_WRITE_BIT as VkAccessFlags,
                oldLayout: VK_IMAGE_LAYOUT_UNDEFINED,
                newLayout: VK_IMAGE_LAYOUT_GENERAL,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: dfg_image,
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: 1,
                    baseArrayLayer: 0,
                    layerCount: 1
                }
            }
        ];

        unsafe
        {
            vkCmdPipelineBarrier(
                preinteg_cmd_buffer,
                VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT as VkPipelineStageFlags,
                VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT as VkPipelineStageFlags,
                0,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                general_barriers.len() as u32,
                general_barriers.as_ptr()
            );
        }

        unsafe
        {
            vkCmdBindDescriptorSets(
                preinteg_cmd_buffer,
                VK_PIPELINE_BIND_POINT_COMPUTE,
                env_compute_pipeline_layout,
                0,
                1,
                &env_preinteg_descriptor_set,
                0,
                std::ptr::null()
            );
        }

        unsafe
        {
            vkCmdBindPipeline(preinteg_cmd_buffer, VK_PIPELINE_BIND_POINT_COMPUTE, env_compute_pipeline);
        }

        let mut divisor = 1;
        for i in 0..MAX_ENV_MIP_LVL_COUNT
        {
            let mip_level = i as u32;
            let roughness = i as f32 * (1.0 / (MAX_ENV_MIP_LVL_COUNT - 1) as f32);

            unsafe
            {
                vkCmdPushConstants(
                    preinteg_cmd_buffer,
                    env_compute_pipeline_layout,
                    VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
                    0,
                    1 * std::mem::size_of::<u32>() as u32,
                    &mip_level as *const u32 as *const std::ffi::c_void
                );
            }

            unsafe
            {
                vkCmdPushConstants(
                    preinteg_cmd_buffer,
                    env_compute_pipeline_layout,
                    VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
                    1 * std::mem::size_of::<u32>() as u32,
                    1 * std::mem::size_of::<f32>() as u32,
                    &roughness as *const f32 as *const std::ffi::c_void
                );
            }

            let mip_lvl_width = env_img_width / divisor;
            let mip_lvl_height = env_img_height / divisor;

            let workgroup_x = if mip_lvl_width % 8 == 0  {mip_lvl_width/8}  else {mip_lvl_width/8 + 1};
            let workgroup_y = if mip_lvl_height % 8 == 0 {mip_lvl_height/8} else {mip_lvl_height/8 + 1};

            unsafe
            {
                vkCmdDispatch(
                    preinteg_cmd_buffer,
                    workgroup_x as u32,
                    workgroup_y as u32,
                    6
                );
            }

            divisor *= 2;
        }

        unsafe
        {
            vkCmdBindDescriptorSets(
                preinteg_cmd_buffer,
                VK_PIPELINE_BIND_POINT_COMPUTE,
                dfg_compute_pipeline_layout,
                0,
                1,
                &dfg_preinteg_descriptor_set,
                0,
                std::ptr::null()
            );
        }

        unsafe
        {
            vkCmdBindPipeline(
                preinteg_cmd_buffer,
                VK_PIPELINE_BIND_POINT_COMPUTE,
                dfg_compute_pipeline
            );
        }

        let workgroup_x = if dfg_img_width % 8 == 0  {dfg_img_width/8}  else {dfg_img_width/8 + 1};
        let workgroup_y = if dfg_img_height % 8 == 0 {dfg_img_height/8} else {dfg_img_height/8 + 1};

        unsafe
        {
            vkCmdDispatch(
                preinteg_cmd_buffer,
                workgroup_x as u32,
                workgroup_y as u32,
                1
            );
        }

        let shader_read_src_barriers = [
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: VK_ACCESS_SHADER_WRITE_BIT as VkAccessFlags,
                dstAccessMask: VK_ACCESS_SHADER_READ_BIT as VkAccessFlags,
                oldLayout: VK_IMAGE_LAYOUT_GENERAL,
                newLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: env_image,
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: MAX_ENV_MIP_LVL_COUNT as u32,
                    baseArrayLayer: 0,
                    layerCount: 6
                }
            },
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: VK_ACCESS_SHADER_WRITE_BIT as VkAccessFlags,
                dstAccessMask: VK_ACCESS_SHADER_READ_BIT as VkAccessFlags,
                oldLayout: VK_IMAGE_LAYOUT_GENERAL,
                newLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: dfg_image,
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: 1,
                    baseArrayLayer: 0,
                    layerCount: 1
                }
            }
        ];

        unsafe
        {
            vkCmdPipelineBarrier(
                preinteg_cmd_buffer,
                VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT as VkPipelineStageFlags,
                VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT as VkPipelineStageFlags,
                0,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                shader_read_src_barriers.len() as u32,
                shader_read_src_barriers.as_ptr()
            );
        }

        let result = unsafe
        {
            vkEndCommandBuffer(
                preinteg_cmd_buffer
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to end recording the comand buffer. Error: {}.", result);
        }

        let cmd_buffer = [preinteg_cmd_buffer];

        let submit_info = VkSubmitInfo {
            sType: VK_STRUCTURE_TYPE_SUBMIT_INFO,
            pNext: core::ptr::null(),
            waitSemaphoreCount: 0,
            pWaitSemaphores: core::ptr::null(),
            pWaitDstStageMask: core::ptr::null(),
            commandBufferCount: cmd_buffer.len() as u32,
            pCommandBuffers: cmd_buffer.as_ptr(),
            signalSemaphoreCount: 0,
            pSignalSemaphores: core::ptr::null()
        };

        let result = unsafe
        {
            vkQueueSubmit(
                graphics_queue,
                1,
                &submit_info,
                core::ptr::null_mut()
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to submit preinteg commands: {:?}.", result);
        }

        //
        // Cleanup
        //

        let _result = unsafe
        {
            vkQueueWaitIdle(graphics_queue)
        };

        println!("Deleting preinteg command pool.");
        unsafe
        {
            vkDestroyCommandPool(
                device,
                cmd_pool,
                core::ptr::null_mut()
            );
        }

        println!("Deleting preinteg descriptor pool.");
        unsafe
        {
            vkDestroyDescriptorPool(
                device,
                descriptor_pool,
                std::ptr::null_mut()
            );
        }
    }

    //
    // Uniform buffers
    //

    // Create buffer

    let uniform_buffer_create_info = VkBufferCreateInfo {
        sType: VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        size: uniform_buffer_size as VkDeviceSize,
        usage: VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT as VkBufferUsageFlags,
        sharingMode: VK_SHARING_MODE_EXCLUSIVE,
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: core::ptr::null()
    };

    println!("Creating uniform buffer.");
    let mut uniform_buffer = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateBuffer(
            device,
            &uniform_buffer_create_info,
            core::ptr::null(),
            &mut uniform_buffer
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create uniform buffer. Error: {}.", result);
    }

    // Create memory

    let mut mem_requirements = VkMemoryRequirements::default();
    unsafe
    {
        vkGetBufferMemoryRequirements(
            device,
            uniform_buffer,
            &mut mem_requirements
        );
    }

    let mut chosen_memory_type = phys_device_mem_properties.memoryTypeCount;
    for i in 0..phys_device_mem_properties.memoryTypeCount
    {
        if mem_requirements.memoryTypeBits & (1 << i) != 0 &&
            (phys_device_mem_properties.memoryTypes[i as usize].propertyFlags & uniform_buf_mem_props) == uniform_buf_mem_props
        {
            chosen_memory_type = i;
            break;
        }
    }

    if chosen_memory_type == phys_device_mem_properties.memoryTypeCount
    {
        panic!("Could not find memory type.");
    }

    let uniform_buffer_alloc_info = VkMemoryAllocateInfo {
        sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: core::ptr::null(),
        allocationSize: mem_requirements.size,
        memoryTypeIndex: chosen_memory_type
    };

    println!("Uniform buffer size: {}", mem_requirements.size);
    println!("Uniform buffer align: {}", mem_requirements.alignment);

    println!("Allocating uniform buffer memory.");
    let mut uniform_buffer_memory = core::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateMemory(
            device,
            &uniform_buffer_alloc_info,
            core::ptr::null(),
            &mut uniform_buffer_memory
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Could not allocate memory. Error: {}.", result);
    }

    // Bind buffer to memory

    println!("Binding uniform buffer memory.");
    let result = unsafe
    {
        vkBindBufferMemory(
            device,
            uniform_buffer,
            uniform_buffer_memory,
            0
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to bind memory to uniform buffer. Error: {}.", result);
    }

    // Map memory persistently

    let mut uniform_buffer_ptr = core::ptr::null_mut();
    let result = unsafe
    {
        vkMapMemory(
            device,
            uniform_buffer_memory,
            0,
            uniform_buffer_size as VkDeviceSize,
            0,
            &mut uniform_buffer_ptr
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to map memory. Error: {}.", result);
    }

    //
    // Descriptor pool & descriptor set
    //

    let pool_sizes = [
        VkDescriptorPoolSize {
            type_: VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
            descriptorCount: max_ubo_descriptor_count * 3
        },
        VkDescriptorPoolSize {
            type_: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
            descriptorCount: max_tex2d_descriptor_count + max_texcube_descriptor_count
        },
        VkDescriptorPoolSize {
            type_: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
            descriptorCount: 4 * frame_count as u32
        },
        VkDescriptorPoolSize {
            type_: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
            descriptorCount: 3 * frame_count as u32
        }
    ];

    let descriptor_pool_create_info = VkDescriptorPoolCreateInfo {
        sType: VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
        pNext: core::ptr::null(),
        flags: 0x0,
        maxSets: 1 + 2 * frame_count as u32,
        poolSizeCount: pool_sizes.len() as u32,
        pPoolSizes: pool_sizes.as_ptr()
    };

    println!("Creating descriptor pool.");
    let mut descriptor_pool = core::ptr::null_mut();
    let result = unsafe
    {
        vkCreateDescriptorPool(
            device,
            &descriptor_pool_create_info,
            core::ptr::null_mut(),
            &mut descriptor_pool
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to create descriptor pool. Error: {}", result);
    }

    // Allocating descriptor set

    let descriptor_set_layouts = [
        descriptor_set_layout
    ];

    let descriptor_set_alloc_info = VkDescriptorSetAllocateInfo {
        sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
        pNext: core::ptr::null(),
        descriptorPool: descriptor_pool,
        descriptorSetCount: descriptor_set_layouts.len() as u32,
        pSetLayouts: descriptor_set_layouts.as_ptr()
    };

    println!("Allocating descriptor set.");
    let mut descriptor_set = core::ptr::null_mut();
    let result = unsafe
    {
        vkAllocateDescriptorSets(
            device,
            &descriptor_set_alloc_info,
            &mut descriptor_set
        )
    };

    if result != VK_SUCCESS
    {
        panic!("Failed to allocate descriptor set. Error: {}", result);
    }

    let mut avg_luminance_descriptor_sets = vec![std::ptr::null_mut(); frame_count];
    {
        let descriptor_set_layouts = vec![
            avg_luminance_descriptor_set_layout;
            frame_count
        ];

        let descriptor_set_alloc_info = VkDescriptorSetAllocateInfo {
            sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            descriptorPool: descriptor_pool,
            descriptorSetCount: descriptor_set_layouts.len() as u32,
            pSetLayouts: descriptor_set_layouts.as_ptr()
        };

        println!("Allocating avg luminance descriptor set.");
        let result = unsafe
        {
            vkAllocateDescriptorSets(
                device,
                &descriptor_set_alloc_info,
                avg_luminance_descriptor_sets.as_mut_ptr()
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to allocate avg luminance descriptor set. Error: {}", result);
        }
    }

    let mut postprocessing_descriptor_sets = vec![std::ptr::null_mut(); frame_count];
    {
        let descriptor_set_layouts = vec![
            postprocessing_descriptor_set_layout;
            frame_count
        ];

        let descriptor_set_alloc_info = VkDescriptorSetAllocateInfo {
            sType: VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            descriptorPool: descriptor_pool,
            descriptorSetCount: descriptor_set_layouts.len() as u32,
            pSetLayouts: descriptor_set_layouts.as_ptr()
        };

        println!("Allocating postprocessing descriptor set.");
        let result = unsafe
        {
            vkAllocateDescriptorSets(
                device,
                &descriptor_set_alloc_info,
                postprocessing_descriptor_sets.as_mut_ptr()
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to allocate postprocessing descriptor set. Error: {}", result);
        }
    }

    // Writing descriptor set

    // Writing UBO descriptors

    let mut transform_ubo_descriptor_writes = Vec::with_capacity(max_ubo_descriptor_count as usize);
    for i in 0..max_ubo_descriptor_count
    {
        let ubo_region_index = (frame_count - 1).min(i as usize);
        transform_ubo_descriptor_writes.push(
            VkDescriptorBufferInfo {
                buffer: uniform_buffer,
                offset: (
                    ubo_region_index * padded_transform_data_size
                ) as VkDeviceSize,
                range: transform_data_size as VkDeviceSize
            }
        );
    }

    let mut material_ubo_descriptor_writes = Vec::with_capacity(max_ubo_descriptor_count as usize);
    for i in 0..max_ubo_descriptor_count
    {
        let ubo_region_index = (frame_count - 1).min(i as usize);
        material_ubo_descriptor_writes.push(
            VkDescriptorBufferInfo {
                buffer: uniform_buffer,
                offset: (
                    total_transform_data_size +
                    ubo_region_index * padded_material_data_size
                ) as VkDeviceSize,
                range: material_data_size as VkDeviceSize
            }
        );
    }

    let mut light_ubo_descriptor_writes = Vec::with_capacity(max_ubo_descriptor_count as usize);
    for i in 0..max_ubo_descriptor_count
    {
        let ubo_region_index = (frame_count - 1).min(i as usize);
        light_ubo_descriptor_writes.push(
            VkDescriptorBufferInfo {
                buffer: uniform_buffer,
                offset: (
                    total_transform_data_size + total_material_data_size +
                    ubo_region_index * padded_light_data_size
                ) as VkDeviceSize,
                range: light_data_size as VkDeviceSize
            }
        );
    }

    // Writing texture descriptors

    let mut tex2d_descriptor_writes = Vec::with_capacity(max_tex2d_descriptor_count as usize);

    tex2d_descriptor_writes.push(
        VkDescriptorImageInfo {
            sampler: dfg_sampler,
            imageView: dfg_image_view,
            imageLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL
        }
    );

    let prepended_img_count = tex2d_descriptor_writes.len() as u32;
    for i in 0..max_tex2d_descriptor_count - prepended_img_count
    {
        let image_index = (max_tex2d_descriptor_count as usize - 1)
            .min(image_views.len() - 1)
            .min(i as usize);
        tex2d_descriptor_writes.push(
            VkDescriptorImageInfo {
                sampler: sampler,
                imageView: image_views[image_index],
                imageLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL
            }
        );
    }

    // Writing cube texture descriptors

    let texcube_descriptor_writes = [
        VkDescriptorImageInfo {
            sampler: cube_sampler,
            imageView: skydome_image_view,
            imageLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL
        },
        VkDescriptorImageInfo {
            sampler: cube_sampler,
            imageView: env_image_view,
            imageLayout: VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL
        }
    ];

    let descriptor_set_writes = [
        VkWriteDescriptorSet {
            sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
            pNext: core::ptr::null(),
            dstSet: descriptor_set,
            dstBinding: 0,
            dstArrayElement: 0,
            descriptorCount: transform_ubo_descriptor_writes.len() as u32,
            descriptorType: VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
            pImageInfo: core::ptr::null(),
            pBufferInfo: transform_ubo_descriptor_writes.as_ptr(),
            pTexelBufferView: core::ptr::null()
        },
        VkWriteDescriptorSet {
            sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
            pNext: core::ptr::null(),
            dstSet: descriptor_set,
            dstBinding: 1,
            dstArrayElement: 0,
            descriptorCount: tex2d_descriptor_writes.len() as u32,
            descriptorType: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
            pImageInfo: tex2d_descriptor_writes.as_ptr(),
            pBufferInfo: core::ptr::null(),
            pTexelBufferView: core::ptr::null()
        },
        VkWriteDescriptorSet {
            sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
            pNext: core::ptr::null(),
            dstSet: descriptor_set,
            dstBinding: 2,
            dstArrayElement: 0,
            descriptorCount: material_ubo_descriptor_writes.len() as u32,
            descriptorType: VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
            pImageInfo: core::ptr::null(),
            pBufferInfo: material_ubo_descriptor_writes.as_ptr(),
            pTexelBufferView: core::ptr::null()
        },
        VkWriteDescriptorSet {
            sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
            pNext: core::ptr::null(),
            dstSet: descriptor_set,
            dstBinding: 3,
            dstArrayElement: 0,
            descriptorCount: light_ubo_descriptor_writes.len() as u32,
            descriptorType: VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
            pImageInfo: core::ptr::null(),
            pBufferInfo: light_ubo_descriptor_writes.as_ptr(),
            pTexelBufferView: core::ptr::null()
        },
        VkWriteDescriptorSet {
            sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
            pNext: core::ptr::null(),
            dstSet: descriptor_set,
            dstBinding: 4,
            dstArrayElement: 0,
            descriptorCount: texcube_descriptor_writes.len() as u32,
            descriptorType: VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
            pImageInfo: texcube_descriptor_writes.as_ptr(),
            pBufferInfo: core::ptr::null(),
            pTexelBufferView: core::ptr::null()
        }
    ];

    println!("Updating descriptor sets.");
    unsafe
    {
        vkUpdateDescriptorSets(
            device,
            descriptor_set_writes.len() as u32,
            descriptor_set_writes.as_ptr(),
            0,
            core::ptr::null()
        );
    }

    // Writing postprocess descriptor sets

    let avg_luminance_buf_descriptor_write = [
        VkDescriptorBufferInfo {
            buffer: postprocessing_buffer,
            offset: avg_luminance_begin as VkDeviceSize,
            range: avg_luminance_size as VkDeviceSize
        }
    ];

    let atomic_cnt_buf_descriptor_write = [
        VkDescriptorBufferInfo {
            buffer: postprocessing_buffer,
            offset: atomic_cnt_begin as VkDeviceSize,
            range: atomic_cnt_size as VkDeviceSize
        }
    ];

    for (i, (avg_luminance_image_view, (avg_luminance_descriptor_set, postprocess_descriptor_set))) in avg_luminance_image_views.iter().zip(avg_luminance_descriptor_sets.iter().zip(postprocessing_descriptor_sets.iter())).enumerate()
    {
        let img_descriptor_write = [
            VkDescriptorImageInfo {
                sampler: std::ptr::null_mut(),
                imageView: *avg_luminance_image_view,
                imageLayout: VK_IMAGE_LAYOUT_GENERAL
            }
        ];

        let descriptor_set_writes = [
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: core::ptr::null(),
                dstSet: *avg_luminance_descriptor_set,
                dstBinding: 1,
                dstArrayElement: 0,
                descriptorCount: img_descriptor_write.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
                pImageInfo: img_descriptor_write.as_ptr(),
                pBufferInfo: core::ptr::null(),
                pTexelBufferView: core::ptr::null()
            },
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: core::ptr::null(),
                dstSet: *avg_luminance_descriptor_set,
                dstBinding: 2,
                dstArrayElement: 0,
                descriptorCount: avg_luminance_buf_descriptor_write.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
                pImageInfo: core::ptr::null(),
                pBufferInfo: avg_luminance_buf_descriptor_write.as_ptr(),
                pTexelBufferView: core::ptr::null()
            },
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: core::ptr::null(),
                dstSet: *avg_luminance_descriptor_set,
                dstBinding: 3,
                dstArrayElement: 0,
                descriptorCount: atomic_cnt_buf_descriptor_write.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
                pImageInfo: core::ptr::null(),
                pBufferInfo: atomic_cnt_buf_descriptor_write.as_ptr(),
                pTexelBufferView: core::ptr::null()
            },
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: core::ptr::null(),
                dstSet: *postprocess_descriptor_set,
                dstBinding: 1,
                dstArrayElement: 0,
                descriptorCount: img_descriptor_write.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_IMAGE,
                pImageInfo: img_descriptor_write.as_ptr(),
                pBufferInfo: core::ptr::null(),
                pTexelBufferView: core::ptr::null()
            },
            VkWriteDescriptorSet {
                sType: VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
                pNext: core::ptr::null(),
                dstSet: *postprocess_descriptor_set,
                dstBinding: 2,
                dstArrayElement: 0,
                descriptorCount: avg_luminance_buf_descriptor_write.len() as u32,
                descriptorType: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
                pImageInfo: core::ptr::null(),
                pBufferInfo: avg_luminance_buf_descriptor_write.as_ptr(),
                pTexelBufferView: core::ptr::null()
            }
        ];

        println!("Updating avg luminance image and buffer in avg luminance and postprocess descriptor set {:?}.", i);
        unsafe
        {
            vkUpdateDescriptorSets(
                device,
                descriptor_set_writes.len() as u32,
                descriptor_set_writes.as_ptr(),
                0,
                core::ptr::null()
            );
        }
    }

    unsafe
    {
        update_postprocess_descriptor_sets(
            device,
            &mut avg_luminance_descriptor_sets,
            &mut postprocessing_descriptor_sets,
            &color_buffer_views,
            &swapchain_img_views
        );
    }

    //
    // Command pool & command buffer
    //

    let mut cmd_pools = Vec::with_capacity(frame_count);
    for _i in 0..frame_count
    {
        let cmd_pool_create_info = VkCommandPoolCreateInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            queueFamilyIndex: chosen_graphics_queue_family
        };

        println!("Creating command pool.");
        let mut cmd_pool = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateCommandPool(
                device,
                &cmd_pool_create_info,
                core::ptr::null_mut(),
                &mut cmd_pool
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create command pool. Error: {:?}.", result);
        }

        cmd_pools.push(cmd_pool);
    }

    let mut cmd_buffers = Vec::with_capacity(frame_count);
    for cmd_pool in cmd_pools.iter()
    {
        let cmd_buffer_alloc_info = VkCommandBufferAllocateInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
            pNext: core::ptr::null(),
            commandPool: *cmd_pool,
            level: VK_COMMAND_BUFFER_LEVEL_PRIMARY,
            commandBufferCount: 1
        };

        println!("Allocating command buffers.");
        let mut cmd_buffer = core::ptr::null_mut();
        let result = unsafe
        {
            vkAllocateCommandBuffers(
                device,
                &cmd_buffer_alloc_info,
                &mut cmd_buffer
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create command buffers. Error: {:?}.", result);
        }

        cmd_buffers.push(cmd_buffer);
    }

    //
    // Synchronization primitives
    //

    let mut frame_submitted = vec![false; frame_count];

    let mut image_acquired_sems = Vec::with_capacity(frame_count);
    let mut rendering_finished_sems = Vec::with_capacity(frame_count);
    let mut rendering_finished_fences = Vec::with_capacity(frame_count);

    for _i in 0..frame_count
    {
        // Image acquired semaphore

        let sem_create_info = VkSemaphoreCreateInfo {
            sType: VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0
        };

        println!("Creating image acquired semaphore.");
        let mut new_image_acquired_sem = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateSemaphore(
                device,
                &sem_create_info,
                core::ptr::null(),
                &mut new_image_acquired_sem
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create image acquired semaphore. Error: {:?}.", result);
        }

        image_acquired_sems.push(new_image_acquired_sem);

        // Rendering finished semaphore

        let sem_create_info = VkSemaphoreCreateInfo {
            sType: VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0
        };

        println!("Creating rendering finished semaphore.");
        let mut new_rendering_finished_sem = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateSemaphore(
                device,
                &sem_create_info,
                core::ptr::null(),
                &mut new_rendering_finished_sem
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create rendering finished semaphore. Error: {:?}.", result);
        }

        rendering_finished_sems.push(new_rendering_finished_sem);

        // Frame fence

        let fence_create_info = VkFenceCreateInfo {
            sType: VK_STRUCTURE_TYPE_FENCE_CREATE_INFO,
            pNext: core::ptr::null(),
            flags: 0x0
        };

        println!("Creating frame finished fence.");
        let mut new_frame_fence = core::ptr::null_mut();
        let result = unsafe
        {
            vkCreateFence(
                device,
                &fence_create_info,
                core::ptr::null(),
                &mut new_frame_fence
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to create frame fence. Error: {:?}.", result);
        }

        rendering_finished_fences.push(new_frame_fence);
    }

    //
    // Model and Texture ID-s
    //

    // Models

    struct Model
    {
        index_count: u32,
        first_index: u32,
        vertex_offset: i32
    }

    const PER_VERTEX_DATA_SIZE: usize = 8;

    let triangle_index = 0;
    let quad_index = 1;
    let cube_index = 2;
    let sphere_index = 3;

    let models = [
        // Triangle
        Model {
            index_count: 3,
            first_index: 0,
            vertex_offset: 0
        },
        // Quad
        Model {
            index_count: 6,
            first_index: 3,
            vertex_offset: 3
        },
        // Cube
        Model {
            index_count: cube_indices.len() as u32,
            first_index: tri_and_quad_indices.len() as u32,
            vertex_offset: (tri_and_quad_vertices.len() / PER_VERTEX_DATA_SIZE) as i32
        },
        // Sphere
        Model {
            index_count: sphere_indices.len() as u32,
            first_index: (tri_and_quad_indices.len() + cube_indices.len()) as u32,
            vertex_offset: ((tri_and_quad_vertices.len() + cube_vertices.len()) / PER_VERTEX_DATA_SIZE) as i32
        }
    ];

    // Textures

    let red_yellow_green_black_tex_index = 0;
    let blue_cyan_magenta_white_tex_index = 1;

    // Materials

    // Source: https://refractiveindex.info/?shelf=3d&book=metals&page=copper

    // Gold
    let gold_refractive_index = [0.18601, 0.59580, 1.4120];
    let gold_extinction_coeff = [3.3762, 2.0765, 1.7827];
    let gold_albedo = calculate_fresnel_rgb(gold_refractive_index, gold_extinction_coeff);

    // Silver
    let silver_refractive_index = [0.15865, 0.14215, 0.13533];
    let silver_extinction_coeff = [3.8929, 3.0051, 2.3276];
    let silver_albedo = calculate_fresnel_rgb(silver_refractive_index, silver_extinction_coeff);

    // Copper
    let copper_refractive_index = [0.28046, 0.85418, 1.3284];
    let copper_extinction_coeff = [3.5587, 2.4518, 2.2949];
    let copper_albedo = calculate_fresnel_rgb(copper_refractive_index, copper_extinction_coeff);

    let non_metal_fresnel = 0.5;

    //
    // Game state
    //

    // Input state

    let mut obj_forward = false;
    let mut obj_backward = false;
    let mut obj_left = false;
    let mut obj_right = false;
    let mut obj_turn_left = false;
    let mut obj_turn_right = false;
    let mut obj_turn_up = false;
    let mut obj_turn_down = false;

    let mut cam_forward = false;
    let mut cam_backward = false;
    let mut cam_left = false;
    let mut cam_right = false;
    let mut cam_turn_left = false;
    let mut cam_turn_right = false;
    let mut cam_turn_up = false;
    let mut cam_turn_down = false;

    let mut manual_exposure = false;
    let mut exposure_increase = false;
    let mut exposure_decrease = false;

    // Game logic state

    struct Camera
    {
        x: f32,
        y: f32,
        z: f32,
        rot_y: f32,
        rot_x: f32
    }

    struct StaticMesh
    {
        x: f32,
        y: f32,
        z: f32,
        scale: f32,
        rot_x: f32,
        rot_y: f32,
        albedo_r: f32,
        albedo_g: f32,
        albedo_b: f32,
        fresnel: f32,
        roughness: f32,
        metalness: f32,
        reflectiveness: f32,
        emissive_r: f32,
        emissive_g: f32,
        emissive_b: f32,
        texture_index: u32,
        model_index: usize
    }

    struct Light
    {
        x: f32,
        y: f32,
        z: f32,
        radius: f32,
        intensity_r: f32,
        intensity_g: f32,
        intensity_b: f32
    }

    let mut camera = Camera {
        x: 0.0,
        y: 0.0,
        z: -0.25,
        rot_y: 0.0,
        rot_x: 0.0
    };

    let mut exposure_value = 9.0;

    let player_id = 0;

    let mut static_meshes = Vec::with_capacity(max_object_count);
    static_meshes.push(
        StaticMesh {
            x: 0.25,
            y: 0.0,
            z: -1.25,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: red_yellow_green_black_tex_index,
            model_index: triangle_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: 0.25,
            y: -0.25,
            z: -2.0,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 0.75,
            albedo_g: 0.5,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: blue_cyan_magenta_white_tex_index,
            model_index: triangle_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: -0.25,
            y: 0.25,
            z: -3.0,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 0.75,
            albedo_g: 0.5,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: blue_cyan_magenta_white_tex_index,
            model_index: quad_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: 1.5,
            y: 0.0,
            z: -2.6,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: red_yellow_green_black_tex_index,
            model_index: quad_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: -1.5,
            y: 0.0,
            z: -2.6,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: blue_cyan_magenta_white_tex_index,
            model_index: quad_index
        }
    );

    // Cubes added later

    static_meshes.push(
        StaticMesh {
            x: 1.0,
            y: 0.5,
            z: -2.5,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: red_yellow_green_black_tex_index,
            model_index: cube_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: 1.0,
            y: -0.5,
            z: -2.5,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: red_yellow_green_black_tex_index,
            model_index: cube_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: -1.0,
            y: 0.5,
            z: -2.5,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: blue_cyan_magenta_white_tex_index,
            model_index: cube_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: -1.0,
            y: -0.5,
            z: -2.5,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: blue_cyan_magenta_white_tex_index,
            model_index: cube_index
        }
    );

    // Spheres added later

    static_meshes.push(
        StaticMesh {
            x: 2.0,
            y: 0.5,
            z: -2.5,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: red_yellow_green_black_tex_index,
            model_index: sphere_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: 2.0,
            y: -0.5,
            z: -2.5,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: red_yellow_green_black_tex_index,
            model_index: sphere_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: -2.0,
            y: 0.5,
            z: -2.5,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: blue_cyan_magenta_white_tex_index,
            model_index: sphere_index
        }
    );
    static_meshes.push(
        StaticMesh {
            x: -2.0,
            y: -0.5,
            z: -2.5,
            scale: 0.25,
            rot_x: 0.0,
            rot_y: 0.0,
            albedo_r: 1.0,
            albedo_g: 1.0,
            albedo_b: 1.0,
            fresnel: non_metal_fresnel,
            roughness: 0.1,
            metalness: 0.0,
            reflectiveness: 0.25,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            texture_index: blue_cyan_magenta_white_tex_index,
            model_index: sphere_index
        }
    );

    //let min_roughness: f32 = 0.01;
    let min_roughness: f32 = 0.1;

    let max_demo_spheres = 6;
    for i in 0..max_demo_spheres
    {
        static_meshes.push(
            StaticMesh {
                x: 0.5 - i as f32 * 0.25,
                y:  0.5,
                z: -0.75,
                scale: 0.125,
                rot_x: 0.0,
                rot_y: 0.0,
                albedo_r: 1.0,
                albedo_g: 1.0,
                albedo_b: 1.0,
                fresnel: non_metal_fresnel,
                roughness: min_roughness.max(1.0 - i as f32 * (1.0 / (max_demo_spheres - 1) as f32)),
                metalness: 0.0,
                reflectiveness: 0.75,
                emissive_r: 0.0,
                emissive_g: 0.0,
                emissive_b: 0.0,
                texture_index: red_yellow_green_black_tex_index,
                model_index: sphere_index
            }
        );

        static_meshes.push(
            StaticMesh {
                x: 0.5 - i as f32 * 0.25,
                y:  0.25,
                z: -0.75,
                scale: 0.125,
                rot_x: 0.0,
                rot_y: 0.0,
                albedo_r: 1.0,
                albedo_g: 1.0,
                albedo_b: 1.0,
                fresnel: non_metal_fresnel,
                roughness: min_roughness.max(1.0 - i as f32 * (1.0 / (max_demo_spheres - 1) as f32)),
                metalness: 0.0,
                reflectiveness: 0.75,
                emissive_r: 0.0,
                emissive_g: 0.0,
                emissive_b: 0.0,
                texture_index: blue_cyan_magenta_white_tex_index,
                model_index: sphere_index
            }
        );

        static_meshes.push(
            StaticMesh {
                x: 0.5 - i as f32 * 0.25,
                y:  0.0,
                z: -0.75,
                scale: 0.125,
                rot_x: 0.0,
                rot_y: 0.0,
                albedo_r: silver_albedo[0],
                albedo_g: silver_albedo[1],
                albedo_b: silver_albedo[2],
                fresnel: 0.0,
                roughness: min_roughness.max(1.0 - i as f32 * (1.0 / (max_demo_spheres - 1) as f32)),
                metalness: 1.0,
                reflectiveness: 1.0,
                emissive_r: 0.0,
                emissive_g: 0.0,
                emissive_b: 0.0,
                texture_index: blue_cyan_magenta_white_tex_index,
                model_index: sphere_index
            }
        );

        static_meshes.push(
            StaticMesh {
                x: 0.5 - i as f32 * 0.25,
                y: -0.25,
                z: -0.75,
                scale: 0.125,
                rot_x: 0.0,
                rot_y: 0.0,
                albedo_r: gold_albedo[0],
                albedo_g: gold_albedo[1],
                albedo_b: gold_albedo[2],
                fresnel: 0.0,
                roughness: min_roughness.max(1.0 - i as f32 * (1.0 / (max_demo_spheres - 1) as f32)),
                metalness: 1.0,
                reflectiveness: 1.0,
                emissive_r: 0.0,
                emissive_g: 0.0,
                emissive_b: 0.0,
                texture_index: blue_cyan_magenta_white_tex_index,
                model_index: sphere_index
            }
        );

        static_meshes.push(
            StaticMesh {
                x: 0.5 - i as f32 * 0.25,
                y: -0.5,
                z: -0.75,
                scale: 0.125,
                rot_x: 0.0,
                rot_y: 0.0,
                albedo_r: copper_albedo[0],
                albedo_g: copper_albedo[1],
                albedo_b: copper_albedo[2],
                fresnel: 0.0,
                roughness: min_roughness.max(1.0 - i as f32 * (1.0 / (max_demo_spheres - 1) as f32)),
                metalness: 1.0,
                reflectiveness: 1.0,
                emissive_r: 0.0,
                emissive_g: 0.0,
                emissive_b: 0.0,
                texture_index: blue_cyan_magenta_white_tex_index,
                model_index: sphere_index
            }
        );
    }

    // Load lights from DAT file or use defaults
    let mut lights = Vec::with_capacity(max_light_count);
    
    // Try to load lights from a DAT file
    let dat_paths = vec![
        "REZ/WORLDS/REALM0/R1M1b_BadStreets.dat",
        "REZ/WORLDS/light_test.dat",
        "../Jupiter_Ent/Development/TO2/Game/Worlds/RetailSinglePlayer/Tut_Lights.dat",
    ];
    
    let mut lights_loaded = false;
    for dat_path in &dat_paths {
        match DatFile::read_from_file(dat_path) {
            Ok(dat_file) => {
                // Extract lights from the DAT file's objects
                let dat_lights = extract_lights_from_objects(&dat_file.objects);
                if !dat_lights.is_empty() {
                    lights.extend(dat_lights);
                    lights_loaded = true;
                    println!("Loaded {} lights from {}", lights.len(), dat_path);
                    break;
                }
            }
            Err(e) => {
                // File not found or parse error, try next path
                println!("Could not load lights from {}: {}", dat_path, e);
            }
        }
    }
    
    // If no lights were loaded from DAT files, use default hardcoded lights
    if !lights_loaded {
        println!("No lights loaded from DAT files, using default hardcoded lights");
        
        let point_light_radius = 0.0125;
        
        lights.push(
            Light {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                radius: 0.075,
                intensity_r: 0.5,
                intensity_g: 0.5,
                intensity_b: 0.5
            }
        );
        lights.push(
            Light {
                x: 0.25,
                y: -0.25,
                z: -1.9,
                radius: point_light_radius,
                intensity_r: 0.025,
                intensity_g: 0.025,
                intensity_b: 0.025
            }
        );
        lights.push(
            Light {
                x: -0.25,
                y: 0.25,
                z: -2.9,
                radius: point_light_radius,
                intensity_r: 0.025,
                intensity_g: 0.025,
                intensity_b: 0.025
            }
        );
        lights.push(
            Light {
                x: 1.5,
                y: 0.0,
                z: -2.5,
                radius: point_light_radius,
                intensity_r: 0.125,
                intensity_g: 0.125,
                intensity_b: 0.025
            }
        );
        lights.push(
            Light {
                x: -1.5,
                y: 0.0,
                z: -2.5,
                radius: point_light_radius,
                intensity_r: 0.125,
                intensity_g: 0.025,
                intensity_b: 0.025
            }
        );
        lights.push(
            Light {
                x: -1.0,
                y: 0.0,
                z: -2.9,
                radius: point_light_radius,
                intensity_r: 0.25,
                intensity_g: 0.25,
                intensity_b: 0.25
            }
        );
        lights.push(
            Light {
                x: 1.0,
                y: 0.0,
                z: -2.9,
                radius: point_light_radius,
                intensity_r: 0.25,
                intensity_g: 0.25,
                intensity_b: 0.25
            }
        );
    }

    //
    // Game loop
    //

    let mut recreate_swapchain = false;
    let mut current_frame_index = 0;
    let mut previous_frame_index = 0;

    let mut event_pump = sdl.event_pump().unwrap();
    'main: loop
    {
        for event in event_pump.poll_iter()
        {
            match event
            {
                sdl2::event::Event::Quit { .. } =>
                {
                    break 'main;
                }
                sdl2::event::Event::Window { win_event, .. } =>
                {
                    if let sdl2::event::WindowEvent::Resized(new_width, new_height) = win_event
                    {
                        let new_width = new_width as u32;
                        let new_height = new_height as u32;
                        if new_width != width || new_height != height
                        {
                            width = new_width;
                            height = new_height;
                            recreate_swapchain = true;
                        }
                    }
                }
                sdl2::event::Event::KeyDown { keycode: Some(keycode), .. } =>
                {
                    if keycode == sdl2::keyboard::Keycode::W
                    {
                        obj_forward = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::S
                    {
                        obj_backward = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::A
                    {
                        obj_left = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::D
                    {
                        obj_right = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::Q
                    {
                        obj_turn_left = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::E
                    {
                        obj_turn_right = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::R
                    {
                        obj_turn_up = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::F
                    {
                        obj_turn_down = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::U
                    {
                        cam_forward = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::J
                    {
                        cam_backward = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::H
                    {
                        cam_left = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::K
                    {
                        cam_right = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::Z
                    {
                        cam_turn_left = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::I
                    {
                        cam_turn_right = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::O
                    {
                        cam_turn_up = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::L
                    {
                        cam_turn_down = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::Y
                    {
                        exposure_increase = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::X
                    {
                        exposure_decrease = true;
                    }
                    if keycode == sdl2::keyboard::Keycode::C
                    {
                        manual_exposure = !manual_exposure;
                    }
                }
                sdl2::event::Event::KeyUp { keycode: Some(keycode), .. } =>
                {
                    if keycode == sdl2::keyboard::Keycode::W
                    {
                        obj_forward = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::S
                    {
                        obj_backward = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::A
                    {
                        obj_left = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::D
                    {
                        obj_right = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::Q
                    {
                        obj_turn_left = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::E
                    {
                        obj_turn_right = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::R
                    {
                        obj_turn_up = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::F
                    {
                        obj_turn_down = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::U
                    {
                        cam_forward = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::J
                    {
                        cam_backward = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::H
                    {
                        cam_left = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::K
                    {
                        cam_right = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::Z
                    {
                        cam_turn_left = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::I
                    {
                        cam_turn_right = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::O
                    {
                        cam_turn_up = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::L
                    {
                        cam_turn_down = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::Y
                    {
                        exposure_increase = false;
                    }
                    if keycode == sdl2::keyboard::Keycode::X
                    {
                        exposure_decrease = false;
                    }
                }
                _ =>
                {}
            }
        }

        //
        // Logic
        //

        if obj_forward
        {
            static_meshes[player_id].z -= 0.01;
        }
        if obj_backward
        {
            static_meshes[player_id].z += 0.01;
        }
        if obj_left
        {
            static_meshes[player_id].x -= 0.01;
        }
        if obj_right
        {
            static_meshes[player_id].x += 0.01;
        }
        if obj_turn_left
        {
            static_meshes[player_id].rot_y += 0.01;
        }
        if obj_turn_right
        {
            static_meshes[player_id].rot_y -= 0.01;
        }
        if obj_turn_up
        {
            static_meshes[player_id].rot_x += 0.01;
        }
        if obj_turn_down
        {
            static_meshes[player_id].rot_x -= 0.01;
        }

        if cam_forward
        {
            camera.x -= 0.01 * camera.rot_y.sin();
            camera.z -= 0.01 * camera.rot_y.cos();
        }
        if cam_backward
        {
            camera.x += 0.01 * camera.rot_y.sin();
            camera.z += 0.01 * camera.rot_y.cos();
        }
        if cam_left
        {
            camera.x -= 0.01 * camera.rot_y.cos();
            camera.z -= 0.01 * -camera.rot_y.sin();
        }
        if cam_right
        {
            camera.x += 0.01 * camera.rot_y.cos();
            camera.z += 0.01 * -camera.rot_y.sin();
        }
        if cam_turn_left
        {
            camera.rot_y += 0.01;
        }
        if cam_turn_right
        {
            camera.rot_y -= 0.01;
        }
        if cam_turn_up
        {
            camera.rot_x += 0.01;
            camera.rot_x = camera.rot_x.min(core::f32::consts::PI / 2.0);
        }
        if cam_turn_down
        {
            camera.rot_x -= 0.01;
            camera.rot_x = camera.rot_x.max(-core::f32::consts::PI / 2.0);
        }

        if exposure_increase
        {
            exposure_value += 0.1;
        }
        if exposure_decrease
        {
            exposure_value -= 0.1;
        }

        //
        // Rendering
        //

        //
        // Recreate swapchain if needed
        //

        if recreate_swapchain
        {
            let mut fences = Vec::with_capacity(frame_count);
            for (frame_index, frame_submitted) in frame_submitted.iter_mut().enumerate()
            {
                if *frame_submitted
                {
                    fences.push(rendering_finished_fences[frame_index]);
                    *frame_submitted = false;
                }
            }

            let result = unsafe
            {
                vkWaitForFences(
                    device,
                    fences.len() as u32,
                    fences.as_ptr(),
                    VK_TRUE,
                    core::u64::MAX
                )
            };

            if result != VK_SUCCESS
            {
                panic!("Error while waiting for fences. Error: {:?}.", result);
            }

            let result = unsafe
            {
                vkResetFences(
                    device,
                    fences.len() as u32,
                    fences.as_ptr()
                )
            };

            if result != VK_SUCCESS
            {
                panic!("Error while resetting fences. Error: {:?}.", result);
            }

            unsafe
            {
                destroy_framebuffers_and_render_targets(
                    device,
                    &mut swapchain_img_views,
                    &mut color_buffers,
                    &mut color_buffer_memories,
                    &mut color_buffer_views,
                    &mut depth_buffers,
                    &mut depth_buffer_memories,
                    &mut depth_buffer_views,
                    &mut framebuffers
                );
            }

            let SwapchainResult {
                swapchain: new_swapchain,
                width: new_width,
                height: new_height
            } = unsafe
            {
                create_swapchain(
                    chosen_phys_device,
                    surface,
                    device,
                    swapchain,
                    width,
                    height,
                    format,
                        chosen_graphics_queue_family,
                        chosen_present_queue_family
                )
            };

            swapchain = new_swapchain;
            width = new_width;
            height = new_height;

            swapchain_imgs.clear();

            unsafe
            {
                create_framebuffers_and_render_targets(
                    device,
                    chosen_phys_device,
                    &phys_device_mem_properties,
                    width,
                    height,
                    format,
                        render_pass,
                        swapchain,
                        &mut swapchain_imgs,
                        &mut swapchain_img_views,
                        &mut color_buffers,
                        &mut color_buffer_memories,
                        &mut color_buffer_views,
                        &mut depth_buffers,
                        &mut depth_buffer_memories,
                        &mut depth_buffer_views,
                        &mut framebuffers
                );
            }

            if swapchain_imgs.len() != frame_count
            {
                panic!("New swapchain has a different amount of images than the old one.");
            }

            unsafe
            {
                update_postprocess_descriptor_sets(
                    device,
                    &mut avg_luminance_descriptor_sets,
                    &mut postprocessing_descriptor_sets,
                    &color_buffer_views,
                    &swapchain_img_views
                );
            }

            recreate_swapchain = false;
        }

        //
        // Waiting for previous frame
        //

        if frame_submitted[current_frame_index]
        {
            let fences = [rendering_finished_fences[current_frame_index]];
            let result = unsafe
            {
                vkWaitForFences(
                    device,
                    fences.len() as u32,
                    fences.as_ptr(),
                    VK_TRUE,
                    core::u64::MAX
                )
            };

            if result != VK_SUCCESS
            {
                panic!("Error while waiting for fences. Error: {:?}.", result);
            }

            let result = unsafe
            {
                vkResetFences(
                    device,
                    fences.len() as u32,
                    fences.as_ptr()
                )
            };

            if result != VK_SUCCESS
            {
                panic!("Error while resetting fences. Error: {:?}.", result);
            }

            frame_submitted[current_frame_index] = false;
        }

        //
        // Uniform upload
        //

        {
            // Getting references

            let current_frame_transform_region_offset = (
                current_frame_index * padded_transform_data_size
            ) as isize;
            let vs_camera_data;
            let transform_data_array;
            unsafe
            {
                let per_frame_transform_region_begin = uniform_buffer_ptr.offset(
                    current_frame_transform_region_offset
                );

                let vs_camera_data_ptr: *mut core::mem::MaybeUninit<VsCameraData> = core::mem::transmute(
                    per_frame_transform_region_begin
                );
                vs_camera_data = &mut *vs_camera_data_ptr;

                let transform_offset = core::mem::size_of::<VsCameraData>() as isize;
                let transform_data_ptr: *mut core::mem::MaybeUninit<TransformData> = core::mem::transmute(
                    per_frame_transform_region_begin.offset(transform_offset)
                );
                transform_data_array = core::slice::from_raw_parts_mut(
                    transform_data_ptr,
                    max_object_count
                );
            }

            let current_frame_material_region_offset = (
                total_transform_data_size +
                current_frame_index * padded_material_data_size
            ) as isize;
            let fs_camera_data;
            let material_data_array;
            unsafe
            {
                let per_frame_material_region_begin = uniform_buffer_ptr.offset(
                    current_frame_material_region_offset
                );

                let fs_camera_data_ptr: *mut core::mem::MaybeUninit<FsCameraData> = core::mem::transmute(
                    per_frame_material_region_begin
                );
                fs_camera_data = &mut *fs_camera_data_ptr;

                let material_offset = core::mem::size_of::<FsCameraData>() as isize;
                let material_data_ptr: *mut core::mem::MaybeUninit<MaterialData> = core::mem::transmute(
                    per_frame_material_region_begin.offset(material_offset)
                );
                material_data_array = core::slice::from_raw_parts_mut(
                    material_data_ptr,
                    max_object_count
                );
            }

            let current_frame_light_region_offset = (
                total_transform_data_size + total_material_data_size +
                current_frame_index * padded_light_data_size
            ) as isize;
            let light_count_data;
            let light_data_array;
            unsafe
            {
                let per_frame_light_region_begin = uniform_buffer_ptr.offset(
                    current_frame_light_region_offset
                );

                let light_count_data_ptr: *mut core::mem::MaybeUninit<LightCountData> = core::mem::transmute(
                    per_frame_light_region_begin
                );
                light_count_data = &mut *light_count_data_ptr;

                let light_offset = core::mem::size_of::<LightCountData>() as isize;
                let light_data_ptr: *mut core::mem::MaybeUninit<LightData> = core::mem::transmute(
                    per_frame_light_region_begin.offset(light_offset)
                );
                light_data_array = core::slice::from_raw_parts_mut(
                    light_data_ptr,
                    max_light_count
                );
            }

            // Filling them with data

            let field_of_view_angle = core::f32::consts::PI / 3.0;
            let aspect_ratio = width as f32 / height as f32;
            let far = 100.0;
            let near = 0.1;
            let projection_matrix = perspective(
                field_of_view_angle,
                aspect_ratio,
                far,
                near
            );

            *vs_camera_data = core::mem::MaybeUninit::new(
                VsCameraData {
                    projection_matrix: mat_mlt(
                        &projection_matrix,
                        &scale(1.0, -1.0, -1.0)
                    ),
                    view_matrix: mat_mlt(
                        &rotate_x(-camera.rot_x),
                        &mat_mlt(
                            &rotate_y(-camera.rot_y),
                            &translate(
                                -camera.x,
                                -camera.y,
                                -camera.z
                            )
                        )
                    )
                }
            );

            let static_mesh_transform_data_array = &mut transform_data_array[..static_meshes.len()];
            for (i, static_mesh) in static_meshes.iter().enumerate()
            {
                static_mesh_transform_data_array[i] = core::mem::MaybeUninit::new(
                    TransformData {
                        model_matrix: mat_mlt(
                            &translate(
                                static_mesh.x,
                                static_mesh.y,
                                static_mesh.z
                            ),
                            &mat_mlt(
                                &rotate_x(static_mesh.rot_x),
                                &mat_mlt(
                                    &rotate_y(static_mesh.rot_y),
                                    &scale(
                                        static_mesh.scale,
                                        static_mesh.scale,
                                        static_mesh.scale
                                    )
                                )
                            )
                        )
                    }
                );
            }

            *fs_camera_data = core::mem::MaybeUninit::new(
                FsCameraData {
                    camera_x: camera.x,
                    camera_y: camera.y,
                    camera_z: camera.z,
                    std140_padding_0: 0.0
                }
            );

            let static_mesh_material_data_array = &mut material_data_array[..static_meshes.len()];
            for (i, static_mesh) in static_meshes.iter().enumerate()
            {
                static_mesh_material_data_array[i] = core::mem::MaybeUninit::new(
                    MaterialData {
                        albedo: [
                            static_mesh.albedo_r,
                            static_mesh.albedo_g,
                            static_mesh.albedo_b,
                            static_mesh.fresnel
                        ],
                        roughness: static_mesh.roughness,
                        metalness: static_mesh.metalness,
                        reflectiveness: static_mesh.reflectiveness,
                        std140_padding_0: 0.0,
                        emissive: [
                            static_mesh.emissive_r,
                            static_mesh.emissive_g,
                            static_mesh.emissive_b,
                            0.0
                        ]
                    }
                );
            }

            *light_count_data = core::mem::MaybeUninit::new(
                LightCountData {
                    light_count: lights.len() as u32,
                    std140_padding_0: 0.0,
                    std140_padding_1: 0.0,
                    std140_padding_2: 0.0
                }
            );

            for (i, light) in lights.iter().enumerate()
            {
                light_data_array[i] = core::mem::MaybeUninit::new(
                    LightData {
                        position: [
                            light.x,
                            light.y,
                            light.z,
                            light.radius
                        ],
                        intensity: [
                            light.intensity_r,
                            light.intensity_g,
                            light.intensity_b,
                            0.0
                        ]
                    }
                );
            }

            let light_transform_data_array = &mut transform_data_array[static_meshes.len()..static_meshes.len() + lights.len()];
            for (i, light) in lights.iter().enumerate()
            {
                light_transform_data_array[i] = core::mem::MaybeUninit::new(
                    TransformData {
                        model_matrix: mat_mlt(
                            &translate(
                                light.x,
                                light.y,
                                light.z
                            ),
                            &scale(
                                light.radius,
                                light.radius,
                                light.radius
                            )
                        )
                    }
                );
            }

            let light_material_data_array = &mut material_data_array[static_meshes.len()..static_meshes.len() + lights.len()];
            for (i, light) in lights.iter().enumerate()
            {
                let mlt = 1.0 / (light.radius * light.radius);
                light_material_data_array[i] = core::mem::MaybeUninit::new(
                    MaterialData {
                        albedo: [
                            0.0,
                            0.0,
                            0.0,
                            0.0
                        ],
                        roughness: 0.0,
                        metalness: 0.0,
                        reflectiveness: 0.0,
                        std140_padding_0: 0.0,
                        emissive: [
                            light.intensity_r * mlt,
                            light.intensity_g * mlt,
                            light.intensity_b * mlt,
                            0.0
                        ]
                    }
                );
            }
        }

        //
        // Acquire image
        //

        let mut image_index: u32 = 0;
        let result = unsafe
        {
            vkAcquireNextImageKHR(
                device,
                swapchain,
                core::u64::MAX,
                image_acquired_sems[current_frame_index],
                core::ptr::null_mut(),
                &mut image_index
            )
        };

        if result != VK_SUCCESS
        {
            if result == VK_ERROR_OUT_OF_DATE_KHR
            {
                recreate_swapchain = true;
                continue;
            }
            else if result == VK_SUBOPTIMAL_KHR
            {
                recreate_swapchain = true;
            }
            else
            {
                panic!("Fatal error while acquiring image: {:?}", result);
            }
        }

        //
        // Record
        //

        // Reset command pool

        let result = unsafe
        {
            vkResetCommandPool(
                device,
                cmd_pools[current_frame_index],
                0x0
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Error while resetting command pool. Error: {:?}.", result);
        }

        // Record command buffer

        let cmd_buffer_begin_info = VkCommandBufferBeginInfo {
            sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
            pNext: core::ptr::null(),
            flags: 0x0,
            pInheritanceInfo: core::ptr::null()
        };

        let result = unsafe
        {
            vkBeginCommandBuffer(
                cmd_buffers[current_frame_index],
                &cmd_buffer_begin_info
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to start recording the comand buffer. Error: {:?}.", result);
        }

        //
        // Rendering commands
        //

        let clear_value = [
            VkClearValue {
                color: VkClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0]
                }
            },
            VkClearValue {
                depthStencil: VkClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0
                }
            }
        ];

        let render_pass_begin_info = VkRenderPassBeginInfo {
            sType: VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
            pNext: core::ptr::null(),
            renderPass: render_pass,
            framebuffer: framebuffers[image_index as usize],
            renderArea: VkRect2D {
                offset: VkOffset2D {
                    x: 0,
                    y: 0
                },
                extent: VkExtent2D {
                    width: width,
                    height: height
                }
            },
            clearValueCount: clear_value.len() as u32,
            pClearValues: clear_value.as_ptr()
        };

        unsafe
        {
            vkCmdBeginRenderPass(
                cmd_buffers[current_frame_index],
                &render_pass_begin_info,
                VK_SUBPASS_CONTENTS_INLINE
            );

            let viewports = [
                VkViewport {
                    x: 0.0,
                    y: 0.0,
                    width: width as f32,
                    height: height as f32,
                    minDepth: 0.0,
                    maxDepth: 1.0
                }
            ];
            vkCmdSetViewport(
                cmd_buffers[current_frame_index],
                0,
                viewports.len() as u32,
                viewports.as_ptr()
            );

            let scissors = [
                VkRect2D {
                    offset: VkOffset2D {
                        x: 0,
                        y: 0
                    },
                    extent: VkExtent2D {
                        width: width,
                        height: height
                    }
                }
            ];
            vkCmdSetScissor(
                cmd_buffers[current_frame_index],
                0,
                scissors.len() as u32,
                scissors.as_ptr()
            );

            let descriptor_sets = [
                descriptor_set
            ];

            vkCmdBindDescriptorSets(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_BIND_POINT_GRAPHICS,
                pipeline_layout,
                0,
                descriptor_sets.len() as u32,
                descriptor_sets.as_ptr(),
                0,
                core::ptr::null()
            );

            let vertex_buffers = [
                vertex_buffer
            ];
            let offsets = [
                0
            ];
            vkCmdBindVertexBuffers(
                cmd_buffers[current_frame_index],
                0,
                vertex_buffers.len() as u32,
                vertex_buffers.as_ptr(),
                offsets.as_ptr()
            );

            vkCmdBindIndexBuffer(
                cmd_buffers[current_frame_index],
                index_buffer,
                0,
                VK_INDEX_TYPE_UINT32
            );

            // Setting per frame descriptor array index
            let ubo_desc_index: u32 = current_frame_index as u32;

            vkCmdPushConstants(
                cmd_buffers[current_frame_index],
                pipeline_layout,
                (VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT) as VkShaderStageFlags,
                core::mem::size_of::<u32>() as u32,
                core::mem::size_of::<u32>() as u32,
                &ubo_desc_index as *const u32 as *const core::ffi::c_void
            );

            vkCmdBindPipeline(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_BIND_POINT_GRAPHICS,
                skydome_pipeline
            );

            vkCmdDrawIndexed(
                cmd_buffers[current_frame_index],
                models[sphere_index].index_count,
                1,
                models[sphere_index].first_index,
                models[sphere_index].vertex_offset,
                0
            );

            vkCmdBindPipeline(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_BIND_POINT_GRAPHICS,
                model_pipeline
            );

            for (i, static_mesh) in static_meshes.iter().enumerate()
            {
                // Per obj array index
                let object_index = i as u32;

                vkCmdPushConstants(
                    cmd_buffers[current_frame_index],
                    pipeline_layout,
                    (VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT) as VkShaderStageFlags,
                    0,
                    core::mem::size_of::<u32>() as u32,
                    &object_index as *const u32 as *const core::ffi::c_void
                );

                // Setting texture descriptor array index
                vkCmdPushConstants(
                    cmd_buffers[current_frame_index],
                    pipeline_layout,
                    VK_SHADER_STAGE_FRAGMENT_BIT as VkShaderStageFlags,
                    2 * core::mem::size_of::<u32>() as u32,
                    core::mem::size_of::<u32>() as u32,
                    &static_mesh.texture_index as *const u32 as *const core::ffi::c_void
                );

                vkCmdDrawIndexed(
                    cmd_buffers[current_frame_index],
                    models[static_mesh.model_index].index_count,
                    1,
                    models[static_mesh.model_index].first_index,
                    models[static_mesh.model_index].vertex_offset,
                    0
                );
            }

            // Setting texture descriptor array index. For lights it is irrelevant.
            let texture_index: u32 = 0;
            vkCmdPushConstants(
                cmd_buffers[current_frame_index],
                pipeline_layout,
                VK_SHADER_STAGE_FRAGMENT_BIT as VkShaderStageFlags,
                2 * core::mem::size_of::<u32>() as u32,
                core::mem::size_of::<u32>() as u32,
                &texture_index as *const u32 as *const core::ffi::c_void
            );

            for light_obj_index in static_meshes.len()..(static_meshes.len() + lights.len())
            {
                // Per obj array index
                let object_index = light_obj_index as u32;

                vkCmdPushConstants(
                    cmd_buffers[current_frame_index],
                    pipeline_layout,
                    (VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT) as VkShaderStageFlags,
                    0,
                    core::mem::size_of::<u32>() as u32,
                    &object_index as *const u32 as *const core::ffi::c_void
                );

                vkCmdDrawIndexed(
                    cmd_buffers[current_frame_index],
                    models[sphere_index].index_count,
                    1,
                    models[sphere_index].first_index,
                    models[sphere_index].vertex_offset,
                    0
                );
            }

            vkCmdEndRenderPass(
                cmd_buffers[current_frame_index]
            );
        }

        //
        // Postprocessing
        //

        let avg_luminance_begin_barriers = [
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT as VkAccessFlags,
                dstAccessMask: VK_ACCESS_SHADER_READ_BIT as VkAccessFlags,
                oldLayout: VK_IMAGE_LAYOUT_GENERAL,
                newLayout: VK_IMAGE_LAYOUT_GENERAL,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: color_buffers[image_index as usize],
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: 1,
                    baseArrayLayer: 0,
                    layerCount: 1
                }
            }
        ];

        unsafe
        {
            vkCmdPipelineBarrier(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT as VkPipelineStageFlags,
                VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT as VkPipelineStageFlags,
                0,
                0,
                core::ptr::null(),
                0,
                core::ptr::null(),
                avg_luminance_begin_barriers.len() as u32,
                avg_luminance_begin_barriers.as_ptr()
            );
        }

        unsafe
        {
            vkCmdBindDescriptorSets(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_BIND_POINT_COMPUTE,
                avg_luminance_pipeline_layout,
                0,
                1,
                &avg_luminance_descriptor_sets[image_index as usize],
                0,
                std::ptr::null()
            );
        }

        let current_image_index: u32 = current_frame_index as u32;
        unsafe
        {
            vkCmdPushConstants(
                cmd_buffers[current_frame_index],
                avg_luminance_pipeline_layout,
                VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
                0,
                core::mem::size_of::<u32>() as u32,
                &current_image_index as *const u32 as *const core::ffi::c_void
            );
        }

        let previous_image_index: u32 = previous_frame_index as u32;
        unsafe
        {
            vkCmdPushConstants(
                cmd_buffers[current_frame_index],
                avg_luminance_pipeline_layout,
                VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
                core::mem::size_of::<u32>() as u32,
                core::mem::size_of::<u32>() as u32,
                &previous_image_index as *const u32 as *const core::ffi::c_void
            );
        }

        unsafe
        {
            vkCmdBindPipeline(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_BIND_POINT_COMPUTE,
                avg_luminance_pipeline
            );
        }

        unsafe
        {
            vkCmdDispatch(
                cmd_buffers[current_frame_index],
                8,
                8,
                1
            );
        }

        let postproc_begin_buffer_barriers = [
            VkBufferMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_BUFFER_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: VK_ACCESS_SHADER_WRITE_BIT as VkAccessFlags,
                dstAccessMask: VK_ACCESS_SHADER_READ_BIT as VkAccessFlags,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                buffer: postprocessing_buffer,
                offset: avg_luminance_begin as VkDeviceSize,
                size: avg_luminance_size as VkDeviceSize
            }
        ];

        let postproc_begin_image_barriers = [
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: 0,
                dstAccessMask: VK_ACCESS_SHADER_WRITE_BIT as VkAccessFlags,
                oldLayout: VK_IMAGE_LAYOUT_UNDEFINED,
                newLayout: VK_IMAGE_LAYOUT_GENERAL,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: swapchain_imgs[image_index as usize],
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: 1,
                    baseArrayLayer: 0,
                    layerCount: 1
                }
            }
        ];

        unsafe
        {
            vkCmdPipelineBarrier(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT as VkPipelineStageFlags,
                VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT as VkPipelineStageFlags,
                0,
                0,
                core::ptr::null(),
                postproc_begin_buffer_barriers.len() as u32,
                postproc_begin_buffer_barriers.as_ptr(),
                postproc_begin_image_barriers.len() as u32,
                postproc_begin_image_barriers.as_ptr()
            );
        }

        unsafe
        {
            vkCmdBindDescriptorSets(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_BIND_POINT_COMPUTE,
                postprocessing_pipeline_layout,
                0,
                1,
                &postprocessing_descriptor_sets[image_index as usize],
                0,
                std::ptr::null()
            );
        }

        let current_image_index: u32 = current_frame_index as u32;
        unsafe
        {
            vkCmdPushConstants(
                cmd_buffers[current_frame_index],
                postprocessing_pipeline_layout,
                VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
                0,
                core::mem::size_of::<u32>() as u32,
                &current_image_index as *const u32 as *const core::ffi::c_void
            );
        }

        let exposure_value: f32 = exposure_value;
        unsafe
        {
            vkCmdPushConstants(
                cmd_buffers[current_frame_index],
                postprocessing_pipeline_layout,
                VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
                core::mem::size_of::<u32>() as u32,
                core::mem::size_of::<f32>() as u32,
                &exposure_value as *const f32 as *const core::ffi::c_void
            );
        }

        let manual_exposure_value: u32 = manual_exposure as u32;
        unsafe
        {
            vkCmdPushConstants(
                cmd_buffers[current_frame_index],
                postprocessing_pipeline_layout,
                VK_SHADER_STAGE_COMPUTE_BIT as VkShaderStageFlags,
                (core::mem::size_of::<u32>() + core::mem::size_of::<f32>()) as u32,
                core::mem::size_of::<u32>() as u32,
                &manual_exposure_value as *const u32 as *const core::ffi::c_void
            );
        }

        unsafe
        {
            vkCmdBindPipeline(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_BIND_POINT_COMPUTE,
                postprocessing_pipeline
            );
        }

        let workgroup_x = if width % 8 == 0  {width/8}  else {width/8 + 1};
        let workgroup_y = if height % 8 == 0 {height/8} else {height/8 + 1};

        unsafe
        {
            vkCmdDispatch(
                cmd_buffers[current_frame_index],
                workgroup_x as u32,
                workgroup_y as u32,
                1
            );
        }

        let postproc_end_barriers = [
            VkImageMemoryBarrier {
                sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
                pNext: std::ptr::null(),
                srcAccessMask: VK_ACCESS_SHADER_WRITE_BIT as VkAccessFlags,
                dstAccessMask: 0,
                oldLayout: VK_IMAGE_LAYOUT_GENERAL,
                newLayout: VK_IMAGE_LAYOUT_PRESENT_SRC_KHR,
                srcQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                dstQueueFamilyIndex: VK_QUEUE_FAMILY_IGNORED as u32,
                image: swapchain_imgs[image_index as usize],
                subresourceRange: VkImageSubresourceRange {
                    aspectMask: VK_IMAGE_ASPECT_COLOR_BIT as VkImageAspectFlags,
                    baseMipLevel: 0,
                    levelCount: 1,
                    baseArrayLayer: 0,
                    layerCount: 1
                }
            }
        ];

        unsafe
        {
            vkCmdPipelineBarrier(
                cmd_buffers[current_frame_index],
                VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT as VkPipelineStageFlags,
                VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT as VkPipelineStageFlags,
                0,
                0,
                core::ptr::null(),
                0,
                core::ptr::null(),
                postproc_end_barriers.len() as u32,
                postproc_end_barriers.as_ptr()
            );
        }

        let result = unsafe
        {
            vkEndCommandBuffer(
                cmd_buffers[current_frame_index]
            )
        };

        if result != VK_SUCCESS
        {
            panic!("Failed to end recording the comand buffer. Error: {:?}.", result);
        }

        //
        // Submit
        //

        {
            let wait_semaphores = [
                image_acquired_sems[current_frame_index]
            ];
            let rendering_finished_sem = [
                rendering_finished_sems[current_frame_index]
            ];
            let wait_pipeline_stages = [
                VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT as VkPipelineStageFlags
            ];
            let cmd_buffer = [
                cmd_buffers[current_frame_index]
            ];

            let submit_info = VkSubmitInfo {
                sType: VK_STRUCTURE_TYPE_SUBMIT_INFO,
                pNext: core::ptr::null(),
                waitSemaphoreCount: wait_semaphores.len() as u32,
                pWaitSemaphores: wait_semaphores.as_ptr(),
                pWaitDstStageMask: wait_pipeline_stages.as_ptr(),
                commandBufferCount: cmd_buffer.len() as u32,
                pCommandBuffers: cmd_buffer.as_ptr(),
                signalSemaphoreCount: rendering_finished_sem.len() as u32,
                pSignalSemaphores: rendering_finished_sem.as_ptr()
            };

            let result = unsafe { vkQueueSubmit(graphics_queue, 1, &submit_info, rendering_finished_fences[current_frame_index]) };

            if result != VK_SUCCESS
            {
                panic!("Failed to submit rendering commands: {:?}.", result);
            }
        }

        {
            let swapchains = [swapchain];
            let image_indices = [image_index];
            let rendering_finished_sem = [rendering_finished_sems[current_frame_index]];

            let present_info = VkPresentInfoKHR {
                sType: VK_STRUCTURE_TYPE_PRESENT_INFO_KHR,
                pNext: core::ptr::null(),
                waitSemaphoreCount: rendering_finished_sem.len() as u32,
                pWaitSemaphores: rendering_finished_sem.as_ptr(),
                swapchainCount: swapchains.len() as u32,
                pSwapchains: swapchains.as_ptr(),
                pImageIndices: image_indices.as_ptr(),
                pResults: core::ptr::null_mut()
            };

            let result = unsafe { vkQueuePresentKHR(present_queue, &present_info) };

            if result != VK_SUCCESS
            {
                if result == VK_SUBOPTIMAL_KHR || result == VK_ERROR_OUT_OF_DATE_KHR
                {
                    recreate_swapchain = true;
                }
                else
                {
                    panic!("Fatal error while submitting present: {:?}.", result);
                }
            }
        }

        frame_submitted[current_frame_index] = true;

        previous_frame_index = current_frame_index;
        current_frame_index = (current_frame_index + 1) % frame_count;
    }

    //
    // Cleanup
    //

    let result = unsafe
    {
        vkDeviceWaitIdle(device)
    };

    if result != VK_SUCCESS
    {
        panic!("Error while waiting for device before cleanup. Error: {:?}.", result);
    }

    for frame_finished_fence in rendering_finished_fences
    {
        println!("Deleting fence.");
        unsafe
        {
            vkDestroyFence(
                device,
                frame_finished_fence,
                core::ptr::null_mut()
            );
        }
    }

    for rendering_finished_sem in rendering_finished_sems
    {
        println!("Deleting semaphore.");
        unsafe
        {
            vkDestroySemaphore(
                device,
                rendering_finished_sem,
                core::ptr::null_mut()
            );
        }
    }

    for image_acquired_sem in image_acquired_sems
    {
        println!("Deleting semaphore.");
        unsafe
        {
            vkDestroySemaphore(
                device,
                image_acquired_sem,
                core::ptr::null_mut()
            );
        }
    }

    for cmd_pool in cmd_pools.iter()
    {
        println!("Deleting command pool.");
        unsafe
        {
            vkDestroyCommandPool(
                device,
                *cmd_pool,
                core::ptr::null_mut()
            );
        }
    }

    println!("Deleting descriptor pool.");
    unsafe
    {
        vkDestroyDescriptorPool(
            device,
            descriptor_pool,
            core::ptr::null_mut()
        );
    }

    println!("Deleting uniform buffer device memory");
    unsafe
    {
        vkUnmapMemory(
            device,
            uniform_buffer_memory
        );
        vkFreeMemory(
            device,
            uniform_buffer_memory,
            core::ptr::null_mut()
        );
    }

    println!("Deleting uniform buffer");
    unsafe
    {
        vkDestroyBuffer(
            device,
            uniform_buffer,
            core::ptr::null_mut()
        );
    }

    println!("Deleting dfg sampler");
    unsafe
    {
        vkDestroySampler(
            device,
            dfg_sampler,
            core::ptr::null_mut()
        );
    }

    println!("Deleting cube sampler");
    unsafe
    {
        vkDestroySampler(
            device,
            cube_sampler,
            std::ptr::null_mut()
        );
    }

    println!("Deleting sampler");
    unsafe
    {
        vkDestroySampler(
            device,
            sampler,
            core::ptr::null_mut()
        );
    }

    println!("Deleting staging buffer device memory");
    unsafe
    {
        vkFreeMemory(
            device,
            staging_buffer_memory,
            core::ptr::null_mut()
        );
    }

    println!("Deleting staging buffer");
    unsafe
    {
        vkDestroyBuffer(
            device,
            staging_buffer,
            core::ptr::null_mut()
        );
    }

    for env_image_write_view in env_image_write_views
    {
        println!("Deleting environment write image view");
        unsafe
        {
            vkDestroyImageView(
                device,
                env_image_write_view,
                std::ptr::null_mut()
            );
        }
    }

    println!("Deleting environment image view");
    unsafe
    {
        vkDestroyImageView(
            device,
            env_image_view,
            std::ptr::null_mut()
        );
    }

    println!("Deleting environment image device memory");
    unsafe
    {
        vkFreeMemory(
            device,
            env_image_memory,
            std::ptr::null_mut()
        );
    }

    println!("Deleting environment image");
    unsafe
    {
        vkDestroyImage(
            device,
            env_image,
            std::ptr::null_mut()
        );
    }

    println!("Deleting skydome image view");
    unsafe
    {
        vkDestroyImageView(
            device,
            skydome_image_view,
            std::ptr::null_mut()
        );
    }

    println!("Deleting skydome image device memory");
    unsafe
    {
        vkFreeMemory(
            device,
            skydome_image_memory,
            std::ptr::null_mut()
        );
    }

    println!("Deleting skydome image");
    unsafe
    {
        vkDestroyImage(
            device,
            skydome_image,
            std::ptr::null_mut()
        );
    }

    println!("Deleting dfg image view");
    unsafe
    {
        vkDestroyImageView(
            device,
            dfg_image_view,
            std::ptr::null_mut()
        );
    }

    println!("Deleting dfg image device memory");
    unsafe
    {
        vkFreeMemory(
            device,
            dfg_image_memory,
            std::ptr::null_mut()
        );
    }

    println!("Deleting dfg image");
    unsafe
    {
        vkDestroyImage(
            device,
            dfg_image,
            std::ptr::null_mut()
        );
    }

    for image_view in image_views
    {
        println!("Deleting image view");
        unsafe
        {
            vkDestroyImageView(
                device,
                image_view,
                core::ptr::null_mut()
            );
        }
    }

    for image_memory in image_memories
    {
        println!("Deleting image device memory");
        unsafe
        {
            vkFreeMemory(
                device,
                image_memory,
                core::ptr::null_mut()
            );
        }
    }

    for image in images
    {
        println!("Deleting image");
        unsafe
        {
            vkDestroyImage(
                device,
                image,
                core::ptr::null_mut()
            );
        }
    }

    println!("Deleting index buffer device memory");
    unsafe
    {
        vkFreeMemory(
            device,
            index_buffer_memory,
            core::ptr::null_mut()
        );
    }

    println!("Deleting index buffer");
    unsafe
    {
        vkDestroyBuffer(
            device,
            index_buffer,
            core::ptr::null_mut()
        );
    }

    println!("Deleting vertex buffer device memory");
    unsafe
    {
        vkFreeMemory(
            device,
            vertex_buffer_memory,
            core::ptr::null_mut()
        );
    }

    println!("Deleting vertex buffer");
    unsafe
    {
        vkDestroyBuffer(
            device,
            vertex_buffer,
            core::ptr::null_mut()
        );
    }

    println!("Deleting postprocessing pipeline");
    unsafe
    {
        vkDestroyPipeline(
            device,
            postprocessing_pipeline,
            core::ptr::null_mut()
        );
    }

    println!("Deleting avg luminance pipeline");
    unsafe
    {
        vkDestroyPipeline(
            device,
            avg_luminance_pipeline,
            core::ptr::null_mut()
        );
    }

    println!("Deleting dfg preinteg pipeline");
    unsafe
    {
        vkDestroyPipeline(
            device,
            dfg_compute_pipeline,
            core::ptr::null_mut()
        );
    }

    println!("Deleting env preinteg pipeline");
    unsafe
    {
        vkDestroyPipeline(
            device,
            env_compute_pipeline,
            core::ptr::null_mut()
        );
    }

    println!("Deleting skydome pipeline");
    unsafe
    {
        vkDestroyPipeline(
            device,
            skydome_pipeline,
            core::ptr::null_mut()
        );
    }

    println!("Deleting model pipeline");
    unsafe
    {
        vkDestroyPipeline(
            device,
            model_pipeline,
            core::ptr::null_mut()
        );
    }

    println!("Deleting postprocessing pipeline layout");
    unsafe
    {
        vkDestroyPipelineLayout(
            device,
            postprocessing_pipeline_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting avg luminance pipeline layout");
    unsafe
    {
        vkDestroyPipelineLayout(
            device,
            avg_luminance_pipeline_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting dfg preinteg pipeline layout");
    unsafe
    {
        vkDestroyPipelineLayout(
            device,
            dfg_compute_pipeline_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting env preinteg pipeline layout");
    unsafe
    {
        vkDestroyPipelineLayout(
            device,
            env_compute_pipeline_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting pipeline layout");
    unsafe
    {
        vkDestroyPipelineLayout(
            device,
            pipeline_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting postprocessing descriptor set layout");
    unsafe
    {
        vkDestroyDescriptorSetLayout(
            device,
            postprocessing_descriptor_set_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting avg luminance descriptor set layout");
    unsafe
    {
        vkDestroyDescriptorSetLayout(
            device,
            avg_luminance_descriptor_set_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting dfg preinteg descriptor set layout");
    unsafe
    {
        vkDestroyDescriptorSetLayout(
            device,
            dfg_preinteg_descriptor_set_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting env preinteg descriptor set layout");
    unsafe
    {
        vkDestroyDescriptorSetLayout(
            device,
            env_preinteg_descriptor_set_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting descriptor set layout");
    unsafe
    {
        vkDestroyDescriptorSetLayout(
            device,
            descriptor_set_layout,
            core::ptr::null_mut()
        );
    }

    println!("Deleting postprocessing shader module");
    unsafe
    {
        vkDestroyShaderModule(
            device,
            postprocessing_shader_module,
            std::ptr::null_mut()
        );
    }

    println!("Deleting avg luminance shader module");
    unsafe
    {
        vkDestroyShaderModule(
            device,
            avg_luminance_shader_module,
            std::ptr::null_mut()
        );
    }

    println!("Deleting dfg preinteg shader module");
    unsafe
    {
        vkDestroyShaderModule(
            device,
            dfg_preinteg_shader_module,
            std::ptr::null_mut()
        );
    }

    println!("Deleting env preinteg shader module");
    unsafe
    {
        vkDestroyShaderModule(
            device,
            env_preinteg_shader_module,
            std::ptr::null_mut()
        );
    }

    println!("Deleting skydome fragment shader module");
    unsafe
    {
        vkDestroyShaderModule(
            device,
            skydome_fragment_shader_module,
            std::ptr::null_mut()
        );
    }

    println!("Deleting skydome vertex shader module");
    unsafe
    {
        vkDestroyShaderModule(
            device,
            skydome_vertex_shader_module,
            std::ptr::null_mut()
        );
    }

    println!("Deleting model fragment shader module");
    unsafe
    {
        vkDestroyShaderModule(
            device,
            model_fragment_shader_module,
            core::ptr::null_mut()
        );
    }

    println!("Deleting model vertex shader module");
    unsafe
    {
        vkDestroyShaderModule(
            device,
            model_vertex_shader_module,
            core::ptr::null_mut()
        );
    }

    println!("Deleting postprocessing buffer device memory");
    unsafe
    {
        vkFreeMemory(
            device,
            postprocessing_buffer_memory,
            core::ptr::null_mut()
        );
    }

    println!("Deleting postprocessing buffer");
    unsafe
    {
        vkDestroyBuffer(
            device,
            postprocessing_buffer,
            core::ptr::null_mut()
        );
    }

    for avg_luminance_image_view in avg_luminance_image_views
    {
        println!("Deleting avg luminance image view");
        unsafe
        {
            vkDestroyImageView(
                device,
                avg_luminance_image_view,
                core::ptr::null_mut()
            );
        }
    }

    for avg_luminance_image_memory in avg_luminance_image_memories
    {
        println!("Deleting avg luminance image device memory");
        unsafe
        {
            vkFreeMemory(
                device,
                avg_luminance_image_memory,
                core::ptr::null_mut()
            );
        }
    }

    for avg_luminance_image in avg_luminance_images
    {
        println!("Deleting avg luminance image");
        unsafe
        {
            vkDestroyImage(
                device,
                avg_luminance_image,
                core::ptr::null_mut()
            );
        }
    }

    unsafe
    {
        destroy_framebuffers_and_render_targets(
            device,
            &mut swapchain_img_views,
            &mut color_buffers,
            &mut color_buffer_memories,
            &mut color_buffer_views,
            &mut depth_buffers,
            &mut depth_buffer_memories,
            &mut depth_buffer_views,
            &mut framebuffers
        );
    }

    println!("Deleting render pass.");
    unsafe
    {
        vkDestroyRenderPass(device,
            render_pass,
            core::ptr::null_mut()
        );
    }

    println!("Deleting swapchain.");
    unsafe
    {
        vkDestroySwapchainKHR(
            device,
            swapchain,
            core::ptr::null_mut()
        );
    }

    println!("Deleting device.");
    unsafe
    {
        vkDestroyDevice(
            device,
            core::ptr::null()
        );
    }

    println!("Deleting surface.");
    unsafe
    {
        vkDestroySurfaceKHR(
            instance,
            surface,
            core::ptr::null()
        );
    }

    println!("Deleting instance.");
    unsafe
    {
        vkDestroyInstance(
            instance,
            core::ptr::null()
        );
    }
}
