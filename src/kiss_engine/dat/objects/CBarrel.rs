enum Realm {
    Water,
    Earth,
    Air,
    Fire,
}

pub struct CBarrel {
    // position in 3D space
    pub CBarrel_pos_x: f32,
    pub CBarrel_pos_y: f32,
    pub CBarrel_pos_z: f32,

    // proprieties
    pub CBarrel_breakable: bool,

    pub CBarrel_explosive: bool,
    pub CBarrel_explosion_radius: f32,
    pub CBarrel_explosion_damage: f32,

    pub CBarrel_gravity_force: f32,
    pub CBarrel_health: f32,

    // barrel type
    pub CBarrel_type: Realm,
}

impl CBarrel {
    pub fn read_from_dat() -> Result<Self> {
        // TODO: use abc.rs

        // TODO: read objects proprieties and position from .DAT file
        // TODO: initialize proprieties
    }

    pub fn explode() -> Result<Self> {
        // TODO: check for other barrels in the radius
        // TODO: animate using keyframes
    }

    pub fn check_damaged() -> Result<Self> {
        // TODO: if damage taken > CBarrel_health
        //           then
        //               call explode method

        //               destroy the .ABC model

        //               spawn the .DTX for explosion
        //               spawn the explosion radius ( 20 < planes )

        //               expand explosion radius for 10 units then make it disappear
        //                      if radius hits another CBarrel
        //                         then call explode method for that CBarrel
        //
        //                      if radius hits player/enemy
        //                         then call damage methods for each entity hit
        //
        //
    }
}
