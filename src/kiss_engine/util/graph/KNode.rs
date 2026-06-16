use crate::util::geometry::AABB;

#[derive(Clone, Debug)]

pub struct Node {
    pub aabb: AABB,
    pub start: usize,         // Index into tri_indices (leaf only)
    pub count: usize,         // Number of triangles (0 if internal)
    pub left: Option<usize>,  // Left child index
    pub right: Option<usize>, // Right child index
}