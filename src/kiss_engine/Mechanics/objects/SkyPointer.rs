//! SkyPointer — a lightweight entity that names a sky BSP world model for
//! inclusion in the sky rendering pass.
//!
//! In Blood2 / KPC the SkyPointer usually has the same name as the world model
//! it refers to and is combined with a `DemoSkyWorldModel` of the same name.
//! We treat every SkyPointer `Name` as an additional sky model name so the
//! world loader can include it in the sky draw-group set without it being
//! explicitly listed in a hardcoded array.
//!
//! DAT properties read:
//!   `Name`          (String)  — also the world-model name to include in sky.
//!   `SkyObjectName` (String)  — explicit reference (same as Name when present).

/// Runtime sky-pointer entry.
///
/// No per-frame animation; this type only serves as a record of which world
/// model names should be included in the sky rendering pass.
#[derive(Debug, Clone)]
pub struct SkyPointerObject {
    /// Name of this pointer entity / the sky world model it points to.
    pub name: String,
    /// Explicit target model name (may equal `name`; empty when absent).
    pub sky_object_name: String,
}

impl SkyPointerObject {
    /// Returns the world-model name this pointer targets: `sky_object_name` if
    /// non-empty, otherwise falls back to `name`.
    pub fn target_name(&self) -> &str {
        if !self.sky_object_name.is_empty() {
            &self.sky_object_name
        } else {
            &self.name
        }
    }
}
