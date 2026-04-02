use std::collections::HashMap;

use anyhow::{Result, anyhow};
use cgmath::{vec2, vec3};
use log::*;

use crate::abc;
use crate::collision;
use crate::dat;
use crate::dat::PropertyValue;
use crate::dat_mesh;
use crate::dtx;
use crate::game_objects::GameObjectManager;
use crate::lights;
use crate::types::*;
use crate::vulkan::texture::get_texture_dimensions;
use crate::DemoSkyWorldModel::SkyModelInfo;

/// Fog settings extracted from a level's WorldProperties object.
pub struct LevelFogSettings {
    pub enabled: bool,
    pub color: [f32; 3],
    pub near_z: f32,
    pub far_z: f32,
    /// Whether sky-specific fog is enabled (from SkyFog property).
    pub sky_fog_enabled: bool,
    /// Sky fog far distance (from SkyFogFarZ); 0 = use world fog far.
    pub sky_fog_far: f32,
}

/// Result of loading a DAT world.
pub struct LoadedWorld {
    pub lights: Vec<lights::Light>,
    /// Collision mesh positions (includes invisible surfaces for blocking).
    pub collision_positions: Vec<cgmath::Vector3<f32>>,
    /// Collision mesh indices.
    pub collision_indices: Vec<u32>,
    /// Interactive game objects parsed from DAT WorldObjects.
    pub game_objects: GameObjectManager,
    /// Fog settings from the level's WorldProperties object, if present.
    pub fog: Option<LevelFogSettings>,
    /// Entity cylinders for solid objects (barrels, headless enemies) in Vulkan coords.
    pub entity_cylinders: Vec<collision::EntityCylinder>,
}

/// Load a KISS Psycho Circus DAT file (v127) and extract mesh data
///
/// # Arguments
/// * `path` - Path to the .dat file
/// * `world_model_index` - Which world model to load (0 = main world)
/// * `scale` - Scale factor for the world geometry
///
/// # Returns
/// A `LoadedWorld` containing lights and collision geometry
pub fn load_dat_model<P: AsRef<std::path::Path>>(
    data: &mut AppData,
    path: P,
    world_model_index: usize,
    scale: f32,
) -> Result<LoadedWorld> {
    info!("Loading DAT file: {}", path.as_ref().display());

    let dat_file = dat::DatFile::read_from_file(&path)
        .map_err(|e| anyhow!("Failed to parse DAT file: {}", e))?;

    info!(
        "DAT file loaded: {} objects, {} world models",
        dat_file.objects.len(),
        dat_file.world_models.len()
    );

    // Debug: list all world models to identify sky
    for (i, wm) in dat_file.world_models.iter().enumerate() {
        println!("  WorldModel[{}]: name='{}' flags=0x{:X} translation=({:.1},{:.1},{:.1}) polys={}",
            i, wm.world_name, wm.info_flags,
            wm.world_translation.x, wm.world_translation.y, wm.world_translation.z,
            wm.poly_count);
    }

    if let Some(world) = dat_file.world_models.get(world_model_index) {
        // Compute map center from bounding box (Lithtech Y-up → renderer Z-up)
        data.map_center = [
            (world.min_box.x + world.max_box.x) * 0.5 * scale,
            (world.min_box.z + world.max_box.z) * 0.5 * scale,
            (world.min_box.y + world.max_box.y) * 0.5 * scale,
        ];
        println!("  Map center: ({:.2}, {:.2}, {:.2})", data.map_center[0], data.map_center[1], data.map_center[2]);

        println!("=== WORLD MODEL DEBUG ===");
        println!("  World name: {}", world.world_name);
        println!("  Points: {}", world.points.len());
        println!("  Polygons: {}", world.polygons.len());
        println!("  Surfaces: {}", world.surfaces.len());
        println!("  Planes: {}", world.planes.len());

        for (i, poly) in world.polygons.iter().take(5).enumerate() {
            println!(
                "  Poly[{}]: {} verts, surface_idx={}",
                i,
                poly.disk_verts.len(),
                poly.surface_index
            );
        }
    }

    // Extract render mesh (skip Invisible textures and skyportal surfaces)
    let render_extractor = dat_mesh::MeshExtractor::new(&dat_file)
        .with_scale(scale)
        .with_skip_invisible(false)
        .with_skip_sky(false)
        .with_skip_skyportal(true)
        .with_flip_winding(false)
        .with_skip_textures(vec!["invisible".to_string()]);

    let mesh = render_extractor
        .extract_world_by_index(world_model_index)
        .ok_or_else(|| anyhow!("World model index {} not found", world_model_index))?;

    // Extract collision mesh (include invisible surfaces so they still block the player)
    let collision_extractor = dat_mesh::MeshExtractor::new(&dat_file)
        .with_scale(scale)
        .with_skip_invisible(false)
        .with_skip_sky(false)
        .with_flip_winding(false);

    let collision_mesh = collision_extractor
        .extract_world_by_index(world_model_index)
        .ok_or_else(|| anyhow!("World model index {} not found (collision)", world_model_index))?;

    let collision_positions: Vec<cgmath::Vector3<f32>> = collision_mesh
        .vertices
        .iter()
        .map(|v| cgmath::vec3(v.pos[0], v.pos[1], v.pos[2]))
        .collect();
    let collision_indices = collision_mesh.indices;

    info!(
        "Extracted mesh '{}': {} vertices, {} indices, {} texture groups",
        mesh.name,
        mesh.vertices.len(),
        mesh.indices.len(),
        mesh.textured_meshes.len()
    );

    let mut texture_name_to_index: HashMap<String, usize> = HashMap::new();
    let mut texture_names: Vec<String> = Vec::new();
    let mut texture_dimensions: HashMap<String, (u32, u32)> = HashMap::new();

    let mut current_vertex_offset = 0u32;
    let mut current_index_offset = 0u32;

    for textured_mesh in &mesh.textured_meshes {
        let texture_index =
            if let Some(&idx) = texture_name_to_index.get(&textured_mesh.texture_name) {
                idx
            } else {
                let idx = texture_names.len();
                texture_names.push(textured_mesh.texture_name.clone());
                texture_name_to_index.insert(textured_mesh.texture_name.clone(), idx);
                idx
            };

        let (tex_width, tex_height) =
            if let Some(&dims) = texture_dimensions.get(&textured_mesh.texture_name) {
                dims
            } else {
                let dims = get_texture_dimensions(&textured_mesh.texture_name);
                texture_dimensions.insert(textured_mesh.texture_name.clone(), dims);
                dims
            };

        for dat_vert in &textured_mesh.vertices {
            let normal = vec3(
                dat_vert.normal[0],
                dat_vert.normal[2],
                dat_vert.normal[1],
            );
            let vertex = Vertex {
                pos: vec3(dat_vert.pos[0], dat_vert.pos[1], dat_vert.pos[2]),
                color: vec3(dat_vert.color[0], dat_vert.color[1], dat_vert.color[2]),
                tex_coord: vec2(
                    dat_vert.tex_coord[0] / tex_width as f32,
                    dat_vert.tex_coord[1] / tex_height as f32,
                ),
                normal,
            };
            data.vertices.push(vertex);
        }

        for &idx in &textured_mesh.indices {
            data.indices.push(current_vertex_offset + idx);
        }

        data.draw_groups.push(DrawGroup {
            texture_index,
            first_index: current_index_offset,
            index_count: textured_mesh.indices.len() as u32,
            vertex_offset: 0,
            model_matrix: None,
        });

        current_vertex_offset += textured_mesh.vertices.len() as u32;
        current_index_offset += textured_mesh.indices.len() as u32;
    }

    println!("=== TEXTURE GROUPS ===");
    println!("  Total texture groups: {}", data.draw_groups.len());
    println!("  Unique textures: {}", texture_names.len());
    for (i, name) in texture_names.iter().enumerate() {
        println!("    [{}] {}", i, name);
    }

    // ABC model objects (barrels, decos, pickups, etc.)
    // Extract models first so we can register their skin textures in the atlas before
    // the texture atlas is built. Geometry is added AFTER subdivision (see below).

    // Collect BSP floor-surface triangles for floor-snapping ground objects.
    // In the original Lithtech engine, objects call MoveToFloor() at spawn to
    // drop from their editor position to the nearest surface below.
    // We triangulate upward-facing polygons to allow precise ray-triangle
    // intersection (vertical ray at object XZ position).
    let floor_tris: Vec<abc::FloorTri> = dat_file.world_models.get(world_model_index)
        .map(|bsp| {
            let mut tris = Vec::new();
            for poly in &bsp.polygons {
                let si = poly.surface_index as usize;
                if si >= bsp.surfaces.len() { continue; }
                let pi = bsp.surfaces[si].plane_index as usize;
                if pi >= bsp.planes.len() { continue; }
                // Floor-like surface: normal Y > 0.5 (upward-facing in Lithtech Y-up)
                if bsp.planes[pi].normal.y <= 0.5 { continue; }

                // Fan-triangulate the polygon (convex)
                let verts: Vec<[f32; 3]> = poly.disk_verts.iter()
                    .filter_map(|dv| {
                        let vi = dv.vertex_index as usize;
                        bsp.points.get(vi).map(|p| [p.x, p.y, p.z])
                    })
                    .collect();

                for i in 1..verts.len().saturating_sub(1) {
                    tris.push(abc::FloorTri {
                        v0: verts[0],
                        v1: verts[i],
                        v2: verts[i + 1],
                    });
                }
            }
            tris
        })
        .unwrap_or_default();

    let abc_objects = abc::extract_abc_objects(&dat_file.objects, "REZ", scale, path.as_ref().to_str().unwrap_or(""), &floor_tris);
    println!("=== ABC OBJECTS: {} found ===", abc_objects.len());
    {
        let mut type_counts: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for obj in &abc_objects {
            *type_counts.entry(obj.type_name.as_str()).or_insert(0) += 1;
        }
        for (tn, count) in &type_counts {
            println!("  {}: {}", tn, count);
        }
    }

    // Register skin textures now so they're included in the atlas build.
    for abc_obj in &abc_objects {
        let skin_name = abc_obj.skin_filename.clone();
        if !texture_name_to_index.contains_key(&skin_name) {
            let idx = texture_names.len();
            texture_names.push(skin_name.clone());
            texture_name_to_index.insert(skin_name.clone(), idx);
        }
        if !texture_dimensions.contains_key(&skin_name) {
            let dims = if std::path::Path::new(&abc_obj.skin_filename).exists() {
                match dtx::DtxFile::read_from_file(&abc_obj.skin_filename) {
                    Ok(dtx) => (dtx.width as u32, dtx.height as u32),
                    Err(_) => (256, 256),
                }
            } else {
                (256, 256)
            };
            texture_dimensions.insert(skin_name, dims);
        }
    }

    // Spatially subdivide WORLD draw groups only (ABC geometry not yet in the buffer).
    subdivide_draw_groups(
        &data.vertices.clone(),
        &mut data.indices,
        &mut data.draw_groups,
        16.0,
    );

    // NOW add ABC geometry – first_abc_draw_group is accurate because world subdivision is done.
    let first_abc_draw_group = data.draw_groups.len();

    for abc_obj in &abc_objects {
        let skin_name = abc_obj.skin_filename.clone();
        // Skin was registered above; look it up directly.
        let tex_index = *texture_name_to_index.get(&skin_name).unwrap();

        let vert_base = data.vertices.len() as u32;
        let idx_base = data.indices.len() as u32;

        for v in &abc_obj.mesh.vertices {
            data.vertices.push(Vertex {
                pos: vec3(v.pos[0], v.pos[1], v.pos[2]),
                color: vec3(1.0, 1.0, 1.0),
                tex_coord: vec2(v.tex_coord[0], v.tex_coord[1]),
                normal: vec3(v.normal[0], v.normal[1], v.normal[2]),
            });
        }

        for &i in &abc_obj.mesh.indices {
            data.indices.push(vert_base + i);
        }

        data.draw_groups.push(DrawGroup {
            texture_index: tex_index,
            first_index: idx_base,
            index_count: abc_obj.mesh.indices.len() as u32,
            vertex_offset: 0,
            model_matrix: None,
        });
    }

    let placed_abc_objects = abc_objects;

    // Dynamically gather sky model names from DemoSkyWorldModel and SkyPointer objects
    let sky_model_names: std::collections::HashSet<String> = {
        let mut set = std::collections::HashSet::new();
        for obj in &dat_file.objects {
            if obj.type_name == "DemoSkyWorldModel" || obj.type_name == "SkyPointer" {
                if let Some(PropertyValue::String(name)) = obj.get_property("Name") {
                    set.insert(name.to_lowercase());
                }
                // SkyPointer may also reference via SkyObjectName
                if let Some(PropertyValue::String(name)) = obj.get_property("SkyObjectName") {
                    set.insert(name.to_lowercase());
                }
            }
        }
        // Always include the classic names as fallback
        set.insert("sky".to_string());
        set.insert("clouds".to_string());
        set.insert("clouds2".to_string());
        set
    };
    debug!("Sky model names for this level: {:?}", sky_model_names);

    // Render all sub-world models (doors, windows, crates, ceiling fans, etc.)
    // These are BSP world models with indices 1..end that are not sky/cloud models.
    // Their polygon vertices are in sub-model-local space; we apply world_translation
    // to position them in the world.
    // bsp_submodels tracks (world_name, position_vulkan, draw_group_indices) for
    // animated objects like ceiling fans.
    let mut bsp_submodels: Vec<(String, [f32; 3], Vec<usize>)> = Vec::new();
    {
        let sky_names = &sky_model_names;
        let sub_extractor = dat_mesh::MeshExtractor::new(&dat_file)
            .with_scale(scale)
            .with_skip_invisible(true)
            .with_skip_sky(true)
            .with_skip_skyportal(true)
            .with_flip_winding(false)
            .with_skip_textures(vec!["invisible".to_string()]);

        // Separate extractor for animated models — does NOT skip invisible surfaces
        // because Lithtech marks dynamic BSP mesh surfaces with INVISIBLE so the
        // engine can skip-render them normally and let the object system handle them.
        let dyn_extractor = dat_mesh::MeshExtractor::new(&dat_file)
            .with_scale(scale)
            .with_skip_invisible(false)
            .with_skip_sky(true)
            .with_skip_skyportal(true)
            .with_flip_winding(false)
            .with_skip_textures(vec!["invisible".to_string()]);

        for wm_idx in 1..dat_file.world_models.len() {
            let wm = &dat_file.world_models[wm_idx];
            let name_lower = wm.world_name.to_lowercase();
            if sky_names.contains(&name_lower) {
                continue;
            }

            let is_fan = name_lower.starts_with("crotatingceilingfan");

            let sub_mesh = match (if is_fan { &dyn_extractor } else { &sub_extractor })
                .extract_world_by_index(wm_idx)
            {
                Some(m) => m,
                None => continue,
            };

            if sub_mesh.vertices.is_empty() {
                continue;
            }

            // Translation in Vulkan coords (Lithtech Y-up → Vulkan Z-up).
            // NOTE: sub-model BSP vertices are stored in world space already;
            // world_translation is the model pivot (used as rotation centre for
            // animated objects such as fans) — do NOT add it to vertex positions.
            let tx = wm.world_translation.x * scale;
            let ty = wm.world_translation.z * scale;
            let tz = wm.world_translation.y * scale;

            let mut this_model_dgs: Vec<usize> = Vec::new();

            for textured_mesh in &sub_mesh.textured_meshes {
                if textured_mesh.indices.is_empty() {
                    continue;
                }

                let skin_name = textured_mesh.texture_name.clone();
                let tex_index = if let Some(&idx) = texture_name_to_index.get(&skin_name) {
                    idx
                } else {
                    let idx = texture_names.len();
                    texture_names.push(skin_name.clone());
                    texture_name_to_index.insert(skin_name.clone(), idx);
                    if !texture_dimensions.contains_key(&skin_name) {
                        texture_dimensions.insert(skin_name.clone(), get_texture_dimensions(&skin_name));
                    }
                    idx
                };

                let (tex_w, tex_h) = texture_dimensions
                    .get(&skin_name)
                    .copied()
                    .unwrap_or((256, 256));

                let vert_base = data.vertices.len() as u32;
                let idx_base = data.indices.len() as u32;

                for v in &textured_mesh.vertices {
                    data.vertices.push(Vertex {
                        pos: vec3(v.pos[0], v.pos[1], v.pos[2]),
                        color: vec3(v.color[0], v.color[1], v.color[2]),
                        tex_coord: vec2(v.tex_coord[0] / tex_w as f32, v.tex_coord[1] / tex_h as f32),
                        normal: vec3(v.normal[0], v.normal[1], v.normal[2]),
                    });
                }

                for &i in &textured_mesh.indices {
                    data.indices.push(vert_base + i);
                }

                let dg_idx = data.draw_groups.len();
                data.draw_groups.push(DrawGroup {
                    texture_index: tex_index,
                    first_index: idx_base,
                    index_count: textured_mesh.indices.len() as u32,
                    vertex_offset: 0,
                    model_matrix: None,
                });
                this_model_dgs.push(dg_idx);
            }

            if !this_model_dgs.is_empty() {
                // For fans, use the geometric centroid of the actual mesh vertices as the
                // rotation pivot. world_translation is often (0,0,0) for BSP sub-models
                // that bake their position into vertex coordinates, so it can't be trusted
                // as the spin center.
                let pivot = if is_fan {
                    let all_verts: Vec<_> = sub_mesh
                        .textured_meshes
                        .iter()
                        .flat_map(|tm| tm.vertices.iter())
                        .collect();
                    if all_verts.is_empty() {
                        [tx, ty, tz]
                    } else {
                        let n = all_verts.len() as f32;
                        [
                            all_verts.iter().map(|v| v.pos[0]).sum::<f32>() / n,
                            all_verts.iter().map(|v| v.pos[1]).sum::<f32>() / n,
                            all_verts.iter().map(|v| v.pos[2]).sum::<f32>() / n,
                        ]
                    }
                } else {
                    [tx, ty, tz]
                };
                bsp_submodels.push((wm.world_name.clone(), pivot, this_model_dgs));
            }
        }
        println!("=== Sub-world models rendered: {} world models processed ===", dat_file.world_models.len() - 1);
    }

    // ── Animated torch flame billboard quads ───────────────────────────────
    // For every CTorch ABC object, create a transparent billboard quad that
    // cycles through the 6 TORCH*.DTX frames each frame in game_objects.rs.
    // Each torch gets its own quad geometry placed at its world position so
    // the occlusion culler's AABB (built from vertex positions) is correct.
    const FLAME_FRAMES: [&str; 6] = [
        "REZ/SPRITETEXTURES/FLAMETEST/TORCH1.DTX",
        "REZ/SPRITETEXTURES/FLAMETEST/TORCH2.DTX",
        "REZ/SPRITETEXTURES/FLAMETEST/TORCH3.DTX",
        "REZ/SPRITETEXTURES/FLAMETEST/TORCH4.DTX",
        "REZ/SPRITETEXTURES/FLAMETEST/TORCH5.DTX",
        "REZ/SPRITETEXTURES/FLAMETEST/TORCH6.DTX",
    ];
    // Register all 6 frame textures (if not already present) and record the
    // index of the first frame so frames are at base, base+1 … base+5.
    let flame_base_tex_index = texture_names.len();
    for &frame_path in &FLAME_FRAMES {
        let name = frame_path.to_string();
        if !texture_name_to_index.contains_key(&name) {
            let idx = texture_names.len();
            texture_names.push(name.clone());
            texture_name_to_index.insert(name.clone(), idx);
            if !texture_dimensions.contains_key(&name) {
                let dims = if std::path::Path::new(frame_path).exists() {
                    match dtx::DtxFile::read_from_file(frame_path) {
                        Ok(dtx) => (dtx.width as u32, dtx.height as u32),
                        Err(_) => (64, 64),
                    }
                } else {
                    (64, 64)
                };
                texture_dimensions.insert(name, dims);
            }
        }
    }

    // Quad geometry constants (Vulkan units, scale already applied to pos).
    const FLAME_WIDTH: f32 = 0.28;   // billboard half-width on each side
    const FLAME_HEIGHT: f32 = 0.38;  // billboard total height
    const FLAME_Z_OFFSET: f32 = 0.08; // raise above torch model base

    // (abc_index, flame_draw_group, flame_base_tex_index) for each CTorch.
    let mut torch_flames: Vec<(usize, usize, usize)> = Vec::new();

    for (i, abc) in placed_abc_objects.iter().enumerate() {
        if abc.type_name != "CTorch" {
            continue;
        }
        let pos = abc.position; // already in Vulkan/renderer space

        let fw = FLAME_WIDTH * 0.5;
        let fz_bot = pos[2] + FLAME_Z_OFFSET;
        let fz_top = fz_bot + FLAME_HEIGHT;

        // Four vertices of the quad in world space, initially facing +Y.
        // Billboard rotation (applied via model_matrix each frame) will swing
        // the quad to face the camera in the XY plane.
        //   Bottom-left, Bottom-right, Top-right, Top-left
        let quad_verts = [
            ([pos[0] - fw, pos[1], fz_bot], [0.0_f32, 1.0_f32]),
            ([pos[0] + fw, pos[1], fz_bot], [1.0, 1.0]),
            ([pos[0] + fw, pos[1], fz_top], [1.0, 0.0]),
            ([pos[0] - fw, pos[1], fz_top], [0.0, 0.0]),
        ];

        let vert_base = data.vertices.len() as u32;
        let idx_base = data.indices.len() as u32;

        for (p, uv) in &quad_verts {
            data.vertices.push(Vertex {
                pos: vec3(p[0], p[1], p[2]),
                color: vec3(1.0, 1.0, 1.0), // no pre-baked shadow on flame sprite
                tex_coord: vec2(uv[0], uv[1]),
                normal: vec3(0.0, 1.0, 0.0),
            });
        }
        // Two triangles: 0-1-2, 0-2-3
        data.indices.extend_from_slice(&[
            vert_base, vert_base + 1, vert_base + 2,
            vert_base, vert_base + 2, vert_base + 3,
        ]);

        let flame_dg = data.draw_groups.len();
        data.draw_groups.push(DrawGroup {
            texture_index: flame_base_tex_index, // starts at frame 0 (TORCH1)
            first_index: idx_base,
            index_count: 6,
            vertex_offset: 0,
            model_matrix: None,
        });

        torch_flames.push((i, flame_dg, flame_base_tex_index));
        println!("  CTorch[{}] flame quad: dg={} base_tex={} pos=({:.2},{:.2},{:.2})",
            i, flame_dg, flame_base_tex_index, pos[0], pos[1], pos[2]);
    }
    println!("=== Torch flame quads: {} ===", torch_flames.len());

    // Mark where sky draw groups will start (after all world + ABC + sub-world groups)
    data.sky_draw_group_start = data.draw_groups.len();

    // Extract sky world models and collect SkyModelInfo for animation
    let mut sky_model_infos: Vec<SkyModelInfo> = Vec::new();
    {
        let sky_extractor = dat_mesh::MeshExtractor::new(&dat_file)
            .with_scale(scale)
            .with_skip_invisible(false)
            .with_skip_sky(false)
            .with_skip_skyportal(false)
            .with_flip_winding(false)
            .with_skip_textures(vec!["invisible".to_string()]);

        // Find the first sky model to use its translation as the sky origin
        for wm in &dat_file.world_models {
            let name_lower = wm.world_name.to_lowercase();
            if sky_model_names.contains(&name_lower) {
                // Swizzle Y/Z to match renderer coordinate system (Lithtech Y-up → renderer Z-up)
                data.sky_translation = [
                    wm.world_translation.x * scale,
                    wm.world_translation.z * scale,
                    wm.world_translation.y * scale,
                ];
                break;
            }
        }

        for (wm_idx, wm) in dat_file.world_models.iter().enumerate() {
            let name_lower = wm.world_name.to_lowercase();
            if !sky_model_names.contains(&name_lower) {
                continue;
            }

            let dg_start = data.draw_groups.len();

            if let Some(sky_mesh) = sky_extractor.extract_world_by_index(wm_idx) {
                println!("=== SKY MESH '{}': {} verts, {} indices, {} groups, translation=({:.1},{:.1},{:.1}) ===",
                    wm.world_name, sky_mesh.vertices.len(), sky_mesh.indices.len(),
                    sky_mesh.textured_meshes.len(),
                    wm.world_translation.x, wm.world_translation.y, wm.world_translation.z);

                for textured_mesh in &sky_mesh.textured_meshes {
                    let tex_index =
                        if let Some(&idx) = texture_name_to_index.get(&textured_mesh.texture_name) {
                            idx
                        } else {
                            let idx = texture_names.len();
                            texture_names.push(textured_mesh.texture_name.clone());
                            texture_name_to_index.insert(textured_mesh.texture_name.clone(), idx);
                            idx
                        };

                    let (tex_width, tex_height) =
                        if let Some(&dims) = texture_dimensions.get(&textured_mesh.texture_name) {
                            dims
                        } else {
                            let dims = get_texture_dimensions(&textured_mesh.texture_name);
                            texture_dimensions.insert(textured_mesh.texture_name.clone(), dims);
                            dims
                        };

                    println!("  SKY TEX '{}': {}x{} ({} verts, {} idx)",
                        textured_mesh.texture_name, tex_width, tex_height,
                        textured_mesh.vertices.len(), textured_mesh.indices.len());

                    let group_vert_base = data.vertices.len() as u32;
                    let group_idx_base = data.indices.len() as u32;

                    for dat_vert in &textured_mesh.vertices {
                        let normal = vec3(
                            dat_vert.normal[0],
                            dat_vert.normal[2],
                            dat_vert.normal[1],
                        );
                        data.vertices.push(Vertex {
                            pos: vec3(dat_vert.pos[0], dat_vert.pos[1], dat_vert.pos[2]),
                            color: vec3(dat_vert.color[0], dat_vert.color[1], dat_vert.color[2]),
                            tex_coord: vec2(
                                dat_vert.tex_coord[0] / tex_width as f32,
                                dat_vert.tex_coord[1] / tex_height as f32,
                            ),
                            normal,
                        });
                    }

                    for &idx in &textured_mesh.indices {
                        data.indices.push(group_vert_base + idx);
                    }

                    data.draw_groups.push(DrawGroup {
                        texture_index: tex_index,
                        first_index: group_idx_base,
                        index_count: textured_mesh.indices.len() as u32,
                        vertex_offset: 0,
                        model_matrix: None,
                    });
                }

                // Track this sky model's draw group range for animation
                let dg_count = data.draw_groups.len() - dg_start;
                if dg_count > 0 {
                    sky_model_infos.push(SkyModelInfo {
                        name: wm.world_name.to_lowercase(),
                        draw_group_start: dg_start,
                        draw_group_count: dg_count,
                    });
                }
            }
        }

        println!("=== Sky groups: {} (starting at index {}) ===",
            data.draw_groups.len() - data.sky_draw_group_start,
            data.sky_draw_group_start);
    }

    // Store texture names and dimensions for later loading
    for name in &texture_names {
        let (width, height) = texture_dimensions.get(name).copied().unwrap_or((256, 256));
        data.level_textures.push(LevelTexture {
            name: name.clone(),
            width,
            height,
            ..Default::default()
        });
    }

    // Calculate and print bounds
    let mut min_pos = [f32::MAX, f32::MAX, f32::MAX];
    let mut max_pos = [f32::MIN, f32::MIN, f32::MIN];
    for v in &data.vertices {
        min_pos[0] = min_pos[0].min(v.pos.x);
        min_pos[1] = min_pos[1].min(v.pos.y);
        min_pos[2] = min_pos[2].min(v.pos.z);
        max_pos[0] = max_pos[0].max(v.pos.x);
        max_pos[1] = max_pos[1].max(v.pos.y);
        max_pos[2] = max_pos[2].max(v.pos.z);
    }

    println!("=== MESH BOUNDS (after scale {}) ===", scale);
    println!(
        "  Min: ({:.2}, {:.2}, {:.2})",
        min_pos[0], min_pos[1], min_pos[2]
    );
    println!(
        "  Max: ({:.2}, {:.2}, {:.2})",
        max_pos[0], max_pos[1], max_pos[2]
    );
    let center = [
        (min_pos[0] + max_pos[0]) / 2.0,
        (min_pos[1] + max_pos[1]) / 2.0,
        (min_pos[2] + max_pos[2]) / 2.0,
    ];
    let size = [
        max_pos[0] - min_pos[0],
        max_pos[1] - min_pos[1],
        max_pos[2] - min_pos[2],
    ];
    println!(
        "  Center: ({:.2}, {:.2}, {:.2})",
        center[0], center[1], center[2]
    );
    println!(
        "  Size: ({:.2}, {:.2}, {:.2})",
        size[0], size[1], size[2]
    );

    info!(
        "Loaded {} vertices, {} indices, {} draw groups from DAT file",
        data.vertices.len(),
        data.indices.len(),
        data.draw_groups.len()
    );

    let world_lights = lights::extract_lights_from_objects(&dat_file.objects, scale);

    {
        let mut types: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for obj in &dat_file.objects {
            types.insert(&obj.type_name);
        }
        let mut sorted: Vec<&str> = types.into_iter().collect();
        sorted.sort();
        println!("=== DAT OBJECT TYPES ({} unique) ===", sorted.len());
        for t in &sorted {
            println!("  {}", t);
        }
        println!("  Lights found: {}", world_lights.len());
    }

    // Extract fog settings from the WorldProperties object.
    let level_fog = dat_file.objects.iter()
        .find(|o| o.type_name == "WorldProperties")
        .map(|wp| {
            // Print ALL WorldProperties for diagnostics
            println!("=== WorldProperties ({} props) ===", wp.properties.len());
            for prop in &wp.properties {
                println!("  {:?} = {:?}", prop.name, prop.value);
            }

            let enabled = match wp.get_property("EnableFog").or_else(|| wp.get_property("FogEnable")) {
                Some(PropertyValue::Bool(b)) => *b != 0,
                _ => false,
            };
            let color = match wp.get_property("FogColor") {
                Some(PropertyValue::Color(c)) | Some(PropertyValue::Vector(c)) =>
                    [c.x / 255.0, c.y / 255.0, c.z / 255.0],
                _ => [0.05, 0.05, 0.08],
            };
            let near_z = match wp.get_property("FogNearZ") {
                Some(PropertyValue::Float(f)) => *f * scale,
                _ => 5.0,
            };
            let far_z = match wp.get_property("FogFarZ") {
                Some(PropertyValue::Float(f)) => *f * scale,
                _ => 22.0,
            };
            let sky_fog_enabled = match wp.get_property("SkyFog") {
                Some(PropertyValue::Bool(b)) => *b != 0,
                _ => enabled,
            };
            let sky_fog_far = match wp.get_property("SkyFogFarZ") {
                Some(PropertyValue::Float(f)) => *f * scale,
                _ => far_z,
            };
            println!("=== Fog: enabled={} color=[{:.3},{:.3},{:.3}] near={:.2} far={:.2} (scale={}) ===",
                enabled, color[0], color[1], color[2], near_z, far_z, scale);
            println!("=== SkyFog: enabled={} far={:.2} ===", sky_fog_enabled, sky_fog_far);
            LevelFogSettings { enabled, color, near_z, far_z, sky_fog_enabled, sky_fog_far }
        });

    // Build the interactive game object manager from placed ABC objects and BSP sub-models.
    let game_objects = GameObjectManager::extract_from_dat(
        &dat_file.objects,
        &placed_abc_objects,
        first_abc_draw_group,
        &bsp_submodels,
        &torch_flames,
        &sky_model_infos,
        scale,
    );

    // Build entity cylinders for solid objects (barrels, headless enemies).
    // Cylinders match the model shape better than AABBs and produce smooth
    // radial sliding when the player walks into them.
    let entity_cylinders: Vec<collision::EntityCylinder> = placed_abc_objects.iter()
        .filter(|o| o.type_name == "CBarrel" || o.type_name == "CHeadless")
        .filter_map(|o| {
            if o.mesh.vertices.is_empty() { return None; }
            // Compute XY center and Z bounds from world-space mesh vertices.
            let mut x_min = f32::MAX;
            let mut x_max = f32::MIN;
            let mut y_min = f32::MAX;
            let mut y_max = f32::MIN;
            let mut z_min = f32::MAX;
            let mut z_max = f32::MIN;
            for v in &o.mesh.vertices {
                x_min = x_min.min(v.pos[0]);
                x_max = x_max.max(v.pos[0]);
                y_min = y_min.min(v.pos[1]);
                y_max = y_max.max(v.pos[1]);
                z_min = z_min.min(v.pos[2]);
                z_max = z_max.max(v.pos[2]);
            }
            let cx = (x_min + x_max) * 0.5;
            let cy = (y_min + y_max) * 0.5;
            // Radius = half the smaller horizontal extent (tighter fit).
            let half_x = (x_max - x_min) * 0.5;
            let half_y = (y_max - y_min) * 0.5;
            let radius = half_x.min(half_y);
            Some(collision::EntityCylinder { center_x: cx, center_y: cy, radius, z_min, z_max })
        })
        .collect();
    log::info!("Built {} entity cylinders for collision", entity_cylinders.len());

    Ok(LoadedWorld {
        lights: world_lights,
        collision_positions,
        collision_indices,
        game_objects,
        fog: level_fog,
        entity_cylinders,
    })
}

/// Print summary information about a DAT file without loading mesh data
pub fn print_dat_info<P: AsRef<std::path::Path>>(path: P) -> Result<()> {
    info!("Analyzing DAT file: {}", path.as_ref().display());

    let dat_file = dat::DatFile::read_from_file(&path)
        .map_err(|e| anyhow!("Failed to parse DAT file: {}", e))?;

    println!("\n{}", "=".repeat(60));
    println!("DAT FILE SUMMARY");
    println!("{}", "=".repeat(60));

    println!(
        "\nVersion: {} (KISS Psycho Circus)",
        dat_file.header.version
    );
    println!("World Properties: {}", dat_file.world_info.properties);
    println!(
        "Lightmap Grid Size: {}",
        dat_file.world_info.light_map_grid_size
    );

    println!("\n--- WORLD OBJECTS ({}) ---", dat_file.objects.len());
    let mut object_types: HashMap<&str, usize> = HashMap::new();
    for obj in &dat_file.objects {
        *object_types.entry(&obj.type_name).or_insert(0) += 1;
    }
    for (type_name, count) in &object_types {
        println!("  {}: {}", type_name, count);
    }

    println!("\n--- WORLD MODELS ({}) ---", dat_file.world_models.len());
    for (i, model) in dat_file.world_models.iter().enumerate() {
        println!("\n  [{}] {}", i, model.world_name);
        println!("      Points: {}", model.point_count);
        println!("      Polygons: {}", model.poly_count);
        println!("      Surfaces: {}", model.surface_count);
        println!("      Textures: {}", model.texture_count);
        println!(
            "      Bounds: ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1})",
            model.min_box.x,
            model.min_box.y,
            model.min_box.z,
            model.max_box.x,
            model.max_box.y,
            model.max_box.z
        );
    }

    let extractor = dat_mesh::MeshExtractor::new(&dat_file);
    let meshes = extractor.extract_all_worlds();
    let stats = dat_mesh::MeshStats::from_meshes(&meshes);

    println!("\n--- MESH STATISTICS ---");
    println!("  Total Vertices: {}", stats.total_vertices);
    println!("  Total Indices: {}", stats.total_indices);
    println!("  Total Triangles: {}", stats.total_triangles);
    println!("  Total Texture Groups: {}", stats.texture_count);

    println!("\n{}", "=".repeat(60));

    Ok(())
}

/// Spatially subdivide draw groups into smaller cells for tighter frustum culling.
pub fn subdivide_draw_groups(
    vertices: &[Vertex],
    indices: &mut Vec<u32>,
    draw_groups: &mut Vec<DrawGroup>,
    cell_size: f32,
) {
    use rayon::prelude::*;

    // Each draw group produces a list of sub-groups (texture_index, cell_indices)
    let sub_results: Vec<Vec<(usize, Vec<u32>)>> = draw_groups
        .par_iter()
        .map(|group| {
            let start = group.first_index as usize;
            let end = (start + group.index_count as usize).min(indices.len());
            let group_indices = &indices[start..end];

            if group.index_count <= 36 {
                return vec![(group.texture_index, group_indices.to_vec())];
            }

            let mut cells: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();

            for tri in group_indices.chunks(3) {
                if tri.len() < 3 {
                    continue;
                }
                let v0 = vertices[tri[0] as usize].pos;
                let v1 = vertices[tri[1] as usize].pos;
                let v2 = vertices[tri[2] as usize].pos;
                let cx = (v0.x + v1.x + v2.x) / 3.0;
                let cy = (v0.y + v1.y + v2.y) / 3.0;
                let cz = (v0.z + v1.z + v2.z) / 3.0;

                let cell_x = (cx / cell_size).floor() as i32;
                let cell_y = (cy / cell_size).floor() as i32;
                let cell_z = (cz / cell_size).floor() as i32;

                cells
                    .entry((cell_x, cell_y, cell_z))
                    .or_default()
                    .extend_from_slice(tri);
            }

            cells
                .into_values()
                .map(|cell_indices| (group.texture_index, cell_indices))
                .collect()
        })
        .collect();

    // Flatten results sequentially to build final index/group buffers
    let old_group_count = draw_groups.len();
    let mut new_indices: Vec<u32> = Vec::with_capacity(indices.len());
    let mut new_groups: Vec<DrawGroup> = Vec::new();

    for sub_groups in sub_results {
        for (texture_index, cell_indices) in sub_groups {
            let first_index = new_indices.len() as u32;
            let index_count = cell_indices.len() as u32;
            new_indices.extend_from_slice(&cell_indices);
            new_groups.push(DrawGroup {
                texture_index,
                first_index,
                index_count,
                vertex_offset: 0,
                model_matrix: None,
            });
        }
    }

    *indices = new_indices;
    *draw_groups = new_groups;
    println!(
        "  Spatial subdivision: {} groups -> {} sub-groups (cell_size={:.0})",
        old_group_count,
        draw_groups.len(),
        cell_size
    );
}
