use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    Idle,
    Waiting,
    Recording,
    Transcribing,
    Error,
}

impl std::fmt::Display for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppState::Idle => write!(f, "idle"),
            AppState::Waiting => write!(f, "waiting"),
            AppState::Recording => write!(f, "recording"),
            AppState::Transcribing => write!(f, "transcribing"),
            AppState::Error => write!(f, "error"),
        }
    }
}

#[derive(Clone)]
pub struct AppStateManager {
    state: Arc<RwLock<AppState>>,
}

impl AppStateManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AppState::Idle)),
        }
    }

    pub fn get(&self) -> AppState {
        *self.state.read()
    }

    pub fn set(&self, new_state: AppState) {
        let mut state = self.state.write();
        log::info!("State: {} -> {}", *state, new_state);
        *state = new_state;
    }

    pub fn is_idle(&self) -> bool {
        self.get() == AppState::Idle
    }
}
