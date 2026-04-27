//******************************************************************/
//
// CSwitchSlide — a button/slide-switch activated by pressing E.
//
// Same behaviour as CSwitchRotating but animates as a translation rather than
// a rotation (`RotationAngle` is 0). The `SwitchObject` type and all logic
// live in `CSwitchRotating`; re-export for documentation purposes.
//
//******************************************************************/

pub use crate::CSwitchRotating::SwitchObject;
pub use crate::CSwitchRotating::{parse_slide as parse, try_interact, update};
