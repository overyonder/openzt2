use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::ui::picking::UiPointerCapture;

/// Whether a press at the scripted pointer would reach a node, judged by the
/// game's alpha-aware picking result used for control activation.
#[derive(SystemParam)]
pub(crate) struct VerificationPointerReach<'w, 's> {
    capture: Res<'w, UiPointerCapture>,
    interactive: Query<'w, 's, (), With<Interaction>>,
    parents: Query<'w, 's, &'static ChildOf>,
    named_nodes: Query<'w, 's, (Entity, &'static Name), With<ComputedNode>>,
    clips: Query<'w, 's, &'static bevy::ui::CalculatedClip>,
}

impl VerificationPointerReach<'_, '_> {
    /// UI nodes with this name, in no particular order.
    pub(crate) fn nodes_named(&self, name: &str) -> Vec<Entity> {
        self.named_nodes
            .iter()
            .filter(|(_, node_name)| node_name.as_str() == name)
            .map(|(entity, _)| entity)
            .collect()
    }

    /// Whether a scrolling or clipping ancestor hides the point, as picking treats it.
    pub(crate) fn is_clipped_at(&self, entity: Entity, point: Vec2) -> bool {
        self.clips
            .get(entity)
            .is_ok_and(|clip| !clip.clip.contains(point))
    }

    /// A control must own the game's pointer hit. Noninteractive labels may
    /// resolve to their containing control, but covered buttons cannot.
    pub(crate) fn pointer_reaches(&self, target: Entity) -> bool {
        let frontmost = self.capture.target;
        frontmost.is_some_and(|frontmost| {
            self.is_self_or_ancestor(target, frontmost)
                || (!self.interactive.contains(target)
                    && self.is_self_or_ancestor(frontmost, target))
        })
    }

    fn is_self_or_ancestor(&self, ancestor: Entity, mut entity: Entity) -> bool {
        loop {
            if entity == ancestor {
                return true;
            }
            match self.parents.get(entity) {
                Ok(parent) => entity = parent.parent(),
                Err(_) => return false,
            }
        }
    }
}
