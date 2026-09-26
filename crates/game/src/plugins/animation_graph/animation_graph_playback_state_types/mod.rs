use bevy::prelude::*;

/// Authored presentation-graph gate. TEXTKEY `dis` and `en` commands change
/// this Bevy-owned state instead of leaving a source command interpreter alive.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AnimationGraphEnabled(pub(crate) bool);

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
pub(crate) struct AnimationGraphPlaybackState {
    pub(crate) current_animation_graph_node_index: u32,
    pub(crate) active_animation_graph_transition: Option<AnimationGraphTransition>,
    pub(crate) active_animation_graph_blend: Option<AnimationGraphBlend>,
}

impl AnimationGraphPlaybackState {
    pub(crate) fn active_blend_duration_milliseconds(&self) -> Option<u32> {
        self.active_animation_graph_blend
            .as_ref()
            .map(|animation_graph_blend| animation_graph_blend.duration_milliseconds)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AnimationGraphTransition {
    pub target_animation_graph_node_index: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AnimationGraphBlend {
    pub elapsed_milliseconds: u32,
    pub duration_milliseconds: u32,
}
