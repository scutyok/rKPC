#![allow(non_snake_case)]

// Manually mapping the folder structure to modules
#[path = "kiss_engine/dat/dat.rs"]
pub mod dat;

#[path = "kiss_engine/dat/dat_mesh.rs"]
pub mod dat_mesh;

#[path = "kiss_engine/dat/objects/game_objects.rs"]
pub mod game_objects;

// ── Individual object modules ────────────────────────────────────────────────
#[path = "kiss_engine/dat/objects/object_utils.rs"]
pub(crate) mod object_utils;

#[path = "kiss_engine/dat/objects/CBarrel.rs"]
pub mod CBarrel;

#[path = "kiss_engine/dat/objects/CCrate.rs"]
pub mod CCrate;

#[path = "kiss_engine/dat/objects/CDoorSliding.rs"]
pub mod CDoorSliding;

#[path = "kiss_engine/dat/objects/CSwitchRotating.rs"]
pub mod CSwitchRotating;

#[path = "kiss_engine/dat/objects/CSwitchSlide.rs"]
pub mod CSwitchSlide;

#[path = "kiss_engine/dat/objects/CTorch.rs"]
pub mod CTorch;

#[path = "kiss_engine/dat/objects/CRotatingCeilingFan.rs"]
pub mod CRotatingCeilingFan;

#[path = "kiss_engine/dat/objects/CWindow.rs"]
pub mod CWindow;

#[path = "kiss_engine/dat/objects/CWater.rs"]
pub mod CWater;

#[path = "kiss_engine/dat/objects/CLadder.rs"]
pub mod CLadder;

#[path = "kiss_engine/dat/objects/DemoSkyWorldModel.rs"]
pub mod DemoSkyWorldModel;

#[path = "kiss_engine/dat/objects/SkyPointer.rs"]
pub mod SkyPointer;

#[path = "kiss_engine/dat/objects/OutsideDef.rs"]
pub mod OutsideDef;

#[path = "kiss_engine/dat/objects/CPickupItem.rs"]
pub mod CPickupItem;

#[path = "kiss_engine/dat/objects/CCreature.rs"]
pub mod CCreature;

#[path = "kiss_engine/dat/objects/scripted_sequence.rs"]
pub mod scripted_sequence;

#[path = "kiss_engine/dat/world_loader.rs"]
pub mod world_loader;

#[path = "kiss_engine/dat/world_chooser.rs"]
pub mod world_chooser;

#[path = "kiss_engine/dtx/dtx.rs"]
pub mod dtx;

#[path = "kiss_engine/egui/egui_renderer.rs"]
pub mod egui_renderer;

#[path = "kiss_engine/pcx.rs"]
pub mod pcx;

#[path = "kiss_engine/collision/collision.rs"]
pub mod collision;

#[path = "kiss_engine/rendering/occlusion_culling.rs"]
pub mod occlusion_culling;

#[path = "kiss_engine/rendering/lights.rs"]
pub mod lights;

#[path = "kiss_engine/rendering/shadows.rs"]
pub mod shadows;

#[path = "kiss_engine/rendering/lighting_ubo.rs"]
pub mod lighting_ubo;

#[path = "kiss_engine/rendering/camera.rs"]
pub mod camera;

#[path = "kiss_engine/rendering/types.rs"]
pub mod types;

#[path = "kiss_engine/player/player_movement.rs"]
pub mod player_movement;

#[path = "kiss_engine/abc/abc.rs"]
pub mod abc;

#[path = "kiss_engine/vulkan/mod.rs"]
pub mod vulkan;