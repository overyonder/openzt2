use bevy::prelude::*;

#[derive(Component, Clone, Debug, Default)]
pub(super) struct AuthoredAnimationEventCursor {
    pub(super) playback_generation: u64,
    pub(super) animation_clip_asset_key: String,
    pub(super) previous_elapsed_milliseconds: u32,
    pub(super) requires_initial_event_sample: bool,
}
