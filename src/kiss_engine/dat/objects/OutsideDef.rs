//! OutsideDef — marks regions of a level that are considered "outside".
//!
//! Blood2 / KPC uses OutsideDef zones to control:
//!   * Whether rain / weather effects are active.
//!   * Whether the sky is visible (the sky rendering pass is enabled in these zones).
//!   * Sound reverb presets (large open space vs. interior).
//!
//! In this renderer we record OutsideDef volumes so future systems (rain,
//! reverb, skybox visibility) can query whether the player is outdoors.
//!
//! DAT properties read:
//!   `Pos`   — centre of the zone (Vector3, Lithtech coords).
//!   `Dims`  — half-extents of the AABB  (Vector3, Lithtech units).

/// An axis-aligned "outside" zone parsed from the DAT file.
#[derive(Debug, Clone)]
pub struct OutsideDefObject {
    /// Minimum corner of the AABB in Vulkan/renderer coordinates.
    pub min: [f32; 3],
    /// Maximum corner of the AABB.
    pub max: [f32; 3],
}

impl OutsideDefObject {
    /// Returns `true` if the point `p` (Vulkan coords) is inside this zone.
    pub fn contains(&self, p: [f32; 3]) -> bool {
        p[0] >= self.min[0]
            && p[0] <= self.max[0]
            && p[1] >= self.min[1]
            && p[1] <= self.max[1]
            && p[2] >= self.min[2]
            && p[2] <= self.max[2]
    }
}
