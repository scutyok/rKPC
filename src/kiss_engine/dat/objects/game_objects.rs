//! GameObjectManager — thin orchestrator for all interactive game objects.
//!
//! Object structs, parsing, and per-frame logic live in their own modules
//! (CBarrel.rs, CDoorSliding.rs, etc.).  This file owns:
//!   * The `GameObject` enum wrapping every concrete type.
//!   * `GameObjectManager` — the active object list.
//!   * Per-frame `update()`, `interact()`, `apply_area_damage()`,
//!     `apply_point_damage()`, and `dynamic_lights()`.

use crate::abc::PlacedAbcObject;
use crate::dat::WorldObject;
use crate::lights::Light;
use crate::object_utils::{dist3, hide_draw_group, time_to_fraction};
use crate::types::DrawGroup;

use crate::CBarrel::{self, BarrelObject, BarrelState, EXPLOSION_FLASH_DURATION};
use crate::CCrate::{self, CrateObject, CrateState};
use crate::CDoorSliding::{self, DoorObject};
use crate::CLadder::{self, LadderObject};
use crate::CRotatingCeilingFan::{self, FanObject};
use crate::CSwitchRotating::{self, SwitchObject, INTERACT_RADIUS};
use crate::CTorch::{self, TorchObject};
use crate::CWater::{self, WaterObject};
use crate::CWindow::{self, WindowObject, WindowState};
use crate::DemoSkyWorldModel::{SkyModelInfo, SkyWorldModelObject};
use crate::OutsideDef::OutsideDefObject;
use crate::CPickupItem::{self, PickupItemObject};
use crate::CCreature::{self, CreatureObject};
use crate::scripted_sequence::{BspDoor, BspDoorState, ScriptRunner, ScriptCommand};











// ─── Explosion effect ────────────────────────────────────────────────────────

/// Transient explosion light flash managed by the object system.
/// The App merges these into the frame's lighting UBO each frame.
#[derive(Debug, Clone)]
pub struct ExplosionLight {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub radius: f32,
    pub time_remaining: f32,
}

impl ExplosionLight {
    /// Current intensity, fades linearly to zero over its lifetime.
    pub fn intensity(&self) -> f32 {
        (time_to_fraction(self.time_remaining, EXPLOSION_FLASH_DURATION) * 4.0).min(4.0)
    }

    /// Convert to a `lights::Light` for inclusion in the lighting UBO.
    pub fn to_light(&self) -> Light {
        Light {
            position: self.position,
            radius: self.radius,
            color: self.color,
            intensity: self.intensity(),
        }
    }
}

// ─── Game Object enum ────────────────────────────────────────────────────────

pub enum GameObject {
    Barrel(BarrelObject),
    Crate(CrateObject),
    Door(DoorObject),
    Switch(SwitchObject),
    Torch(TorchObject),
    Fan(FanObject),
    Window(WindowObject),
    Water(WaterObject),
    Ladder(LadderObject),
    SkyModel(SkyWorldModelObject),
    Outside(OutsideDefObject),
    Pickup(PickupItemObject),
    Creature(CreatureObject),
}

impl GameObject {
    pub fn position(&self) -> Option<[f32; 3]> {
        match self {
            Self::Barrel(o) => Some(o.position),
            Self::Crate(o) => Some(o.position),
            Self::Door(o) => Some(o.position),
            Self::Switch(o) => Some(o.position),
            Self::Torch(o) => Some(o.position),
            Self::Fan(o) => Some(o.position),
            Self::Window(o) => Some(o.position),
            Self::Pickup(o) => Some(o.position),
            Self::Creature(o) => Some(o.position),
            Self::Water(_) | Self::Ladder(_) | Self::SkyModel(_) | Self::Outside(_) => None,
        }
    }
}

// ─── Game Object Manager ─────────────────────────────────────────────────────

/// A BSP trigger volume that activates a script when the player presses E inside it.
#[derive(Debug, Clone)]
pub struct ScriptTrigger {
    pub name: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub activated: bool,
}

impl ScriptTrigger {
    pub fn contains(&self, pos: [f32; 3]) -> bool {
        pos[0] >= self.min[0] && pos[0] <= self.max[0]
            && pos[1] >= self.min[1] && pos[1] <= self.max[1]
            && pos[2] >= self.min[2] && pos[2] <= self.max[2]
    }
}

/// A BSP-based switch (CSwitchSlide / CSwitchRotating) — geometry is a world model, not an ABC.
#[derive(Debug, Clone)]
pub struct BspSwitch {
    pub name: String,
    pub center: [f32; 3],
    /// SCR script trigger names this switch activates (resolved from CScriptObject chain).
    pub script_targets: Vec<String>,
    pub activated: bool,
    pub draw_groups: Vec<usize>,
}

pub struct GameObjectManager {
    pub objects: Vec<GameObject>,
    pub explosion_lights: Vec<ExplosionLight>,
    pub player_in_water: bool,
    pub player_on_ladder: bool,
    pub player_outside: bool,
    pub bsp_doors: Vec<BspDoor>,
    pub scripts: Vec<(String, ScriptRunner)>,
    pub script_triggers: Vec<ScriptTrigger>,
    pub bsp_switches: Vec<BspSwitch>,
}

impl GameObjectManager {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            explosion_lights: Vec::new(),
            player_in_water: false,
            player_on_ladder: false,
            player_outside: false,
            bsp_doors: Vec::new(),
            scripts: Vec::new(),
            script_triggers: Vec::new(),
            bsp_switches: Vec::new(),
        }
    }

    /// Build the manager from DAT objects, placed ABC objects, BSP sub-models,
    /// torch flame entries, and pre-collected sky model infos.
    pub fn extract_from_dat(
        dat_objects: &[WorldObject],
        placed: &[PlacedAbcObject],
        first_draw_group: usize,
        bsp_submodels: &[(String, [f32; 3], Vec<usize>, f32)],
        torch_flames: &[(usize, usize, usize)],
        sky_models: &[SkyModelInfo],
        trigger_volumes: &[(String, [f32; 3], [f32; 3])],
        door_collision_ranges: &[(String, usize, usize)],
        collision_positions: &[cgmath::Vector3<f32>],
        scale: f32,
        creature_anim_data: &[(usize, Vec<u32>, Vec<u32>, u32)],
    ) -> Self {
        let mut mgr = Self::new();

        for (i, abc) in placed.iter().enumerate() {
            let dg = first_draw_group + i;
            let props = dat_objects.get(abc.dat_object_index);
            let pos = abc.position;

            match abc.type_name.as_str() {
                "CBarrel" => {
                    mgr.objects.push(GameObject::Barrel(
                        CBarrel::parse(pos, props, dg, &abc.skin_filename, scale),
                    ));
                }
                "CCrate" | "CModelBreakable" => {
                    mgr.objects.push(GameObject::Crate(CCrate::parse(pos, props, dg)));
                }
                "CDoorSliding" => {
                    let mesh_bounds = if !abc.mesh.vertices.is_empty() {
                        let mut bmin = [f32::MAX; 3];
                        let mut bmax = [f32::MIN; 3];
                        for v in &abc.mesh.vertices {
                            for k in 0..3 {
                                bmin[k] = bmin[k].min(v.pos[k]);
                                bmax[k] = bmax[k].max(v.pos[k]);
                            }
                        }
                        Some((bmin, bmax))
                    } else {
                        None
                    };
                    mgr.objects.push(GameObject::Door(CDoorSliding::parse(pos, props, dg, scale, mesh_bounds)));
                }
                "CSwitchRotating" => {
                    let sw = CSwitchRotating::parse_rotating(pos, props, dg);
                    println!("  Switch at ({:.2},{:.2},{:.2}) → target='{}'", pos[0], pos[1], pos[2], sw.target_name);
                    mgr.objects.push(GameObject::Switch(sw));
                }
                "CSwitchSlide" => {
                    let sw = CSwitchRotating::parse_slide(pos, props, dg);
                    println!("  SwitchSlide at ({:.2},{:.2},{:.2}) → target='{}'", pos[0], pos[1], pos[2], sw.target_name);
                    mgr.objects.push(GameObject::Switch(sw));
                }
                "CTorch" => {
                    let (fdg, fti) = torch_flames
                        .iter()
                        .find(|&&(ai, _, _)| ai == i)
                        .map(|&(_, fdg, fti)| (fdg, fti))
                        .unwrap_or((0, 0));
                    mgr.objects.push(GameObject::Torch(CTorch::parse(pos, props, dg, fdg, fti)));
                }
                "CWindow" | "CWindowShattering" => {
                    mgr.objects.push(GameObject::Window(CWindow::parse(pos, props, dg)));
                }
                "CWater" | "CWaterVolume" => {
                    mgr.objects.push(GameObject::Water(CWater::parse(pos, props, scale)));
                }
                "CLadder" => {
                    mgr.objects.push(GameObject::Ladder(CLadder::parse(pos, props, scale)));
                }
                "OutsideDef" => {
                    use crate::dat::PropertyValue;
                    let half = props
                        .and_then(|o| o.get_property("Dims"))
                        .and_then(|v| {
                            if let PropertyValue::Vector(vec) = v {
                                Some([vec.x * scale * 0.5, vec.z * scale * 0.5, vec.y * scale * 0.5])
                            } else {
                                None
                            }
                        })
                        .unwrap_or([5.0, 5.0, 5.0]);
                    mgr.objects.push(GameObject::Outside(OutsideDefObject {
                        min: [pos[0] - half[0], pos[1] - half[1], pos[2] - half[2]],
                        max: [pos[0] + half[0], pos[1] + half[1], pos[2] + half[2]],
                    }));
                }
                _ => {
                    // Pickup items (spinning/bobbing animation)
                    if abc.type_name.ends_with("Item_t") || abc.type_name == "CPickupTrigger" {
                        mgr.objects.push(GameObject::Pickup(
                            CPickupItem::parse(pos, dg),
                        ));
                    }
                    // Creatures (static placement for now)
                    else if abc.type_name.starts_with("CHeadless")
                        || abc.type_name.starts_with("CArachniclown")
                        || abc.type_name.starts_with("CBallBuster")
                        || abc.type_name.starts_with("CBatwing")
                        || abc.type_name.starts_with("CBlackwell")
                        || abc.type_name.starts_with("CBladeMaster")
                        || abc.type_name.starts_with("CFatLady")
                        || abc.type_name.starts_with("CGasBag")
                        || abc.type_name.starts_with("CGrinder")
                        || abc.type_name.starts_with("CHellSpore")
                        || abc.type_name.starts_with("CLarva")
                        || abc.type_name.starts_with("CMeanieBeanie")
                        || abc.type_name.starts_with("CPin")
                        || abc.type_name.starts_with("CRotCrawl")
                        || abc.type_name.starts_with("CStrongman")
                        || abc.type_name.starts_with("CStrutter")
                        || abc.type_name.starts_with("CStump")
                        || abc.type_name.starts_with("CTiberius")
                        || abc.type_name.starts_with("CTickler")
                        || abc.type_name.starts_with("CUniPsycho")
                        || abc.type_name.starts_with("CStarGrave")
                        || abc.type_name.starts_with("CFortunado")
                        || abc.type_name.starts_with("CRoly")
                    {
                        let mut creature = CCreature::parse(pos, dg, &abc.type_name);
                        // Set up animation data if available
                        if let Some((_, kf_indices, kf_times, duration)) =
                            creature_anim_data.iter().find(|(idx, _, _, _)| *idx == i)
                        {
                            let index_count = abc.mesh.indices.len() as u32;
                            CCreature::set_animation(
                                &mut creature,
                                kf_indices.clone(),
                                index_count,
                                kf_times,
                                *duration,
                            );
                        }
                        mgr.objects.push(GameObject::Creature(creature));
                    }
                }
            }
        }

        // BSP sub-models: rotating ceiling fans and doors
        for (name, pivot, dgs, z_height) in bsp_submodels {
            if name.to_ascii_uppercase().starts_with("CROTATINGCEILINGFAN") {
                mgr.objects.push(GameObject::Fan(
                    CRotatingCeilingFan::parse_from_bsp(*pivot, dgs.clone()),
                ));
            }
            let nl = name.to_lowercase();
            if nl.starts_with("door") || nl.starts_with("freezedoor") {
                // Find the collision vertex range for this door
                let coll_range = door_collision_ranges.iter()
                    .find(|(n, _, _)| n == &nl);
                let (coll_vertex_range, coll_base) = if let Some(&(_, start, end)) = coll_range {
                    (Some((start, end)), collision_positions[start..end].to_vec())
                } else {
                    (None, Vec::new())
                };
                mgr.bsp_doors.push(BspDoor {
                    name: nl,
                    draw_groups: dgs.clone(),
                    slide_distance: *z_height,
                    slide_offset: 0.0,
                    state: BspDoorState::Closed,
                    collision_vertex_range: coll_vertex_range,
                    collision_base_positions: coll_base,
                });
            }
        }

        // Sky world models (animated)
        for info in sky_models {
            mgr.objects.push(GameObject::SkyModel(
                SkyWorldModelObject::from_info(info, [0.0, 0.0, 0.0]),
            ));
        }

        // Register trigger volumes for script activation via E key
        for (name, vmin, vmax) in trigger_volumes {
            mgr.script_triggers.push(ScriptTrigger {
                name: name.to_lowercase(),
                min: *vmin,
                max: *vmax,
                activated: false,
            });
        }

        log::info!(
            "GameObjectManager: {} interactive + {} sky objects",
            mgr.objects.iter().filter(|o| !matches!(o, GameObject::SkyModel(_))).count(),
            mgr.objects.iter().filter(|o| matches!(o, GameObject::SkyModel(_))).count(),
        );
        mgr
    }

    /// Collect current AABBs of doors that should block the player.
    pub fn door_aabbs(&self) -> Vec<([f32; 3], [f32; 3])> {
        self.objects.iter().filter_map(|obj| {
            if let GameObject::Door(d) = obj {
                d.current_aabb()
            } else {
                None
            }
        }).collect()
    }

    // ── Per-frame update ─────────────────────────────────────────────────────

    /// Advance all object state machines, apply model-matrix overrides to
    /// `draw_groups`, and determine player zone membership.
    ///
    /// `_time` is the total elapsed time in seconds (reserved for future per-object phase effects).
    pub fn update(
        &mut self,
        dt: f32,
        _time: f32,
        player_pos: [f32; 3],
        draw_groups: &mut Vec<DrawGroup>,
    ) {
        let mut pending: Vec<([f32; 3], f32, f32, [f32; 3])> = Vec::new();

        for obj in &mut self.objects {
            match obj {
                GameObject::Barrel(b) => {
                    if let Some(expl) = CBarrel::update(b, dt) {
                        hide_draw_group(draw_groups, b.draw_group);
                        pending.push((expl.position, expl.radius, expl.damage, expl.color));
                    }
                }
                GameObject::Door(d) => CDoorSliding::update(d, dt, player_pos, draw_groups),
                GameObject::Switch(sw) => CSwitchRotating::update(sw, dt, draw_groups),
                GameObject::Fan(f) => CRotatingCeilingFan::update(f, dt, draw_groups),
                GameObject::Torch(t) => CTorch::update(t, dt, player_pos, draw_groups),
                GameObject::SkyModel(sky) => sky.update(dt, draw_groups),
                GameObject::Pickup(p) => CPickupItem::update(p, dt, _time, draw_groups),
                GameObject::Creature(c) => CCreature::update(c, dt, draw_groups),
                _ => {}
            }
        }

        for (pos, radius, damage, color) in pending {
            self.explosion_lights.push(ExplosionLight {
                position: pos,
                color,
                radius: radius * 2.5,
                time_remaining: EXPLOSION_FLASH_DURATION,
            });
            self.apply_area_damage(pos, radius, damage, draw_groups);
        }

        for el in &mut self.explosion_lights {
            el.time_remaining -= dt;
        }
        self.explosion_lights.retain(|el| el.time_remaining > 0.0);

        // Tick BSP doors
        for door in &mut self.bsp_doors {
            door.update(dt, draw_groups);
        }

        // Tick script runners and collect fired commands
        let mut fired_commands: Vec<ScriptCommand> = Vec::new();
        for (_name, runner) in &mut self.scripts {
            fired_commands.extend(runner.update(dt));
        }

        // Dispatch fired script commands
        for cmd in fired_commands {
            match cmd {
                ScriptCommand::TriggerDoorOpen { door_name } => {
                    for door in &mut self.bsp_doors {
                        if door.name == door_name {
                            door.open();
                        }
                    }
                }
            }
        }

        self.player_in_water = false;
        self.player_on_ladder = false;
        self.player_outside = false;
        for obj in &self.objects {
            match obj {
                GameObject::Water(w) if w.contains(player_pos) => { self.player_in_water = true; }
                GameObject::Ladder(l) if l.contains(player_pos) => { self.player_on_ladder = true; }
                GameObject::Outside(o) if o.contains(player_pos) => { self.player_outside = true; }
                _ => {}
            }
        }
    }

    /// Update collision mesh vertices for any BSP doors that are sliding open.
    pub fn update_door_collision(&self, collision_positions: &mut [cgmath::Vector3<f32>]) {
        for door in &self.bsp_doors {
            door.update_collision(collision_positions);
        }
    }

    // ── Interaction ──────────────────────────────────────────────────────────

    pub fn interact(&mut self, player_pos: [f32; 3], draw_groups: &mut Vec<DrawGroup>) {
        let mut targets_to_open: Vec<String> = Vec::new();

        for obj in &mut self.objects {
            if let GameObject::Switch(sw) = obj {
                if let Some(target) = CSwitchRotating::try_interact(sw, player_pos, draw_groups) {
                    println!("[INTERACT] Switch triggered → target='{}'", target);
                    targets_to_open.push(target);
                }
            }
        }

        for obj in &mut self.objects {
            if let GameObject::Door(d) = obj {
                let in_range = dist3(player_pos, d.position) < INTERACT_RADIUS;
                let switch_linked = !d.trigger_name.is_empty()
                    && targets_to_open.contains(&d.trigger_name);
                if in_range || switch_linked {
                    d.open();
                }
            }
        }

        // Start scripts whose trigger name matches a switch target (case-insensitive)
        for target in &targets_to_open {
            let target_lower = target.to_lowercase();
            for (name, runner) in &mut self.scripts {
                if name.to_lowercase() == target_lower {
                    println!("[SCRIPT] Starting script '{}' (switch target '{}')", name, target);
                    runner.start();
                }
            }
        }

        // BSP switches (CSwitchSlide/CSwitchRotating BSP sub-models)
        let mut bsp_switch_targets: Vec<String> = Vec::new();
        for sw in &mut self.bsp_switches {
            if !sw.activated && dist3(player_pos, sw.center) < INTERACT_RADIUS {
                println!("[BSP SWITCH] '{}' activated → scripts: {:?}", sw.name, sw.script_targets);
                sw.activated = true;
                bsp_switch_targets.extend(sw.script_targets.clone());
            }
        }
        for target in &bsp_switch_targets {
            for (name, runner) in &mut self.scripts {
                if name == target {
                    println!("[SCRIPT] Starting script '{}' (BSP switch)", name);
                    runner.start();
                }
            }
        }

        // Check if player is inside a trigger volume and start matching script
        let mut trigger_script_targets: Vec<String> = Vec::new();
        for trigger in &mut self.script_triggers {
            if !trigger.activated && trigger.contains(player_pos) {
                println!("[TRIGGER] Player inside volume '{}' → activating", trigger.name);
                trigger.activated = true;
                trigger_script_targets.push(trigger.name.clone());
            }
        }
        for tname in &trigger_script_targets {
            for (script_name, runner) in &mut self.scripts {
                // Match if script name appears in trigger name or vice versa
                if tname.contains(script_name.as_str())
                    || script_name.contains(tname.as_str())
                {
                    println!("[SCRIPT] Starting script '{}' (trigger volume '{}')", script_name, tname);
                    runner.start();
                }
            }
        }
    }

    // ── Damage API ───────────────────────────────────────────────────────────

    pub fn apply_area_damage(
        &mut self,
        origin: [f32; 3],
        radius: f32,
        damage: f32,
        draw_groups: &mut Vec<DrawGroup>,
    ) {
        // Collect which indices are barrels that just exploded so we can chain.
        let mut newly_exploding: Vec<usize> = Vec::new();

        for (idx, obj) in self.objects.iter_mut().enumerate() {
            match obj {
                GameObject::Barrel(b) => {
                    if b.state != BarrelState::Intact {
                        continue;
                    }
                    if dist3(origin, b.position) < radius {
                        if b.apply_damage(damage) {
                            newly_exploding.push(idx);
                        }
                    }
                }
                GameObject::Crate(c) => {
                    if c.state != CrateState::Intact {
                        continue;
                    }
                    if dist3(origin, c.position) < radius {
                        if c.apply_damage(damage) {
                            CCrate::on_destroy(c, draw_groups);
                        }
                    }
                }
                GameObject::Window(w) => {
                    if w.state != WindowState::Intact {
                        continue;
                    }
                    if dist3(origin, w.position) < radius {
                        if w.apply_damage(damage) {
                            CWindow::on_break(w, draw_groups);
                        }
                    }
                }
                _ => {}
            }
        }

        // Trigger chain explosions immediately (they'll be processed next frame).
        for idx in newly_exploding {
            if let GameObject::Barrel(b) = &mut self.objects[idx] {
                // Already set to Exploding by apply_damage above; light will be
                // emitted in the next update() call.
                let expl_pos = b.position;
                let expl_rad = b.explosion_radius;
                let expl_dmg = b.explosion_damage;
                let expl_col = b.element.flash_color();
                b.state = BarrelState::Exploding { timer: 0.0 };
                hide_draw_group(draw_groups, b.draw_group);
                self.explosion_lights.push(ExplosionLight {
                    position: expl_pos,
                    color: expl_col,
                    radius: expl_rad * 2.5,
                    time_remaining: EXPLOSION_FLASH_DURATION,
                });
                // Recursing here would cause borrow issues; caller will call
                // update() which drains pending explosions via area damage.
                let _ = (expl_pos, expl_rad, expl_dmg); // used above
            }
        }
    }

    /// Directly damage the object closest to `origin` (hit-scan style).
    /// Returns the amount of damage absorbed, or 0 if nothing was hit.
    pub fn apply_point_damage(&mut self, origin: [f32; 3], damage: f32, draw_groups: &mut Vec<DrawGroup>) -> f32 {
        let mut best_dist = 1.5_f32; // max hit-scan reach in Vulkan units
        let mut best_idx: Option<usize> = None;

        for (idx, obj) in self.objects.iter().enumerate() {
            if let Some(pos) = obj.position() {
                let d = dist3(origin, pos);
                if d < best_dist {
                    best_dist = d;
                    best_idx = Some(idx);
                }
            }
        }

        if let Some(idx) = best_idx {
            match &mut self.objects[idx] {
                GameObject::Barrel(b) => {
                    if b.apply_damage(damage) {
                        // Explosion will trigger in next update() via state machine.
                    }
                }
                GameObject::Crate(c) => {
                    if c.apply_damage(damage) {
                        CCrate::on_destroy(c, draw_groups);
                    }
                }
                GameObject::Window(w) => {
                    if w.apply_damage(damage) {
                        CWindow::on_break(w, draw_groups);
                    }
                }
                _ => return 0.0,
            }
            damage
        } else {
            0.0
        }
    }

    // ── Shadow caster export ──────────────────────────────────────────────

    /// Returns world-space positions of all creatures for blob shadow casting.
    pub fn shadow_caster_positions(&self) -> Vec<[f32; 3]> {
        let mut out = Vec::new();
        for obj in &self.objects {
            if let GameObject::Creature(c) = obj {
                out.push(c.position);
            }
        }
        out
    }

    // ── Dynamic light export ─────────────────────────────────────────────────

    /// Build the list of dynamic lights for this frame: explosion flashes +
    /// torch flicker lights.  The caller merges these with the static world
    /// lights before uploading the lighting UBO.
    /// Returns dynamic point lights this frame: explosion flashes + torch flicker.
    ///
    /// `time` is the total elapsed time in seconds used for torch flicker sine waves.
    pub fn dynamic_lights(&self, time: f32) -> Vec<Light> {
        let mut out: Vec<Light> = Vec::new();

        // Explosion flashes
        for el in &self.explosion_lights {
            if el.time_remaining > 0.0 {
                out.push(el.to_light());
            }
        }

        // Torch flicker
        for obj in &self.objects {
            if let GameObject::Torch(t) = obj {
                out.push(CTorch::dynamic_light(t, time));
            }
        }

        out
    }
}
