#![allow(non_snake_case)]

// Manually mapping the folder structure to modules
#[path = "kiss_engine/Resources/dat/dat.rs"]
pub mod dat;

#[path = "kiss_engine/Resources/dat/dat_mesh.rs"]
pub mod dat_mesh;

#[path = "kiss_engine/Mechanics/objects/game_objects.rs"]
pub mod game_objects;

// ── Individual object modules ────────────────────────────────────────────────
#[path = "kiss_engine/Mechanics/objects/object_utils.rs"]
pub(crate) mod object_utils;

#[path = "kiss_engine/Mechanics/objects/CBarrel.rs"]
pub mod CBarrel;

#[path = "kiss_engine/Mechanics/objects/CCrate.rs"]
pub mod CCrate;

#[path = "kiss_engine/Mechanics/objects/CDoorSliding.rs"]
pub mod CDoorSliding;

#[path = "kiss_engine/Mechanics/objects/CSwitchRotating.rs"]
pub mod CSwitchRotating;

#[path = "kiss_engine/Mechanics/objects/CSwitchSlide.rs"]
pub mod CSwitchSlide;

#[path = "kiss_engine/Mechanics/objects/CTorch.rs"]
pub mod CTorch;

#[path = "kiss_engine/Mechanics/objects/CRotatingCeilingFan.rs"]
pub mod CRotatingCeilingFan;

#[path = "kiss_engine/Mechanics/objects/CWindow.rs"]
pub mod CWindow;

#[path = "kiss_engine/Mechanics/objects/CWater.rs"]
pub mod CWater;

#[path = "kiss_engine/Mechanics/objects/CLadder.rs"]
pub mod CLadder;

#[path = "kiss_engine/Mechanics/objects/DemoSkyWorldModel.rs"]
pub mod DemoSkyWorldModel;

#[path = "kiss_engine/Mechanics/objects/SkyPointer.rs"]
pub mod SkyPointer;

#[path = "kiss_engine/Mechanics/objects/OutsideDef.rs"]
pub mod OutsideDef;

#[path = "kiss_engine/Mechanics/objects/CPickupItem.rs"]
pub mod CPickupItem;

#[path = "kiss_engine/Mechanics/objects/CCreature.rs"]
pub mod CCreature;

#[path = "kiss_engine/Mechanics/levelscripting/scripted_sequence.rs"]
pub mod scripted_sequence;

#[path = "kiss_engine/Resources/dat/world_loader.rs"]
pub mod world_loader;

#[path = "kiss_engine/Resources/dat/world_chooser.rs"]
pub mod world_chooser;

#[path = "kiss_engine/Resources/dtx/dtx.rs"]
pub mod dtx;

#[path = "kiss_engine/Rendering/egui/egui_renderer.rs"]
pub mod egui_renderer;

#[path = "kiss_engine/Resources/pcx.rs"]
pub mod pcx;

#[path = "kiss_engine/Mechanics/collision/collision.rs"]
pub mod collision;

#[path = "kiss_engine/Rendering/OcclusionCulling.rs"]
pub mod OcclusionCulling;

#[path = "kiss_engine/Rendering/LightObj.rs"]
pub mod LightObj;

#[path = "kiss_engine/Rendering/ShadowObj.rs"]
pub mod ShadowObj;

#[path = "kiss_engine/Rendering/LightingUbo.rs"]
pub mod LightingUbo;

#[path = "kiss_engine/Rendering/CameraObj.rs"]
pub mod CameraObj;

#[path = "kiss_engine/Rendering/types.rs"]
pub mod types;

#[path = "kiss_engine/Mechanics/player/CPlayerMovement.rs"]
pub mod CPlayerMovement;

#[path = "kiss_engine/Resources/abc/abc.rs"]
pub mod abc;

#[path = "kiss_engine/Rendering/vulkan/mod.rs"]
pub mod vulkan;

// Headless enemy AI (ported from original C code)
#[path = "kiss_engine/Mechanics/enemies/normal_enemies/headless.rs"]
pub mod headless_enemy;