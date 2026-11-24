use bevy::prelude::*;

/// Authored presentation-graph gate. TEXTKEY `dis` and `en` commands change
/// this Bevy-owned state instead of leaving a source command interpreter alive.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationGraphEnabled(pub bool);

impl Default for AnimationGraphEnabled {
    fn default() -> Self {
        Self(true)
    }
}

/// Per-entity cursor over one immutable authored animation graph.
///
/// This contains only playback state. Nodes, edges, clips, and keyframes remain
/// borrowed from the entity's shared animation asset.
#[derive(Component, Clone, Debug, Default)]
pub struct AnimationGraphPlaybackState {
    pub current_animation_graph_node_index: u32,
    pub active_animation_graph_transition: Option<AnimationGraphTransition>,
    pub active_animation_graph_blend: Option<AnimationGraphBlend>,

    pub remaining_transition_steps: std::collections::VecDeque<AnimationGraphRouteStep>,
    pub route_owner: Option<AnimationGraphRouteOwner>,
}

#[derive(Clone, Debug)]
pub struct AnimationGraphRoute {
    pub source_node: u32,
    pub target_node: u32,
    pub steps: std::collections::VecDeque<AnimationGraphRouteStep>,
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationGraphRouteStep {
    pub source_node: u32,
    pub target_node: u32,
    pub edge_index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationGraphRouteOwner {
    pub request_id: usize,
    pub target_node: u32,
    pub playback_generation: u64,
}

impl AnimationGraphPlaybackState {
    pub fn active_blend_duration_milliseconds(&self) -> Option<u32> {
        self.active_animation_graph_blend
            .as_ref()
            .map(|blend| blend.timer.duration().as_millis().min(u128::from(u32::MAX)) as u32)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationGraphTransition {
    pub target_animation_graph_node_index: u32,
}

#[derive(Clone, Debug)]
pub struct AnimationGraphBlend {
    pub timer: Timer,
}

impl AnimationGraphBlend {
    pub fn from_duration_milliseconds(milliseconds: u32) -> Option<Self> {
        (milliseconds != 0).then(|| Self {
            timer: Timer::new(
                std::time::Duration::from_millis(u64::from(milliseconds)),
                TimerMode::Once,
            ),
        })
    }
}
