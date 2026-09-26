use openzt2_game_data::animation::animation_clip_metadata::AuthoredAnimationMarker;

#[derive(Clone, Copy)]
pub(super) enum NetImmerseKfAnimationChannel {
    Translation,
    Rotation,
    Scale,
}

pub(super) struct SampledNetImmerseKfAnimationTrack {
    pub(super) skeleton_joint_node_index: usize,
    pub(super) animation_channel: NetImmerseKfAnimationChannel,
    pub(super) keyframe_times_seconds: Vec<f32>,
    pub(super) keyframe_values: Vec<f32>,
    pub(super) gltf_interpolation_mode: &'static str,
}

pub(super) struct ParsedNetImmerseKfAnimation {
    pub(super) animation_clip_asset_key: String,
    pub(super) skeleton_joint_names: Vec<String>,
    pub(super) sampled_animation_tracks: Vec<SampledNetImmerseKfAnimationTrack>,
    pub(super) duration_seconds: f32,
    pub(super) playback_is_looped: bool,
    pub(super) accumulation_root_name: String,
    pub(super) animation_markers: Vec<AuthoredAnimationMarker>,
}
