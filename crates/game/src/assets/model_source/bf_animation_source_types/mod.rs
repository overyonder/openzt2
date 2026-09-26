#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum BlueFangAnimationProperty {
    Translation,
    Rotation,
    Scale,
}

pub(super) struct BlueFangAnimationTrack {
    pub(super) animated_property: BlueFangAnimationProperty,
    pub(super) keyframe_times_seconds: Vec<f32>,
    pub(super) keyframe_values: Vec<f32>,
}

pub(super) struct BlueFangAnimationNode {
    pub(super) skeleton_joint_name: String,
    pub(super) animation_tracks: Vec<BlueFangAnimationTrack>,
}

pub(super) struct BlueFangAnimationMarker {
    pub(super) event_time_seconds: f32,
    pub(super) marker_name: String,
}

pub(super) struct ParsedBlueFangAnimation {
    pub(super) animation_nodes: Vec<BlueFangAnimationNode>,
    pub(super) duration_seconds: f32,
    pub(super) authored_flags: u16,
    pub(super) animation_markers: Vec<BlueFangAnimationMarker>,
}
