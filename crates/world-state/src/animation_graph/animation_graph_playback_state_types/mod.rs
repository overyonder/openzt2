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
    pub requested_blend_duration_milliseconds: Option<u32>,

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
        self.requested_blend_duration_milliseconds
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationGraphTransition {
    pub target_animation_graph_node_index: u32,
    pub chain_cursor: Option<AnimationGraphChainCursor>,
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationGraphChainCursor {
    pub edge_index: usize,
    pub next_clip: usize,
}
