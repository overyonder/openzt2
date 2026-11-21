use super::{
    bf_animation_cubic_keyframe_sampling::BlueFangScalarCurve,
    bf_animation_quaternion_keyframe_sampling::BlueFangQuaternionCurve,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum BlueFangAnimationProperty {
    Translation,
    Rotation,
    Scale,
}

pub(super) enum BlueFangAnimationTrack {
    Translation(BlueFangScalarCurve<3>),
    TranslationAxes([Option<BlueFangScalarCurve<1>>; 3]),
    Rotation(BlueFangQuaternionCurve),
    Euler([Option<BlueFangScalarCurve<1>>; 3]),
    Scale(BlueFangScalarCurve<3>),
}

impl BlueFangAnimationTrack {
    pub(super) fn property(&self) -> BlueFangAnimationProperty {
        match self {
            Self::Translation(_) | Self::TranslationAxes(_) => {
                BlueFangAnimationProperty::Translation
            }
            Self::Rotation(_) | Self::Euler(_) => BlueFangAnimationProperty::Rotation,
            Self::Scale(_) => BlueFangAnimationProperty::Scale,
        }
    }

    pub(super) fn end_seconds(&self) -> Option<f32> {
        match self {
            Self::Translation(curve) | Self::Scale(curve) => curve.end_seconds(),
            Self::Rotation(curve) => curve.end_seconds(),
            Self::TranslationAxes(axes) | Self::Euler(axes) => axes
                .iter()
                .flatten()
                .filter_map(BlueFangScalarCurve::end_seconds)
                .reduce(f32::max),
        }
    }
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
