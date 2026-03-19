// ABC v6 model file parser for Lithtech 1.5 (KISS Psycho Circus / Blood 2 / Shogo)
//
// Binary format: section-based structure
//   Header → Geometry → Nodes → Animation → AnimDims → TransformInfo (optional)
//
// Reference: specs_abc_v6.md

use byteorder::{LittleEndian, ReadBytesExt};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;

use crate::dat::{PropertyValue, Quaternion, WorldObject};

// ─── Errors ───

#[derive(Error, Debug)]
pub enum AbcError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid ABC file: {0}")]
    InvalidFile(String),

    #[error("Unsupported version token: {0}")]
    UnsupportedVersion(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

pub type Result<T> = std::result::Result<T, AbcError>;

// ─── Constants ───

const ABC_V6_TOKEN: &str = "MonolithExport Model File v6";

// Node flags
pub const FLAG_NULL: u8 = 0x01;
pub const FLAG_TRIS: u8 = 0x02;
pub const FLAG_DEFORMATION: u8 = 0x04;

// ─── Data Types ───

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcVector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcQuaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl AbcQuaternion {
    /// Conjugate (negate x, y, z) for flip_anim
    pub fn conjugated(&self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: self.w,
        }
    }

    /// Convert to a 3x3 rotation matrix (row-major)
    pub fn to_matrix3(&self) -> [[f32; 3]; 3] {
        let (x, y, z, w) = (self.x, self.y, self.z, self.w);
        let x2 = x + x;
        let y2 = y + y;
        let z2 = z + z;
        let xx = x * x2;
        let xy = x * y2;
        let xz = x * z2;
        let yy = y * y2;
        let yz = y * z2;
        let zz = z * z2;
        let wx = w * x2;
        let wy = w * y2;
        let wz = w * z2;

        [
            [1.0 - (yy + zz), xy - wz, xz + wy],
            [xy + wz, 1.0 - (xx + zz), yz - wx],
            [xz - wy, yz + wx, 1.0 - (xx + yy)],
        ]
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcNormal {
    pub x: i8,
    pub y: i8,
    pub z: i8,
}

impl AbcNormal {
    /// Normalize from signed byte range [-127..127] to unit vector
    pub fn to_float(&self) -> [f32; 3] {
        let fx = self.x as f32 / 127.0;
        let fy = self.y as f32 / 127.0;
        let fz = self.z as f32 / 127.0;
        let len = (fx * fx + fy * fy + fz * fz).sqrt();
        if len > 0.0 {
            [fx / len, fy / len, fz / len]
        } else {
            [0.0, 0.0, 1.0]
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcUVPair {
    pub u: f32,
    pub v: f32,
}

// ─── Geometry ───

#[derive(Debug, Clone)]
pub struct AbcTriangle {
    pub tex_coords: [AbcUVPair; 3],
    pub vertex_indices: [u16; 3],
    pub face_normal: AbcNormal,
}

#[derive(Debug, Clone)]
pub struct AbcVertex {
    pub position: AbcVector,
    pub normal: AbcNormal,
    pub transformation_index: u8,
    pub replacements: [u16; 2],
}

#[derive(Debug, Clone)]
pub struct AbcPiece {
    pub name: String,
    pub bounds_min: AbcVector,
    pub bounds_max: AbcVector,
    pub num_lods: u32,
    pub vertex_start_nums: Vec<u16>,
    pub triangles: Vec<AbcTriangle>,
    pub vertices: Vec<AbcVertex>,
    pub normal_verts: u32,
}

// ─── Nodes ───

#[derive(Debug, Clone)]
pub struct AbcNode {
    pub bounds_min: AbcVector,
    pub bounds_max: AbcVector,
    pub name: String,
    pub transformation_index: u16,
    pub flags: u8,
    pub md_vert_list: Vec<u16>,
    pub num_children: u32,
    /// Index of parent node (-1 for root)
    pub parent_index: i32,
    /// Bind matrix (4x4 row-major), calculated from first animation frame
    pub bind_matrix: [[f32; 4]; 4],
}

// ─── Animation ───

#[derive(Debug, Clone)]
pub struct AbcKeyframeInfo {
    pub time_index: u32,
    pub bounds_min: AbcVector,
    pub bounds_max: AbcVector,
    pub frame_string: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcNodeKeyframe {
    pub translation: AbcVector,
    pub rotation: AbcQuaternion,
}

#[derive(Debug, Clone)]
pub struct AbcNodeDeformation {
    /// Decompressed vertex positions per keyframe, flattened [keyframe][md_vert]
    pub positions: Vec<AbcVector>,
}

#[derive(Debug, Clone)]
pub struct AbcAnimation {
    pub name: String,
    pub length_ms: u32,
    pub bounds_min: AbcVector,
    pub bounds_max: AbcVector,
    pub keyframes: Vec<AbcKeyframeInfo>,
    /// Per-node, per-keyframe transforms: [node_index][keyframe_index]
    pub node_keyframes: Vec<Vec<AbcNodeKeyframe>>,
    /// Per-node vertex deformations (only for nodes with md_verts > 0)
    pub node_deformations: Vec<AbcNodeDeformation>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcAnimDims {
    pub dimensions: AbcVector,
}

// ─── Transform Info ───

#[derive(Debug, Clone, Copy)]
pub struct AbcTransformInfo {
    pub flip_geom: i32,
    pub flip_anim: i32,
}

impl Default for AbcTransformInfo {
    fn default() -> Self {
        Self {
            flip_geom: 1,
            flip_anim: 1,
        }
    }
}

// ─── Model ───

#[derive(Debug, Clone)]
pub struct AbcModel {
    pub command_string: String,
    pub pieces: Vec<AbcPiece>,
    pub nodes: Vec<AbcNode>,
    pub animations: Vec<AbcAnimation>,
    pub anim_dims: Vec<AbcAnimDims>,
    pub transform_info: AbcTransformInfo,
}

impl AbcModel {
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path).map_err(|e| {
            AbcError::Io(std::io::Error::new(
                e.kind(),
                format!("{}: {}", path.as_ref().display(), e),
            ))
        })?;
        let mut reader = BufReader::new(file);
        Self::read(&mut reader)
    }

    pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut model = AbcModel {
            command_string: String::new(),
            pieces: Vec::new(),
            nodes: Vec::new(),
            animations: Vec::new(),
            anim_dims: Vec::new(),
            transform_info: AbcTransformInfo::default(),
        };

        let mut next_offset: i32 = 0;

        while next_offset != -1 {
            reader.seek(SeekFrom::Start(next_offset as u64))?;

            let section_name = read_lt_string(reader)?;
            next_offset = reader.read_i32::<LittleEndian>()?;

            log::debug!("ABC section: '{}', next_offset={}", section_name, next_offset);

            match section_name.as_str() {
                "Header" => {
                    let version = read_lt_string(reader)?;
                    if version != ABC_V6_TOKEN {
                        return Err(AbcError::UnsupportedVersion(version));
                    }
                    model.command_string = read_lt_string(reader)?;
                    log::info!("ABC v6: command_string='{}'", model.command_string);
                }
                "Geometry" => {
                    let piece = read_geometry_section(reader)?;
                    model.pieces.push(piece);
                }
                "Nodes" => {
                    model.nodes = read_nodes_section(reader)?;
                }
                "Animation" => {
                    model.animations =
                        read_animation_section(reader, &model.nodes)?;
                }
                "AnimDims" => {
                    model.anim_dims =
                        read_animdims_section(reader, model.animations.len())?;
                }
                "TransformInfo" => {
                    model.transform_info = AbcTransformInfo {
                        flip_geom: reader.read_i32::<LittleEndian>()?,
                        flip_anim: reader.read_i32::<LittleEndian>()?,
                    };
                }
                other => {
                    log::warn!("Unknown ABC section: '{}'", other);
                }
            }
        }

        // Post-process: compute bind matrices from first animation
        compute_bind_matrices(&mut model);

        Ok(model)
    }
}

// ─── Section Readers ───

fn read_geometry_section<R: Read>(reader: &mut R) -> Result<AbcPiece> {
    let bounds_min = read_abc_vector(reader)?;
    let bounds_max = read_abc_vector(reader)?;
    let num_lods = reader.read_u32::<LittleEndian>()?;

    // vertex_start_nums: num_lods + 1 entries
    let mut vertex_start_nums = Vec::with_capacity((num_lods + 1) as usize);
    for _ in 0..=num_lods {
        vertex_start_nums.push(reader.read_u16::<LittleEndian>()?);
    }

    // Triangles
    let num_tris = reader.read_u32::<LittleEndian>()?;
    let mut triangles = Vec::with_capacity(num_tris as usize);
    for _ in 0..num_tris {
        let mut tex_coords = [AbcUVPair::default(); 3];
        for tc in &mut tex_coords {
            tc.u = reader.read_f32::<LittleEndian>()?;
            tc.v = reader.read_f32::<LittleEndian>()?;
        }
        let v1 = reader.read_u16::<LittleEndian>()?;
        let v2 = reader.read_u16::<LittleEndian>()?;
        let v3 = reader.read_u16::<LittleEndian>()?;
        let nx = reader.read_i8()?;
        let ny = reader.read_i8()?;
        let nz = reader.read_i8()?;

        triangles.push(AbcTriangle {
            tex_coords,
            vertex_indices: [v1, v2, v3],
            face_normal: AbcNormal { x: nx, y: ny, z: nz },
        });
    }

    // Vertices
    let num_verts = reader.read_u32::<LittleEndian>()?;
    let normal_verts = reader.read_u32::<LittleEndian>()?;
    let mut vertices = Vec::with_capacity(num_verts as usize);
    for _ in 0..num_verts {
        let position = read_abc_vector(reader)?;
        let nx = reader.read_i8()?;
        let ny = reader.read_i8()?;
        let nz = reader.read_i8()?;
        let transformation_index = reader.read_u8()?;
        let r0 = reader.read_u16::<LittleEndian>()?;
        let r1 = reader.read_u16::<LittleEndian>()?;

        vertices.push(AbcVertex {
            position,
            normal: AbcNormal { x: nx, y: ny, z: nz },
            transformation_index,
            replacements: [r0, r1],
        });
    }

    log::info!(
        "ABC Geometry: {} tris, {} verts ({} normal), {} LODs",
        num_tris, num_verts, normal_verts, num_lods
    );

    Ok(AbcPiece {
        name: String::new(),
        bounds_min,
        bounds_max,
        num_lods,
        vertex_start_nums,
        triangles,
        vertices,
        normal_verts,
    })
}

fn read_nodes_section<R: Read>(reader: &mut R) -> Result<Vec<AbcNode>> {
    let mut nodes = Vec::new();
    let mut children_left: i32 = 1; // start with root

    while children_left > 0 {
        children_left -= 1;

        let bounds_min = read_abc_vector(reader)?;
        let bounds_max = read_abc_vector(reader)?;
        let name = read_lt_string(reader)?;
        let transformation_index = reader.read_u16::<LittleEndian>()?;
        let flags = reader.read_u8()?;

        let num_md_verts = reader.read_u32::<LittleEndian>()?;
        let mut md_vert_list = Vec::with_capacity(num_md_verts as usize);
        for _ in 0..num_md_verts {
            md_vert_list.push(reader.read_u16::<LittleEndian>()?);
        }

        let num_children = reader.read_u32::<LittleEndian>()?;
        children_left += num_children as i32;

        nodes.push(AbcNode {
            bounds_min,
            bounds_max,
            name,
            transformation_index,
            flags,
            md_vert_list,
            num_children,
            parent_index: -1,
            bind_matrix: identity_4x4(),
        });
    }

    // Build parent-child relationships (depth-first order)
    build_node_hierarchy(&mut nodes);

    log::info!("ABC Nodes: {} total", nodes.len());
    Ok(nodes)
}

fn read_animation_section<R: Read>(
    reader: &mut R,
    nodes: &[AbcNode],
) -> Result<Vec<AbcAnimation>> {
    let num_anims = reader.read_u32::<LittleEndian>()?;
    let mut animations = Vec::with_capacity(num_anims as usize);

    for _ in 0..num_anims {
        let name = read_lt_string(reader)?;
        let length_ms = reader.read_u32::<LittleEndian>()?;
        let bounds_min = read_abc_vector(reader)?;
        let bounds_max = read_abc_vector(reader)?;
        let num_keyframes = reader.read_u32::<LittleEndian>()?;

        // Keyframe metadata
        let mut keyframes = Vec::with_capacity(num_keyframes as usize);
        for _ in 0..num_keyframes {
            let time_index = reader.read_u32::<LittleEndian>()?;
            let kf_bounds_min = read_abc_vector(reader)?;
            let kf_bounds_max = read_abc_vector(reader)?;
            let frame_string = read_lt_string(reader)?;

            keyframes.push(AbcKeyframeInfo {
                time_index,
                bounds_min: kf_bounds_min,
                bounds_max: kf_bounds_max,
                frame_string,
            });
        }

        // Per-node, per-keyframe transforms
        let num_nodes = nodes.len();
        let mut node_keyframes = Vec::with_capacity(num_nodes);
        let mut node_deformations = Vec::with_capacity(num_nodes);

        for node_idx in 0..num_nodes {
            // Read keyframe transforms for this node
            let mut kfs = Vec::with_capacity(num_keyframes as usize);
            for _ in 0..num_keyframes {
                let translation = read_abc_vector(reader)?;
                let rx = reader.read_f32::<LittleEndian>()?;
                let ry = reader.read_f32::<LittleEndian>()?;
                let rz = reader.read_f32::<LittleEndian>()?;
                let rw = reader.read_f32::<LittleEndian>()?;

                kfs.push(AbcNodeKeyframe {
                    translation,
                    rotation: AbcQuaternion {
                        x: rx,
                        y: ry,
                        z: rz,
                        w: rw,
                    },
                });
            }
            node_keyframes.push(kfs);

            // Read vertex deformations if this node has md_verts
            let md_vert_count = nodes[node_idx].md_vert_list.len();
            let mut deformation = AbcNodeDeformation {
                positions: Vec::new(),
            };

            if md_vert_count > 0 {
                // Read compressed positions
                let total = num_keyframes as usize * md_vert_count;
                let mut compressed = Vec::with_capacity(total);
                for _ in 0..total {
                    let cx = reader.read_u8()?;
                    let cy = reader.read_u8()?;
                    let cz = reader.read_u8()?;
                    compressed.push([cx, cy, cz]);
                }

                // Read scale and transform for decompression
                let scale = read_abc_vector(reader)?;
                let transform = read_abc_vector(reader)?;

                // Decompress
                deformation.positions.reserve(total);
                for c in &compressed {
                    deformation.positions.push(AbcVector {
                        x: (c[0] as f32 * scale.x) + transform.x,
                        y: (c[1] as f32 * scale.y) + transform.y,
                        z: (c[2] as f32 * scale.z) + transform.z,
                    });
                }
            } else {
                // Still read scale + transform even if no md_verts
                let _scale = read_abc_vector(reader)?;
                let _transform = read_abc_vector(reader)?;
            }

            node_deformations.push(deformation);
        }

        log::info!(
            "ABC Animation '{}': {}ms, {} keyframes",
            name,
            length_ms,
            num_keyframes
        );

        animations.push(AbcAnimation {
            name,
            length_ms,
            bounds_min,
            bounds_max,
            keyframes,
            node_keyframes,
            node_deformations,
        });
    }

    Ok(animations)
}

fn read_animdims_section<R: Read>(
    reader: &mut R,
    num_anims: usize,
) -> Result<Vec<AbcAnimDims>> {
    let mut dims = Vec::with_capacity(num_anims);
    for _ in 0..num_anims {
        dims.push(AbcAnimDims {
            dimensions: read_abc_vector(reader)?,
        });
    }
    Ok(dims)
}

// ─── Node Hierarchy Builder ───

fn build_node_hierarchy(nodes: &mut [AbcNode]) {
    // Depth-first order: reconstruct parent indices using a stack
    if nodes.is_empty() {
        return;
    }

    // Stack of (node_index, remaining_children)
    let mut stack: Vec<(usize, u32)> = Vec::new();

    // Root has no parent
    nodes[0].parent_index = -1;
    stack.push((0, nodes[0].num_children));

    for i in 1..nodes.len() {
        // Pop finished parents
        while let Some(top) = stack.last() {
            if top.1 == 0 {
                stack.pop();
            } else {
                break;
            }
        }

        if let Some(top) = stack.last_mut() {
            nodes[i].parent_index = top.0 as i32;
            top.1 -= 1;
        }

        if nodes[i].num_children > 0 {
            stack.push((i, nodes[i].num_children));
        }
    }
}

// ─── Bind Matrix Computation ───

fn identity_4x4() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn mat4_multiply(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                out[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    out
}

fn transform_point(mat: &[[f32; 4]; 4], p: &AbcVector) -> AbcVector {
    AbcVector {
        x: mat[0][0] * p.x + mat[0][1] * p.y + mat[0][2] * p.z + mat[0][3],
        y: mat[1][0] * p.x + mat[1][1] * p.y + mat[1][2] * p.z + mat[1][3],
        z: mat[2][0] * p.x + mat[2][1] * p.y + mat[2][2] * p.z + mat[2][3],
    }
}

fn transform_normal(mat: &[[f32; 4]; 4], n: &[f32; 3]) -> [f32; 3] {
    // Transform normal (ignore translation)
    let x = mat[0][0] * n[0] + mat[0][1] * n[1] + mat[0][2] * n[2];
    let y = mat[1][0] * n[0] + mat[1][1] * n[1] + mat[1][2] * n[2];
    let z = mat[2][0] * n[0] + mat[2][1] * n[1] + mat[2][2] * n[2];
    let len = (x * x + y * y + z * z).sqrt();
    if len > 0.0 {
        [x / len, y / len, z / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn compute_bind_matrices(model: &mut AbcModel) {
    if model.animations.is_empty() || model.nodes.is_empty() {
        return;
    }

    let flip_anim = model.transform_info.flip_anim != 0;

    // Use first animation, first keyframe for bind pose
    let anim = &model.animations[0];

    for node_idx in 0..model.nodes.len() {
        if node_idx >= anim.node_keyframes.len() || anim.node_keyframes[node_idx].is_empty() {
            continue;
        }

        let kf = &anim.node_keyframes[node_idx][0];

        let mut rot = kf.rotation;
        if flip_anim {
            rot = rot.conjugated();
        }

        let rot_m = rot.to_matrix3();

        // Build local matrix: rotation + translation
        let local_mat: [[f32; 4]; 4] = [
            [rot_m[0][0], rot_m[0][1], rot_m[0][2], kf.translation.x],
            [rot_m[1][0], rot_m[1][1], rot_m[1][2], kf.translation.y],
            [rot_m[2][0], rot_m[2][1], rot_m[2][2], kf.translation.z],
            [0.0, 0.0, 0.0, 1.0],
        ];

        let parent_idx = model.nodes[node_idx].parent_index;
        let parent_mat = if parent_idx >= 0 {
            model.nodes[parent_idx as usize].bind_matrix
        } else {
            identity_4x4()
        };

        model.nodes[node_idx].bind_matrix = mat4_multiply(&parent_mat, &local_mat);
    }
}

// ─── Mesh Extraction ───

/// A ready-to-render vertex from an ABC model
#[derive(Debug, Clone, Copy)]
pub struct AbcMeshVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub tex_coord: [f32; 2],
}

/// Complete renderable mesh extracted from an ABC model
#[derive(Debug, Clone)]
pub struct AbcMesh {
    pub vertices: Vec<AbcMeshVertex>,
    pub indices: Vec<u32>,
}

impl AbcModel {
    /// Extract a renderable mesh from the first piece (LOD 0), with vertices
    /// transformed by their bind-pose matrices and coordinate-swapped into Vulkan space.
    ///
    /// Coordinate convention (same as dat_mesh.rs):
    ///   Lithtech (X,Y,Z) → Vulkan (X, Z, Y)
    pub fn extract_mesh(&self) -> Option<AbcMesh> {
        let mut mesh = self.extract_mesh_lithtech()?;
        // Coordinate swap: Lithtech (X,Y,Z) → Vulkan (X, Z, Y)
        for v in &mut mesh.vertices {
            let y = v.pos[1];
            v.pos[1] = v.pos[2];
            v.pos[2] = y;
            let ny = v.normal[1];
            v.normal[1] = v.normal[2];
            v.normal[2] = ny;
        }
        Some(mesh)
    }

    /// Extract a renderable mesh in Lithtech coordinate space (no coord swap).
    /// Used internally so world-object transforms can be applied before the
    /// final Lithtech→Vulkan coordinate conversion.
    pub fn extract_mesh_lithtech(&self) -> Option<AbcMesh> {
        let piece = self.pieces.first()?;

        // Pre-transform all vertices by their node bind matrix
        let mut transformed_positions: Vec<[f32; 3]> = Vec::with_capacity(piece.vertices.len());
        let mut transformed_normals: Vec<[f32; 3]> = Vec::with_capacity(piece.vertices.len());

        for (vert_idx, vert) in piece.vertices.iter().enumerate() {
            let node_idx = vert.transformation_index as usize;

            let bind_mat = if node_idx < self.nodes.len() {
                &self.nodes[node_idx].bind_matrix
            } else {
                &identity_4x4()
            };

            // Check for vertex animation (mesh deformation)
            let pos = if node_idx < self.nodes.len()
                && !self.nodes[node_idx].md_vert_list.is_empty()
                && !self.animations.is_empty()
            {
                // Find this vertex in the md_vert_list
                if let Some(md_idx) = self.nodes[node_idx]
                    .md_vert_list
                    .iter()
                    .position(|&v| v == vert_idx as u16)
                {
                    // Use deformed position from first frame
                    let deform = &self.animations[0].node_deformations[node_idx];
                    if md_idx < deform.positions.len() {
                        let dp = &deform.positions[md_idx];
                        transform_point(bind_mat, dp)
                    } else {
                        transform_point(bind_mat, &vert.position)
                    }
                } else {
                    transform_point(bind_mat, &vert.position)
                }
            } else {
                transform_point(bind_mat, &vert.position)
            };

            let raw_normal = vert.normal.to_float();
            let n = transform_normal(bind_mat, &raw_normal);

            // Keep in Lithtech space (X, Y, Z) — no coord swap here
            transformed_positions.push([pos.x, pos.y, pos.z]);
            transformed_normals.push([n[0], n[1], n[2]]);
        }

        // Build index buffer from triangles (LOD 0 only — use normal_verts range)
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut vert_map: HashMap<u64, u32> = HashMap::new();

        for tri in &piece.triangles {
            for corner in 0..3 {
                let vi = tri.vertex_indices[corner] as usize;
                if vi >= transformed_positions.len() {
                    continue;
                }

                let uv = &tri.tex_coords[corner];

                // Build a unique key from vertex index + UV (faces may re-use
                // geometry verts with different UVs)
                let uv_bits_u = uv.u.to_bits() as u64;
                let uv_bits_v = uv.v.to_bits() as u64;
                let key = (vi as u64) | (uv_bits_u << 16) | (uv_bits_v << 48);

                let idx = if let Some(&existing) = vert_map.get(&key) {
                    existing
                } else {
                    let new_idx = vertices.len() as u32;
                    vertices.push(AbcMeshVertex {
                        pos: transformed_positions[vi],
                        normal: transformed_normals[vi],
                        tex_coord: [uv.u, uv.v],
                    });
                    vert_map.insert(key, new_idx);
                    new_idx
                };

                indices.push(idx);
            }
        }

        log::info!(
            "ABC mesh extracted: {} vertices, {} indices ({} triangles)",
            vertices.len(),
            indices.len(),
            indices.len() / 3
        );

        Some(AbcMesh { vertices, indices })
    }
}

// ─── World Object Extraction ───

/// An ABC-model object placed in the world, with its position, rotation, and
/// scale taken from the DAT world objects, and a pre-loaded mesh.
#[derive(Debug, Clone)]
pub struct PlacedAbcObject {
    /// Object type name from DAT (e.g. "CBarrel", "CModel", "CModelDeco")
    pub type_name: String,
    /// Model filename from DAT properties (e.g. "models\\decos\\barrel.abc")
    pub model_filename: String,
    /// Skin texture filename (resolved filesystem path to .dtx)
    pub skin_filename: String,
    /// World position in Vulkan coords (scaled)
    pub position: [f32; 3],
    /// Rotation quaternion from DAT
    pub rotation: [f32; 4],
    /// Mesh data ready for rendering
    pub mesh: AbcMesh,
}

/// Scan DAT world objects for ABC model placements, load each referenced model,
/// transform its mesh to world space, and return placed objects ready for
/// rendering.
///
/// Supported object types:
/// - **CBarrel**: hardcoded model (`models/decos/barrel.abc`), skin from `skin_name`
/// - **CModel / CModelDeco**: model from `model_name`, skin from `skin_name`, per-object `scale`
/// - **CPickupTrigger**: model from `model`, skin from `skin`
/// - Any other type with a `model_name` or `model` property pointing to an `.abc` file
///
/// `rez_root` is the path to the REZ directory (e.g. "REZ").
/// `scale` is the world coordinate scale factor (typically 0.01).
pub fn extract_abc_objects(
    objects: &[WorldObject],
    rez_root: &str,
    scale: f32,
) -> Vec<PlacedAbcObject> {
    // Cache loaded ABC models by resolved path
    let mut model_cache: HashMap<String, Option<AbcModel>> = HashMap::new();
    let mut placed = Vec::new();

    for obj in objects {
        let tn = obj.type_name.as_str();

        // ── Determine model filename ───────────────────────────────
        let filename = if tn == "CBarrel" {
            "models/decos/barrel.abc".to_string()
        } else {
            // Try common property names in priority order
            match obj.get_property("model_name")
                .or_else(|| obj.get_property("model"))
                .or_else(|| obj.get_property("Filename"))
            {
                Some(PropertyValue::String(s)) if s.to_ascii_lowercase().ends_with(".abc") => {
                    s.clone()
                }
                _ => continue,
            }
        };

        // ── Position (required) ────────────────────────────────────
        let pos = match obj.get_position() {
            Some(p) => p,
            None => continue,
        };

        // ── Rotation (optional, defaults to identity) ──────────────
        let rot = obj.get_rotation().unwrap_or(Quaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        });

        // ── Per-object scale (optional, defaults to 1.0) ──────────
        let obj_scale = match obj.get_property("scale") {
            Some(PropertyValue::Float(f)) => *f,
            _ => 1.0,
        };

        // ── Skin texture ───────────────────────────────────────────
        let skin = match obj.get_property("skin_name")
            .or_else(|| obj.get_property("skin"))
            .or_else(|| obj.get_property("Skin"))
        {
            Some(PropertyValue::String(s)) => s.clone(),
            _ => String::new(),
        };

        let resolved_skin = if !skin.is_empty() {
            resolve_rez_path(rez_root, &skin)
        } else {
            // Fallback: derive skin from model name
            let base = filename
                .replace('\\', "/")
                .replace("models/", "skins/")
                .replace("MODELS/", "SKINS/")
                .replace(".abc", "_a.dtx")
                .replace(".ABC", "_A.DTX");
            resolve_rez_path(rez_root, &base)
        };

        // ── Resolve & load the ABC model ───────────────────────────
        let resolved_path = resolve_rez_path(rez_root, &filename);

        let abc_model = model_cache
            .entry(resolved_path.clone())
            .or_insert_with(|| match AbcModel::read_from_file(&resolved_path) {
                Ok(m) => {
                    log::info!("Loaded ABC model: {}", resolved_path);
                    Some(m)
                }
                Err(e) => {
                    log::error!("Failed to load ABC model '{}': {}", resolved_path, e);
                    None
                }
            });

        let abc_model = match abc_model {
            Some(m) => m,
            None => continue,
        };

        // ── Extract mesh & transform to world space ────────────────
        let base_mesh = match abc_model.extract_mesh_lithtech() {
            Some(m) => m,
            None => continue,
        };

        // DAT rotation property stores Euler angles (radians), NOT a quaternion.
        // The 4 floats read as (w, x, y, z) map to:
        //   w = pitch  (around X)
        //   x = yaw    (around Y / up)
        //   y = roll   (around Z)
        //   z = 1.0    (marker)
        //
        // ABC models and Lithtech world are both Y-up, matching the renderer
        // convention (pos[2] = height after L→V swap, camera up = Z).
        // Rotation: R = Ry(yaw) · Rx(pitch) · Rz(roll)
        let yaw   =  rot.x;
        let pitch = -rot.w;
        let roll  = -rot.y;

        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let (sr, cr) = roll.sin_cos();

        // R = Ry(yaw) · Rx(pitch) · Rz(roll)
        let r00 =  cy * cr + sy * sp * sr;
        let r01 = -cy * sr + sy * sp * cr;
        let r02 =  sy * cp;
        let r10 =  cp * sr;
        let r11 =  cp * cr;
        let r12 = -sp;
        let r20 = -sy * cr + cy * sp * sr;
        let r21 =  sy * sr + cy * sp * cr;
        let r22 =  cy * cp;

        let mut world_verts = base_mesh.vertices.clone();
        for v in &mut world_verts {
            let px = v.pos[0] * obj_scale;
            let py = v.pos[1] * obj_scale;
            let pz = v.pos[2] * obj_scale;

            // Apply full Euler rotation in Y-up Lithtech space
            let rx = r00 * px + r01 * py + r02 * pz;
            let ry = r10 * px + r11 * py + r12 * pz;
            let rz = r20 * px + r21 * py + r22 * pz;

            // Translate in Lithtech space
            let lx = rx + pos.x;
            let ly = ry + pos.y;
            let lz = rz + pos.z;

            // Coord swap Lithtech (X,Y,Z) → renderer (X, Z, Y), then world scale
            v.pos[0] = lx * scale;
            v.pos[1] = lz * scale;
            v.pos[2] = ly * scale;

            // Rotate normals then coord-swap
            let nx = v.normal[0];
            let ny = v.normal[1];
            let nz = v.normal[2];
            let rnx = r00 * nx + r01 * ny + r02 * nz;
            let rny = r10 * nx + r11 * ny + r12 * nz;
            let rnz = r20 * nx + r21 * ny + r22 * nz;
            v.normal[0] = rnx;
            v.normal[1] = rnz;
            v.normal[2] = rny;
        }

        placed.push(PlacedAbcObject {
            type_name: obj.type_name.clone(),
            model_filename: filename.clone(),
            skin_filename: resolved_skin.clone(),
            position: [
                pos.x * scale,
                pos.z * scale,
                pos.y * scale,
            ],
            rotation: [rot.x, rot.y, rot.z, rot.w],
            mesh: AbcMesh {
                vertices: world_verts,
                indices: base_mesh.indices.clone(),
            },
        });
    }

    log::info!("Extracted {} ABC objects from world", placed.len());
    placed
}

/// Backward-compatible alias for `extract_abc_objects`.
pub fn extract_barrel_objects(
    objects: &[WorldObject],
    rez_root: &str,
    scale: f32,
) -> Vec<PlacedAbcObject> {
    extract_abc_objects(objects, rez_root, scale)
}

/// Resolve a Lithtech asset path (e.g. "models\\decos\\barrel.abc") to an
/// actual filesystem path under the REZ root, handling case insensitivity.
/// Falls back to a recursive filename search if the exact directory structure
/// doesn't match (e.g. pickups organised in realm sub-directories).
fn resolve_rez_path(rez_root: &str, filename: &str) -> String {
    // Normalize separators
    let normalized = filename.replace('\\', "/");

    // Try the direct path first (uppercase, as REZ assets typically are)
    let upper_path = format!("{}/{}", rez_root, normalized.to_ascii_uppercase());
    if Path::new(&upper_path).exists() {
        return upper_path;
    }

    // Try as-is
    let direct_path = format!("{}/{}", rez_root, normalized);
    if Path::new(&direct_path).exists() {
        return direct_path;
    }

    // Case-insensitive file search: walk the REZ directory structure
    let parts: Vec<&str> = normalized.split('/').collect();
    let mut current = rez_root.to_string();

    for part in &parts {
        let target_lower = part.to_ascii_lowercase();
        let mut found = false;

        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.to_ascii_lowercase() == target_lower {
                    current = format!("{}/{}", current, name_str);
                    found = true;
                    break;
                }
            }
        }

        if !found {
            current = format!("{}/{}", current, part);
        }
    }

    if Path::new(&current).exists() {
        return current;
    }

    // Last resort: recursive filename search under the top-level category dir
    // (e.g. search REZ/MODELS for "KEY.ABC" when exact path didn't match)
    if let Some(top_dir) = parts.first() {
        let search_root = format!("{}/{}", rez_root, top_dir.to_ascii_uppercase());
        if let Some(basename) = parts.last() {
            let target = basename.to_ascii_uppercase();
            if let Some(found) = find_file_recursive(Path::new(&search_root), &target) {
                return found.to_string_lossy().to_string();
            }
        }
    }

    current
}

/// Recursively search for a file by uppercase name in a directory tree.
fn find_file_recursive(dir: &Path, target_upper: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, target_upper) {
                return Some(found);
            }
        } else if let Some(name) = path.file_name() {
            if name.to_string_lossy().to_uppercase() == target_upper {
                return Some(path);
            }
        }
    }
    None
}

// ─── IO Helpers ───

fn read_lt_string<R: Read>(reader: &mut R) -> Result<String> {
    let length = reader.read_u16::<LittleEndian>()?;
    let mut buffer = vec![0u8; length as usize];
    reader.read_exact(&mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).to_string())
}

fn read_abc_vector<R: Read>(reader: &mut R) -> Result<AbcVector> {
    Ok(AbcVector {
        x: reader.read_f32::<LittleEndian>()?,
        y: reader.read_f32::<LittleEndian>()?,
        z: reader.read_f32::<LittleEndian>()?,
    })
}
