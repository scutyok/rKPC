use std::collections::HashMap;

use anyhow::{Result, anyhow};
use cgmath::{vec2, vec3};
use log::*;

use crate::abc;
use crate::dat;
use crate::dat_mesh;
use crate::dtx;
use crate::lights;
use crate::types::*;
use crate::vulkan::texture::get_texture_dimensions;

/// Load a KISS Psycho Circus DAT file (v127) and extract mesh data
///
/// # Arguments
/// * `path` - Path to the .dat file
/// * `world_model_index` - Which world model to load (0 = main world)
/// * `scale` - Scale factor for the world geometry
///
/// # Returns
/// Vec of extracted lights from the DAT world objects
pub fn load_dat_model<P: AsRef<std::path::Path>>(
    data: &mut AppData,
    path: P,
    world_model_index: usize,
    scale: f32,
) -> Result<Vec<lights::Light>> {
    info!("Loading DAT file: {}", path.as_ref().display());

    let dat_file = dat::DatFile::read_from_file(&path)
        .map_err(|e| anyhow!("Failed to parse DAT file: {}", e))?;

    info!(
        "DAT file loaded: {} objects, {} world models",
        dat_file.objects.len(),
        dat_file.world_models.len()
    );

    if let Some(world) = dat_file.world_models.get(world_model_index) {
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

    let extractor = dat_mesh::MeshExtractor::new(&dat_file)
        .with_scale(scale)
        .with_skip_invisible(false)
        .with_skip_sky(false)
        .with_flip_winding(false);

    let mesh = extractor
        .extract_world_by_index(world_model_index)
        .ok_or_else(|| anyhow!("World model index {} not found", world_model_index))?;

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
    {
        let abc_objects = abc::extract_abc_objects(&dat_file.objects, "REZ", scale);
        println!("=== ABC OBJECTS: {} found ===", abc_objects.len());

        let mut type_counts: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for obj in &abc_objects {
            *type_counts.entry(&obj.type_name).or_insert(0) += 1;
        }
        for (tn, count) in &type_counts {
            println!("  {}: {}", tn, count);
        }

        for abc_obj in &abc_objects {
            let skin_name = abc_obj.skin_filename.clone();
            let tex_index = if let Some(&idx) = texture_name_to_index.get(&skin_name) {
                idx
            } else {
                let idx = texture_names.len();
                texture_names.push(skin_name.clone());
                texture_name_to_index.insert(skin_name.clone(), idx);
                idx
            };

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
            });
        }
    }

    // Spatially subdivide draw groups for tighter frustum culling
    subdivide_draw_groups(
        &data.vertices.clone(),
        &mut data.indices,
        &mut data.draw_groups,
        16.0,
    );

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

    Ok(world_lights)
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
    let mut new_indices: Vec<u32> = Vec::with_capacity(indices.len());
    let mut new_groups: Vec<DrawGroup> = Vec::new();

    for group in draw_groups.iter() {
        let start = group.first_index as usize;
        let end = (start + group.index_count as usize).min(indices.len());
        let group_indices = &indices[start..end];

        if group.index_count <= 36 {
            let first_index = new_indices.len() as u32;
            new_indices.extend_from_slice(group_indices);
            new_groups.push(DrawGroup {
                texture_index: group.texture_index,
                first_index,
                index_count: group.index_count,
                vertex_offset: 0,
            });
            continue;
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

        for (_cell_key, cell_indices) in &cells {
            let first_index = new_indices.len() as u32;
            let index_count = cell_indices.len() as u32;
            new_indices.extend_from_slice(cell_indices);
            new_groups.push(DrawGroup {
                texture_index: group.texture_index,
                first_index,
                index_count,
                vertex_offset: 0,
            });
        }
    }

    let old_group_count = draw_groups.len();
    *indices = new_indices;
    *draw_groups = new_groups;
    println!(
        "  Spatial subdivision: {} groups -> {} sub-groups (cell_size={:.0})",
        old_group_count,
        draw_groups.len(),
        cell_size
    );
}
