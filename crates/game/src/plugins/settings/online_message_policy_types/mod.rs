use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnlineMessagePolicy {
    pub enabled: bool,
}

impl Default for OnlineMessagePolicy {
    fn default() -> Self {
        Self { enabled: true }
    }
}
