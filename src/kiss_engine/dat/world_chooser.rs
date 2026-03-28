/// Application loading state
#[derive(Clone, Debug, PartialEq)]
pub enum LoadingState {
    Loading(String),
    Ready,
}

impl Default for LoadingState {
    fn default() -> Self {
        LoadingState::Ready
    }
}

/// World chooser UI state
#[derive(Clone, Debug)]
pub struct WorldChooser {
    pub visible: bool,
    pub worlds: Vec<String>,
    pub selected_index: usize,
    pub pending_load: Option<String>,
    pub scroll_offset: f32,
}

impl WorldChooser {
    pub fn new() -> Self {
        let worlds = Self::scan_worlds();
        Self {
            visible: false,
            worlds,
            selected_index: 0,
            pending_load: None,
            scroll_offset: 0.0,
        }
    }

    fn scan_worlds() -> Vec<String> {
        let mut worlds = Vec::new();
        let base_path = std::path::Path::new("REZ/WORLDS");

        if let Ok(entries) = std::fs::read_dir(base_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(files) = std::fs::read_dir(&path) {
                        for file in files.filter_map(|f| f.ok()) {
                            let file_path = file.path();
                            if let Some(ext) = file_path.extension() {
                                if ext.eq_ignore_ascii_case("dat") {
                                    if let Some(path_str) = file_path.to_str() {
                                        worlds.push(path_str.replace('\\', "/"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        worlds.sort();
        worlds
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn select_index(&mut self, index: usize) {
        if index < self.worlds.len() {
            self.selected_index = index;
        }
    }

    pub fn confirm_selection(&mut self) -> Option<String> {
        if let Some(world) = self.worlds.get(self.selected_index) {
            self.visible = false;
            Some(world.clone())
        } else {
            None
        }
    }

    pub fn get_world_display_name(path: &str) -> String {
        std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string()
    }

    pub fn take_pending_load(&mut self) -> Option<String> {
        self.pending_load.take()
    }
}
