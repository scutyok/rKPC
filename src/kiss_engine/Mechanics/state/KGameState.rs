#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LevelTransition {
    pub next_world: Option<String>,
    pub start_point: String,
    pub exit_type: ExitType,
}

#[derive(Debug, Default)]
pub struct GameState {
    pub current_objective: Option<ObjectivesState>,
    pub pending_level_transition: Option<LevelTransition>, // ExitTrigger writes here
}