#![allow(
    dead_code,
    unsafe_op_in_unsafe_fn,
    unused_variables,
    clippy::too_many_arguments,
    clippy::unnecessary_wraps
)]

use std::mem::size_of;
use std::ptr::copy_nonoverlapping as memcpy;
use std::time::Instant;

use anyhow::{Result, anyhow};
use cgmath::{Deg, InnerSpace, vec3};
use log::*;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::ExtDebugUtilsExtensionInstanceCommands;
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;
use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
use vulkanalia::window as vk_window;
use winit::window::Window;

use rustKPC::collision;
use rustKPC::egui_renderer;
use rustKPC::lights;
use rustKPC::occlusion_culling;
use rustKPC::camera::{Camera, InputState};
use rustKPC::types::*;
use rustKPC::vulkan;
use rustKPC::world_chooser::{LoadingState, WorldChooser};
use rustKPC::world_loader::load_dat_model;

/// Our Vulkan app.
pub struct App {
    pub entry: Entry,
    pub instance: Instance,
    pub data: AppData,
    pub device: Device,
    pub frame: usize,
    pub resized: bool,
    pub start: Instant,
    pub models: usize,
    pub camera: Camera,
    pub input: InputState,
    pub on_ground: bool,
    pub world_chooser: WorldChooser,
    pub current_world: String,
    pub loading_state: LoadingState,
    pub player_mode: collision::PlayerMode,
    pub height_provider: Box<dyn collision::HeightProvider>,
    pub mesh_provider: Option<collision::MeshHeightProvider>,
    // Physics
    pub z_vel: f32,
    pub is_free_cam: bool,
    pub show_fps: bool,
    pub vsync: bool,
    pub freeze_culling: bool,
    pub saved_player_camera: Option<Camera>,
    pub eye_offset_walk: f32,
    pub player_fov: f32,
    pub fps: f32,
    pub occlusion_culler: occlusion_culling::OcclusionCuller,
    // Lighting
    pub world_lights: Vec<lights::Light>,
    /// Cached lighting UBO (rebuilt only when lights change, not every frame)
    pub cached_light_ubo: LightingUBO,
}

impl App {
    /// Creates our Vulkan app.
    pub unsafe fn create(window: &Window) -> Result<Self> {
        let loader = LibloadingLoader::new(LIBRARY)?;
        let entry = Entry::new(loader).map_err(|b| anyhow!("{}", b))?;
        let mut data = AppData::default();
        let instance = vulkan::create_instance(window, &entry, &mut data)?;
        data.surface = vk_window::create_surface(&instance, &window, &window)?;
        vulkan::pick_physical_device(&instance, &mut data)?;
        let device = vulkan::create_logical_device(&entry, &instance, &mut data)?;
        vulkan::create_swapchain(window, &instance, &device, &mut data)?;
        vulkan::create_swapchain_image_views(&device, &mut data)?;
        vulkan::create_render_pass(&instance, &device, &mut data)?;
        vulkan::create_descriptor_set_layout(&device, &mut data)?;
        vulkan::create_pipeline(&device, &mut data)?;
        vulkan::create_command_pools(&instance, &device, &mut data)?;
        vulkan::create_color_objects(&instance, &device, &mut data)?;
        vulkan::create_depth_objects(&instance, &device, &mut data)?;
        vulkan::create_framebuffers(&device, &mut data)?;

        let initial_lights = load_dat_model(&mut data, "REZ/WORLDS/REALM1/R1M1A.DAT", 0, 0.01)?;

        vulkan::create_texture_image(&instance, &device, &mut data)?;
        vulkan::create_texture_image_view(&device, &mut data)?;
        vulkan::create_texture_sampler(&device, &mut data)?;

        vulkan::create_vertex_buffer(&instance, &device, &mut data)?;
        vulkan::create_index_buffer(&instance, &device, &mut data)?;
        vulkan::create_uniform_buffers(&instance, &device, &mut data)?;
        vulkan::create_descriptor_pool(&device, &mut data)?;
        vulkan::create_descriptor_sets(&device, &mut data)?;
        vulkan::create_command_buffers(&device, &mut data)?;
        vulkan::create_sync_objects(&device, &mut data)?;
        let initial_world = "REZ/WORLDS/REALM1/R1M1A.DAT".to_string();

        let (initial_height_provider, initial_mesh_provider) =
            if !data.vertices.is_empty() && !data.indices.is_empty() {
                let positions = data.vertices.iter().map(|v| v.pos).collect::<Vec<_>>();
                let indices = data.indices.clone();
                let mesh =
                    collision::MeshHeightProvider::new(positions.clone(), indices.clone());
                (
                    Box::new(mesh.clone()) as Box<dyn collision::HeightProvider>,
                    Some(mesh),
                )
            } else {
                (
                    Box::new(collision::FlatGround) as Box<dyn collision::HeightProvider>,
                    None,
                )
            };

        let initial_occlusion_culler = {
            let mut culler = occlusion_culling::OcclusionCuller::new();
            let positions: Vec<[f32; 3]> = data
                .vertices
                .iter()
                .map(|v| [v.pos.x, v.pos.y, v.pos.z])
                .collect();
            let groups: Vec<(u32, u32)> = data
                .draw_groups
                .iter()
                .map(|g| (g.first_index, g.index_count))
                .collect();
            culler.build_from_groups(&positions, &data.indices, &groups);
            culler
        };

        let app = Self {
            entry,
            instance,
            data,
            device,
            frame: 0,
            resized: false,
            start: Instant::now(),
            models: 1,
            camera: Camera::default(),
            input: InputState::default(),
            world_chooser: WorldChooser::new(),
            current_world: initial_world,
            loading_state: LoadingState::Ready,
            player_mode: collision::PlayerMode::Walk,
            height_provider: initial_height_provider,
            mesh_provider: initial_mesh_provider,
            z_vel: 0.0,
            is_free_cam: false,
            show_fps: true,
            vsync: true,
            freeze_culling: true,
            saved_player_camera: None,
            eye_offset_walk: 0.4,
            player_fov: 60.0,
            fps: 0.0,
            on_ground: false,
            occlusion_culler: initial_occlusion_culler,
            world_lights: initial_lights.clone(),
            cached_light_ubo: Self::build_light_ubo(&initial_lights),
        };

        // Upload the full light UBO to all persistently mapped swapchain buffers
        app.upload_light_ubo_to_all();

        Ok(app)
    }

    /// Build a LightingUBO from a list of lights, pre-computing GPU-friendly values.
    /// Called once when lights change (map load), not every frame.
    fn build_light_ubo(world_lights: &[lights::Light]) -> LightingUBO {
        let mut ubo = LightingUBO::default();
        let count = world_lights.len().min(MAX_LIGHTS);
        ubo.light_count = count as u32;
        ubo.ambient = [0.4, 0.4, 0.4, 0.0];

        for (i, l) in world_lights.iter().take(count).enumerate() {
            let r_sq = l.radius * l.radius;
            let inv_r = if l.radius > 0.0 { 1.0 / l.radius } else { 0.0 };
            ubo.lights[i] = GpuLight {
                position_radius_sq: [l.position[0], l.position[1], l.position[2], r_sq],
                color_intensity: [
                    l.color[0] * l.intensity,
                    l.color[1] * l.intensity,
                    l.color[2] * l.intensity,
                    inv_r,
                ],
            };
        }
        ubo
    }

    /// Upload the full cached light UBO to all swapchain images' persistently mapped memory.
    /// Called once when lights change (map load / swapchain recreate).
    unsafe fn upload_light_ubo_to_all(&self) {
        for mapped in &self.data.light_uniform_buffers_mapped {
            memcpy(&self.cached_light_ubo, (*mapped).cast(), 1);
        }
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(
        &mut self,
        window: &Window,
        egui_renderer: &mut egui_renderer::EguiRenderer,
        egui_primitives: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
    ) -> Result<()> {
        let in_flight_fence = self.data.in_flight_fences[self.frame];

        self.device
            .wait_for_fences(&[in_flight_fence], true, u64::MAX)?;

        let result = self.device.acquire_next_image_khr(
            self.data.swapchain,
            u64::MAX,
            self.data.image_available_semaphores[self.frame],
            vk::Fence::null(),
        );

        let image_index = match result {
            Ok((image_index, _)) => image_index as usize,
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                return self.recreate_swapchain(window, egui_renderer)
            }
            Err(e) => return Err(anyhow!(e)),
        };

        let image_in_flight = self.data.images_in_flight[image_index];
        if !image_in_flight.is_null() {
            self.device
                .wait_for_fences(&[image_in_flight], true, u64::MAX)?;
        }

        self.data.images_in_flight[image_index] = in_flight_fence;

        self.update_command_buffer(image_index, egui_renderer, egui_primitives, pixels_per_point)?;
        self.update_uniform_buffer(image_index)?;

        let wait_semaphores = &[self.data.image_available_semaphores[self.frame]];
        let wait_stages = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = &[self.data.command_buffers[image_index]];
        let signal_semaphores = &[self.data.render_finished_semaphores[self.frame]];
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores);

        self.device.reset_fences(&[in_flight_fence])?;

        self.device
            .queue_submit(self.data.graphics_queue, &[submit_info], in_flight_fence)?;

        let swapchains = &[self.data.swapchain];
        let image_indices = &[image_index as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        let result = self
            .device
            .queue_present_khr(self.data.present_queue, &present_info);
        let changed = result == Ok(vk::SuccessCode::SUBOPTIMAL_KHR)
            || result == Err(vk::ErrorCode::OUT_OF_DATE_KHR);
        if self.resized || changed {
            self.resized = false;
            self.recreate_swapchain(window, egui_renderer)?;
        } else if let Err(e) = result {
            return Err(anyhow!(e));
        }

        self.frame = (self.frame + 1) % self.data.swapchain_image_count as usize;

        Ok(())
    }

    /// Updates a command buffer for our Vulkan app.
    #[rustfmt::skip]
    unsafe fn update_command_buffer(
        &mut self,
        image_index: usize,
        egui_renderer: &mut egui_renderer::EguiRenderer,
        egui_primitives: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
    ) -> Result<()> {
        let command_pool = self.data.command_pools[image_index];
        self.device.reset_command_pool(command_pool, vk::CommandPoolResetFlags::empty())?;

        let command_buffer = self.data.command_buffers[image_index];

        let info = vk::CommandBufferBeginInfo::builder().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        self.device.begin_command_buffer(command_buffer, &info)?;

        let render_area = vk::Rect2D::builder()
            .offset(vk::Offset2D::default())
            .extent(self.data.swapchain_extent);

        let clear_color = if matches!(self.loading_state, LoadingState::Loading(_)) {
            [0.08, 0.08, 0.12, 1.0]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };

        let color_clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: clear_color,
            },
        };

        let depth_clear_value = vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
        };

        let clear_values = &[color_clear_value, depth_clear_value];
        let info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.data.render_pass)
            .framebuffer(self.data.framebuffers[image_index])
            .render_area(render_area)
            .clear_values(clear_values);

        self.device.cmd_begin_render_pass(command_buffer, &info, vk::SubpassContents::SECONDARY_COMMAND_BUFFERS);

        // Run frustum culling
        {
            let (cull_view, cull_fov) = if self.is_free_cam && self.freeze_culling {
                if let Some(ref saved) = self.saved_player_camera {
                    (saved.view_matrix_with_offset(self.eye_offset_walk), self.player_fov)
                } else {
                    (self.camera.view_matrix_with_offset(0.0), 45.0)
                }
            } else {
                let eye_offset = if self.player_mode == collision::PlayerMode::Walk && !self.is_free_cam {
                    self.eye_offset_walk
                } else { 0.0 };
                let fov_deg = if self.player_mode == collision::PlayerMode::Walk && !self.is_free_cam {
                    self.player_fov
                } else { 45.0 };
                (self.camera.view_matrix_with_offset(eye_offset), fov_deg)
            };

            let raw_proj = cgmath::perspective(
                    Deg(cull_fov),
                    self.data.swapchain_extent.width as f32 / self.data.swapchain_extent.height as f32,
                    0.01,
                    1000.0,
                );
            let vp = raw_proj * cull_view;
            let frustum = occlusion_culling::Frustum::from_view_proj(&vp);
            self.occlusion_culler.cull(&frustum);
        }

        if self.loading_state == LoadingState::Ready {
            let secondary_command_buffers = (0..self.models)
                .map(|i| self.update_secondary_command_buffer(image_index, i))
                .collect::<Result<Vec<_>, _>>()?;
            self.device.cmd_execute_commands(command_buffer, &secondary_command_buffers[..]);
        }

        self.device.cmd_end_render_pass(command_buffer);

        egui_renderer.render(
            &self.instance,
            &self.device,
            self.data.physical_device,
            command_buffer,
            image_index,
            egui_primitives,
            pixels_per_point,
        )?;

        self.device.end_command_buffer(command_buffer)?;

        Ok(())
    }

    /// Updates a secondary command buffer for our Vulkan app.
    #[rustfmt::skip]
    unsafe fn update_secondary_command_buffer(
        &mut self,
        image_index: usize,
        model_index: usize,
    ) -> Result<vk::CommandBuffer> {
        let command_buffers = &mut self.data.secondary_command_buffers[image_index];
        while model_index >= command_buffers.len() {
            let allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_pool(self.data.command_pools[image_index])
                .level(vk::CommandBufferLevel::SECONDARY)
                .command_buffer_count(1);

            let command_buffer = self.device.allocate_command_buffers(&allocate_info)?[0];
            command_buffers.push(command_buffer);
        }

        let command_buffer = command_buffers[model_index];

        let model = Mat4::from_scale(1.0);
        let model_bytes = std::slice::from_raw_parts(&model as *const Mat4 as *const u8, size_of::<Mat4>());

        let opacity = 1.0f32;
        let opacity_bytes = &opacity.to_ne_bytes()[..];

        let inheritance_info = vk::CommandBufferInheritanceInfo::builder()
            .render_pass(self.data.render_pass)
            .subpass(0)
            .framebuffer(self.data.framebuffers[image_index]);

        let info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE)
            .inheritance_info(&inheritance_info);

        self.device.begin_command_buffer(command_buffer, &info)?;

        self.device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.data.pipeline);
        self.device.cmd_bind_vertex_buffers(command_buffer, 0, &[self.data.vertex_buffer], &[0]);
        self.device.cmd_bind_index_buffer(command_buffer, self.data.index_buffer, 0, vk::IndexType::UINT32);

        self.device.cmd_push_constants(
            command_buffer,
            self.data.pipeline_layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            model_bytes,
        );
        self.device.cmd_push_constants(
            command_buffer,
            self.data.pipeline_layout,
            vk::ShaderStageFlags::FRAGMENT,
            64,
            opacity_bytes,
        );

        if self.data.draw_groups.is_empty() {
            self.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.data.pipeline_layout,
                0,
                &[self.data.descriptor_sets[image_index]],
                &[],
            );
            self.device.cmd_draw_indexed(command_buffer, self.data.indices.len() as u32, 1, 0, 0, 0);
        } else {
            for (group_idx, group) in self.data.draw_groups.iter().enumerate() {
                if !self.occlusion_culler.is_visible(group_idx) {
                    continue;
                }
                let descriptor_set = if group.texture_index < self.data.level_textures.len()
                    && !self.data.level_textures[group.texture_index].descriptor_sets.is_empty()
                {
                    self.data.level_textures[group.texture_index].descriptor_sets[image_index]
                } else {
                    self.data.descriptor_sets[image_index]
                };

                self.device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.data.pipeline_layout,
                    0,
                    &[descriptor_set],
                    &[],
                );

                self.device.cmd_draw_indexed(
                    command_buffer,
                    group.index_count,
                    1,
                    group.first_index,
                    0,
                    0
                );
            }
        }

        self.device.end_command_buffer(command_buffer)?;

        Ok(command_buffer)
    }

    /// Updates camera position based on input state
    pub fn update_camera(&mut self, dt: f32) {
        let speed = if self.player_mode == collision::PlayerMode::Flying {
            FLY_SPEED * dt
        } else {
            WALK_SPEED * dt
        };
        let front_flat = {
            let f = self.camera.front();
            vec3(f.x, f.y, 0.0).normalize()
        };
        let right = self.camera.right();
        let prev_pos = self.camera.position;

        self.on_ground = false;
        if self.input.forward {
            self.camera.position = self.camera.position + front_flat * speed;
        }
        if self.input.backward {
            self.camera.position = self.camera.position - front_flat * speed;
        }
        if self.input.left {
            self.camera.position = self.camera.position - right * speed;
        }
        if self.input.right {
            self.camera.position = self.camera.position + right * speed;
        }
        if self.player_mode == collision::PlayerMode::Flying {
            if self.input.up {
                self.camera.position.z += speed;
            }
            if self.input.down {
                self.camera.position.z -= speed;
            }
            self.z_vel = 0.0;
        } else {
            const GRAVITY: f32 = 9.8 * 3.0;
            self.z_vel -= GRAVITY * dt;
            self.z_vel = self.z_vel.max(-8.0);
            if self.input.up && self.on_ground {
                self.z_vel = 7.5;
                self.input.up = false;
                self.on_ground = false;
            }
            self.camera.position.z += self.z_vel * dt;
        }

        if self.player_mode == collision::PlayerMode::Walk {
            if let Some(mesh) = &self.mesh_provider {
                mesh.resolve_player_movement(prev_pos, &mut self.camera.position, 0.25);
            }
            let _before_z = self.camera.position.z;
            collision::resolve_player_collision(
                &mut self.camera.position,
                self.height_provider.as_ref(),
                0.25,
                0.5,
            );
            let ground_z_cur = self.height_provider.ground_height(
                self.camera.position.x,
                self.camera.position.y,
                Some(self.camera.position.z),
            );
            let ground_z_prev = self.height_provider.ground_height(
                self.camera.position.x,
                self.camera.position.y,
                Some(prev_pos.z),
            );
            let ground_z = if prev_pos.z >= ground_z_prev + 0.5 - 0.05
                && self.camera.position.z < ground_z_prev + 0.5
            {
                ground_z_prev.max(ground_z_cur)
            } else {
                ground_z_cur
            };
            let min_z = ground_z + 0.5;
            if self.camera.position.z < min_z {
                let diff = min_z - self.camera.position.z;
                if diff <= 0.5 {
                    self.camera.position.z = min_z;
                } else {
                    self.camera.position.z += 0.3;
                }
                if self.z_vel < 0.0 {
                    self.z_vel = 0.0;
                }
                self.on_ground = true;
            } else if self.camera.position.z < min_z + 0.15 && self.z_vel <= 0.0 {
                self.camera.position.z = min_z;
                self.z_vel = 0.0;
                self.on_ground = true;
            }
        }
    }

    /// Run the egui UI
    pub fn run_ui(&mut self, ctx: &egui::Context, mouse_locked: &mut bool) {
        // Loading screen
        if let LoadingState::Loading(ref map_name) = self.loading_state {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 30)))
                .show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() / 2.0 - 40.0);
                            ui.heading(
                                egui::RichText::new("Loading...")
                                    .size(32.0)
                                    .color(egui::Color32::WHITE),
                            );
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(map_name)
                                    .size(24.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                        });
                    });
                });
            return;
        }

        // FPS counter
        if self.show_fps {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("fps_overlay"),
            ));
            let text = format!("{:.0} FPS", self.fps);
            let font_id = egui::FontId::proportional(16.0);
            let galley = painter.layout_no_wrap(text, font_id, egui::Color32::YELLOW);
            let screen = ctx.screen_rect();
            let padding = egui::vec2(8.0, 4.0);
            let text_pos = egui::pos2(
                screen.max.x - galley.size().x - padding.x - 10.0,
                5.0 + padding.y,
            );
            let bg_rect = egui::Rect::from_min_size(text_pos - padding, galley.size() + padding * 2.0);
            painter.rect_filled(
                bg_rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 140),
            );
            painter.galley(text_pos, galley, egui::Color32::YELLOW);
        }

        // Draw FOV debug rays when freeze culling is active in free cam
        if self.is_free_cam && self.freeze_culling {
            if let Some(ref saved) = self.saved_player_camera {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("fov_rays"),
                ));
                let screen = ctx.screen_rect();
                let sw = self.data.swapchain_extent.width as f32;
                let sh = self.data.swapchain_extent.height as f32;
                let aspect = sw / sh;

                let eye_offset = 0.0f32;
                let free_view = self.camera.view_matrix_with_offset(eye_offset);
                #[rustfmt::skip]
                let correction = Mat4::new(
                    1.0,  0.0,       0.0, 0.0,
                    0.0, -1.0,       0.0, 0.0,
                    0.0,  0.0, 1.0 / 2.0, 0.0,
                    0.0,  0.0, 1.0 / 2.0, 1.0,
                );
                let free_proj =
                    correction * cgmath::perspective(Deg(45.0), aspect, 0.01, 1000.0);
                let vp = free_proj * free_view;

                let saved_eye = vec3(
                    saved.position.x,
                    saved.position.y,
                    saved.position.z + self.eye_offset_walk,
                );
                let half_fov_rad = (self.player_fov * 0.5f32).to_radians();
                let saved_front = saved.front();
                let saved_right = saved.right();
                let ray_len = 50.0f32;

                let front_flat = vec3(saved_front.x, saved_front.y, 0.0).normalize();
                let right_flat = vec3(saved_right.x, saved_right.y, 0.0).normalize();
                let left_dir = (front_flat * half_fov_rad.cos()
                    - right_flat * half_fov_rad.sin())
                .normalize();
                let right_dir = (front_flat * half_fov_rad.cos()
                    + right_flat * half_fov_rad.sin())
                .normalize();

                let origin = saved_eye;
                let left_end = origin + left_dir * ray_len;
                let right_end = origin + right_dir * ray_len;

                let project = |world_pos: Vec3| -> Option<egui::Pos2> {
                    let p = vp * cgmath::vec4(world_pos.x, world_pos.y, world_pos.z, 1.0);
                    if p.w <= 0.001 {
                        return None;
                    }
                    let ndc_x = p.x / p.w;
                    let ndc_y = p.y / p.w;
                    let sx = (ndc_x + 1.0) * 0.5 * screen.width() + screen.min.x;
                    let sy = (ndc_y + 1.0) * 0.5 * screen.height() + screen.min.y;
                    Some(egui::pos2(sx, sy))
                };

                let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 255, 0));
                if let (Some(p0), Some(p1)) = (project(origin), project(left_end)) {
                    painter.line_segment([p0, p1], stroke);
                }
                if let (Some(p0), Some(p1)) = (project(origin), project(right_end)) {
                    painter.line_segment([p0, p1], stroke);
                }
                let front_end = origin + front_flat * ray_len;
                let front_stroke =
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 180, 0));
                if let (Some(p0), Some(p1)) = (project(origin), project(front_end)) {
                    painter.line_segment([p0, p1], front_stroke);
                }
            }
        }

        // World chooser panel
        if self.world_chooser.visible {
            egui::TopBottomPanel::top("world_chooser")
                .frame(
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0))
                        .inner_margin(egui::Margin::same(10.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(
                            egui::RichText::new("World Select").color(egui::Color32::WHITE),
                        );
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new("Select a map to load:")
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                    });

                    // Settings toggles
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Player Mode:")
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        let prev = self.is_free_cam;
                        if ui
                            .checkbox(&mut self.is_free_cam, "Free Camera")
                            .changed()
                        {
                            if self.is_free_cam != prev {
                                if self.is_free_cam && self.freeze_culling {
                                    self.saved_player_camera = Some(self.camera.clone());
                                } else if !self.is_free_cam && self.freeze_culling {
                                    if let Some(saved) = self.saved_player_camera.take() {
                                        self.camera = saved;
                                    }
                                }
                                self.player_mode = if self.is_free_cam {
                                    collision::PlayerMode::Flying
                                } else {
                                    collision::PlayerMode::Walk
                                };
                                if !self.is_free_cam {
                                    self.z_vel = 0.0;
                                    let gz = self.height_provider.ground_height(
                                        self.camera.position.x,
                                        self.camera.position.y,
                                        Some(self.camera.position.z),
                                    );
                                    let min_z = gz + 0.5;
                                    if self.camera.position.z < min_z {
                                        self.camera.position.z = min_z;
                                    }
                                    self.on_ground = true;
                                }
                            }
                        }
                        ui.add_space(20.0);
                        ui.checkbox(&mut self.show_fps, "Show FPS");
                        ui.add_space(20.0);
                        let prev_vsync = self.vsync;
                        ui.checkbox(&mut self.vsync, "VSync");
                        if self.vsync != prev_vsync {
                            self.data.vsync = self.vsync;
                            self.resized = true;
                        }
                        ui.add_space(20.0);
                        let prev_freeze = self.freeze_culling;
                        if ui
                            .checkbox(&mut self.freeze_culling, "Freeze Culling")
                            .changed()
                        {
                            if self.freeze_culling && self.is_free_cam {
                                self.saved_player_camera = Some(self.camera.clone());
                            } else if !self.freeze_culling {
                                self.saved_player_camera = None;
                            }
                        }
                    });

                    ui.add_space(5.0);
                    ui.separator();
                    ui.add_space(5.0);

                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            let worlds = self.world_chooser.worlds.clone();
                            for (i, world_path) in worlds.iter().enumerate() {
                                let display_name =
                                    WorldChooser::get_world_display_name(world_path);
                                let is_selected = i == self.world_chooser.selected_index;
                                let is_current = *world_path == self.current_world;

                                let text = if is_current {
                                    egui::RichText::new(format!(
                                        "{} (current)",
                                        display_name
                                    ))
                                    .color(egui::Color32::LIGHT_GREEN)
                                } else {
                                    egui::RichText::new(&display_name).color(
                                        if is_selected {
                                            egui::Color32::YELLOW
                                        } else {
                                            egui::Color32::WHITE
                                        },
                                    )
                                };

                                let orig_style = ui.style().clone();
                                let mut style = (*orig_style).clone();
                                style.visuals.selection.bg_fill =
                                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0);
                                ui.set_style(std::sync::Arc::new(style));
                                let response = ui.selectable_label(is_selected, text);
                                ui.set_style(orig_style);

                                if response.clicked() {
                                    self.world_chooser.select_index(i);
                                    println!("Selected: {}", display_name);
                                }

                                if response.double_clicked() {
                                    println!("Loading: {}", display_name);
                                    if let Some(path) = self.world_chooser.confirm_selection() {
                                        self.world_chooser.pending_load = Some(path);
                                        *mouse_locked = true;
                                    }
                                }
                            }
                        });

                    ui.add_space(5.0);
                    ui.separator();
                    ui.add_space(5.0);

                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new("Load Selected")
                                    .fill(egui::Color32::TRANSPARENT),
                            )
                            .clicked()
                        {
                            if let Some(path) = self.world_chooser.confirm_selection() {
                                self.world_chooser.pending_load = Some(path);
                                *mouse_locked = true;
                            }
                        }

                        ui.add_space(10.0);

                        let selected_name = self
                            .world_chooser
                            .worlds
                            .get(self.world_chooser.selected_index)
                            .map(|p| WorldChooser::get_world_display_name(p))
                            .unwrap_or_default();
                        ui.label(
                            egui::RichText::new(format!("Selected: {}", selected_name))
                                .color(egui::Color32::GRAY),
                        );
                    });
                });
        }
    }

    /// Reloads a new world file
    pub unsafe fn reload_world(
        &mut self,
        world_path: &str,
        _egui_renderer: &mut egui_renderer::EguiRenderer,
    ) -> Result<()> {
        info!("Loading world: {}", world_path);

        self.device.device_wait_idle()?;

        // Destroy old level textures
        for texture in &self.data.level_textures {
            self.device.destroy_image_view(texture.view, None);
            self.device.free_memory(texture.memory, None);
            self.device.destroy_image(texture.image, None);
        }
        self.data.level_textures.clear();

        // Destroy old vertex/index buffers
        self.device
            .free_memory(self.data.index_buffer_memory, None);
        self.device.destroy_buffer(self.data.index_buffer, None);
        self.device
            .free_memory(self.data.vertex_buffer_memory, None);
        self.device.destroy_buffer(self.data.vertex_buffer, None);

        // Destroy old texture resources
        self.device.destroy_sampler(self.data.texture_sampler, None);
        self.device
            .destroy_image_view(self.data.texture_image_view, None);
        self.device
            .free_memory(self.data.texture_image_memory, None);
        self.device.destroy_image(self.data.texture_image, None);

        // Clear old data
        self.data.vertices.clear();
        self.data.indices.clear();
        self.data.draw_groups.clear();

        // Load new world
        self.world_lights = load_dat_model(&mut self.data, world_path, 0, 0.01)?;
        self.cached_light_ubo = Self::build_light_ubo(&self.world_lights);
        self.upload_light_ubo_to_all();

        // Install mesh-backed height provider
        if !self.data.vertices.is_empty() && !self.data.indices.is_empty() {
            let positions = self.data.vertices.iter().map(|v| v.pos).collect::<Vec<_>>();
            let indices = self.data.indices.clone();
            let mesh = collision::MeshHeightProvider::new(positions.clone(), indices.clone());
            self.height_provider = Box::new(mesh.clone());
            self.mesh_provider = Some(mesh);
        } else {
            self.height_provider = Box::new(collision::FlatGround);
            self.mesh_provider = None;
        }

        // Recreate texture resources
        vulkan::create_texture_image(&self.instance, &self.device, &mut self.data)?;
        vulkan::create_texture_image_view(&self.device, &mut self.data)?;
        vulkan::create_texture_sampler(&self.device, &mut self.data)?;

        // Recreate buffers
        vulkan::create_vertex_buffer(&self.instance, &self.device, &mut self.data)?;
        vulkan::create_index_buffer(&self.instance, &self.device, &mut self.data)?;

        // Recreate descriptor sets
        self.device
            .destroy_descriptor_pool(self.data.descriptor_pool, None);
        vulkan::create_descriptor_pool(&self.device, &mut self.data)?;
        vulkan::create_descriptor_sets(&self.device, &mut self.data)?;

        // Reset camera
        self.camera = Camera::default();

        // Update current world
        self.current_world = world_path.to_string();

        // Rebuild occlusion culling AABBs
        {
            let positions: Vec<[f32; 3]> = self
                .data
                .vertices
                .iter()
                .map(|v| [v.pos.x, v.pos.y, v.pos.z])
                .collect();
            let groups: Vec<(u32, u32)> = self
                .data
                .draw_groups
                .iter()
                .map(|g| (g.first_index, g.index_count))
                .collect();
            self.occlusion_culler
                .build_from_groups(&positions, &self.data.indices, &groups);
        }

        info!("World loaded successfully: {}", world_path);
        Ok(())
    }

    /// Updates the uniform buffer object for our Vulkan app.
    unsafe fn update_uniform_buffer(&mut self, image_index: usize) -> Result<()> {
        let eye_offset = if self.player_mode == collision::PlayerMode::Walk && !self.is_free_cam {
            self.eye_offset_walk
        } else {
            0.0
        };
        let view = self.camera.view_matrix_with_offset(eye_offset);

        // Correction matrix is constant — computed once with const values
        #[rustfmt::skip]
        const CORRECTION: [[f32; 4]; 4] = [
            [1.0,  0.0,  0.0, 0.0],
            [0.0, -1.0,  0.0, 0.0],
            [0.0,  0.0,  0.5, 0.0],
            [0.0,  0.0,  0.5, 1.0],
        ];
        let correction: Mat4 = CORRECTION.into();

        let fov_deg = if self.player_mode == collision::PlayerMode::Walk && !self.is_free_cam {
            self.player_fov
        } else {
            45.0
        };

        let proj = correction
            * cgmath::perspective(
                Deg(fov_deg),
                self.data.swapchain_extent.width as f32
                    / self.data.swapchain_extent.height as f32,
                0.01,
                1000.0,
            );

        let ubo = UniformBufferObject { view, proj };

        // Write directly to persistently mapped memory (no map/unmap overhead)
        memcpy(&ubo, self.data.uniform_buffers_mapped[image_index].cast(), 1);

        // Upload lighting UBO: only camera_pos changes per frame, write it directly
        {
            let cam_pos = self.camera.position;
            // Write camera_pos at offset 0 of the mapped light UBO (first 16 bytes)
            let cam_data: [f32; 4] = [cam_pos.x, cam_pos.y, cam_pos.z, 0.0];
            let dst = self.data.light_uniform_buffers_mapped[image_index] as *mut [f32; 4];
            *dst = cam_data;
        }

        Ok(())
    }

    /// Recreates the swapchain for our Vulkan app.
    #[rustfmt::skip]
    pub unsafe fn recreate_swapchain(&mut self, window: &Window, egui_renderer: &mut egui_renderer::EguiRenderer) -> Result<()> {
        self.device.device_wait_idle()?;
        self.destroy_swapchain();
        vulkan::create_swapchain(window, &self.instance, &self.device, &mut self.data)?;
        vulkan::create_swapchain_image_views(&self.device, &mut self.data)?;
        vulkan::create_render_pass(&self.instance, &self.device, &mut self.data)?;
        vulkan::create_pipeline(&self.device, &mut self.data)?;
        vulkan::create_color_objects(&self.instance, &self.device, &mut self.data)?;
        vulkan::create_depth_objects(&self.instance, &self.device, &mut self.data)?;
        vulkan::create_framebuffers(&self.device, &mut self.data)?;
        vulkan::create_uniform_buffers(&self.instance, &self.device, &mut self.data)?;
        // Re-upload light UBO into freshly mapped buffers
        self.upload_light_ubo_to_all();
        vulkan::create_descriptor_pool(&self.device, &mut self.data)?;
        vulkan::create_descriptor_sets(&self.device, &mut self.data)?;
        vulkan::create_command_buffers(&self.device, &mut self.data)?;
        self.data.images_in_flight.resize(self.data.swapchain_images.len(), vk::Fence::null());
        
        egui_renderer.resize(
            &self.device,
            &self.data.swapchain_image_views,
            self.data.swapchain_extent.width,
            self.data.swapchain_extent.height,
        )?;
        
        Ok(())
    }

    /// Destroys our Vulkan app.
    #[rustfmt::skip]
    pub unsafe fn destroy(&mut self) {
        self.device.device_wait_idle().unwrap();

        self.destroy_swapchain();

        for texture in &self.data.level_textures {
            self.device.destroy_image_view(texture.view, None);
            self.device.free_memory(texture.memory, None);
            self.device.destroy_image(texture.image, None);
        }
        self.data.level_textures.clear();

        self.data.in_flight_fences.iter().for_each(|f| self.device.destroy_fence(*f, None));
        self.data.render_finished_semaphores.iter().for_each(|s| self.device.destroy_semaphore(*s, None));
        self.data.image_available_semaphores.iter().for_each(|s| self.device.destroy_semaphore(*s, None));
        self.data.command_pools.iter().for_each(|p| self.device.destroy_command_pool(*p, None));
        self.device.free_memory(self.data.index_buffer_memory, None);
        self.device.destroy_buffer(self.data.index_buffer, None);
        self.device.free_memory(self.data.vertex_buffer_memory, None);
        self.device.destroy_buffer(self.data.vertex_buffer, None);
        self.device.destroy_sampler(self.data.texture_sampler, None);
        self.device.destroy_image_view(self.data.texture_image_view, None);
        self.device.free_memory(self.data.texture_image_memory, None);
        self.device.destroy_image(self.data.texture_image, None);
        self.device.destroy_command_pool(self.data.command_pool, None);
        self.device.destroy_descriptor_set_layout(self.data.descriptor_set_layout, None);
        self.device.destroy_device(None);
        self.instance.destroy_surface_khr(self.data.surface, None);

        if VALIDATION_ENABLED {
            self.instance.destroy_debug_utils_messenger_ext(self.data.messenger, None);
        }

        self.instance.destroy_instance(None);
    }

    /// Destroys the parts of our Vulkan app related to the swapchain.
    #[rustfmt::skip]
    unsafe fn destroy_swapchain(&mut self) {
        self.device.destroy_descriptor_pool(self.data.descriptor_pool, None);
        // Unmap persistently mapped UBO memory before freeing
        for m in &self.data.uniform_buffers_memory { self.device.unmap_memory(*m); }
        self.data.uniform_buffers_mapped.clear();
        self.data.uniform_buffers_memory.iter().for_each(|m| self.device.free_memory(*m, None));
        self.data.uniform_buffers.iter().for_each(|b| self.device.destroy_buffer(*b, None));
        for m in &self.data.light_uniform_buffers_memory { self.device.unmap_memory(*m); }
        self.data.light_uniform_buffers_mapped.clear();
        self.data.light_uniform_buffers_memory.iter().for_each(|m| self.device.free_memory(*m, None));
        self.data.light_uniform_buffers.iter().for_each(|b| self.device.destroy_buffer(*b, None));
        self.device.destroy_image_view(self.data.depth_image_view, None);
        self.device.free_memory(self.data.depth_image_memory, None);
        self.device.destroy_image(self.data.depth_image, None);
        self.device.destroy_image_view(self.data.color_image_view, None);
        self.device.free_memory(self.data.color_image_memory, None);
        self.device.destroy_image(self.data.color_image, None);
        self.data.framebuffers.iter().for_each(|f| self.device.destroy_framebuffer(*f, None));
        self.device.destroy_pipeline(self.data.pipeline, None);
        self.device.destroy_pipeline_layout(self.data.pipeline_layout, None);
        self.device.destroy_render_pass(self.data.render_pass, None);
        self.data.swapchain_image_views.iter().for_each(|v| self.device.destroy_image_view(*v, None));
        self.device.destroy_swapchain_khr(self.data.swapchain, None);
    }
}
