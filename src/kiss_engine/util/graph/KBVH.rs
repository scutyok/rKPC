//******************************************************************/
//
// Bounding Volume Hierarchy (BVH) for spatial partitioning.
// 
// Uses a top-down object-median split on the largest axis.
//
//******************************************************************/

pub use crate::util::math::{Vector3};
pub use crate::util::geometry::{AABB};
pub use crate::util::graph::{Node};

//******************************************************************/
//
// Data Structure
//
//******************************************************************/

#[derive(Clone)]

pub struct Bvh {
    pub nodes: Vec<Node>,        // Linearized tree nodes
    pub tri_indices: Vec<usize>, // Global triangle index buffer
}

//******************************************************************/
//
// Traits/Methods
//
//******************************************************************/

impl Bvh {
    // Builds a new BVH from vertex positions and primitive indices.
    pub fn new(positions: &[Vector3], indices: &[u32]) -> Self {
        let tri_count = indices.len() / 3;
        let mut tri_indices: Vec<usize> = (0..tri_count).collect();
        let mut nodes: Vec<Node> = Vec::new();
        let mut tri_storage: Vec<usize> = Vec::with_capacity(tri_count);

        if tri_count == 0 {
            return Bvh { nodes, tri_indices: tri_storage };
        }

        Self::build_recursive(
            positions,
            indices,
            &mut tri_indices[..],
            &mut nodes,
            &mut tri_storage,
        );

        Bvh { nodes, tri_indices: tri_storage }
    }

    // Recursively splits geometry into a tree of AABBs.
    fn build_recursive(
        positions: &[Vector3],
        indices: &[u32],
        tri_indices: &mut [usize],
        nodes: &mut Vec<Node>,
        tri_storage: &mut Vec<usize>,
    ) -> usize {
        
        //1. Compute Node AABB

        let node_aabb = tri_indices
            .iter()
            .map(|&ti| AABB::compute_tri_aabb(positions, indices, ti))
            .fold(None, |acc: Option<AABB>, aabb| {
                Some(match acc {
                    None => aabb,
                    Some(prev) => AABB::merge_aabb(&prev, &aabb),
                })
            })
            .unwrap_or(AABB { 
                min: Vector3::new(0.0, 0.0, 0.0), 
                max: Vector3::new(0.0, 0.0, 0.0) 
            });

        let node_index = nodes.len();
        nodes.push(Node {
            aabb: node_aabb.clone(),
            start: 0,
            count: 0,
            left: None,
            right: None,
        });

        //2. Leaf Threshold Check

        if tri_indices.len() <= 8 {
            let start = tri_storage.len();
            tri_storage.extend_from_slice(tri_indices);
            
            let node = &mut nodes[node_index];
            node.start = start;
            node.count = tri_indices.len();
            return node_index;
        }

        //3. Split Strategy (Object Median)

        let ext = node_aabb.max - node_aabb.min;
        let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } 
                   else if ext.y >= ext.x && ext.y >= ext.z { 1 } 
                   else { 2 };

        tri_indices.sort_unstable_by(|&a, &b| {
            let ca = AABB::compute_centroid(positions, indices, a);
            let cb = AABB::compute_centroid(positions, indices, b);
            
            let va = if axis == 0 { ca.x } else if axis == 1 { ca.y } else { ca.z };
            let vb = if axis == 0 { cb.x } else if axis == 1 { cb.y } else { cb.z };
            
            va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
        });

        //4. Recurse

        let mid = tri_indices.len() / 2;
        let (left_slice, right_slice) = tri_indices.split_at_mut(mid);
        
        let left = Self::build_recursive(positions, indices, left_slice, nodes, tri_storage);
        let right = Self::build_recursive(positions, indices, right_slice, nodes, tri_storage);

        let node = &mut nodes[node_index];
        node.left = Some(left);
        node.right = Some(right);
        
        node_index
    }

    // Traverses the BVH to find triangles intersecting the given AABB.
    pub fn query_aabb(&self, aabb: &AABB, out: &mut Vec<usize>) {
        if self.nodes.is_empty() { return; }
        
        let mut stack: Vec<usize> = Vec::with_capacity(32);
        stack.push(0);

        while let Some(idx) = stack.pop() {
            let node = &self.nodes[idx];
            
            if !node.aabb.intersects(aabb) {
                continue;
            }

            if node.count > 0 {
                // Leaf: add triangles to output
                for i in 0..node.count {
                    out.push(self.tri_indices[node.start + i]);
                }
            } else {
                // Internal: push children
                if let Some(r) = node.right { stack.push(r); }
                if let Some(l) = node.left { stack.push(l); }
            }
        }
    }
}