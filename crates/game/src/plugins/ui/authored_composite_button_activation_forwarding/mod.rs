use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use super::authored_button_runtime_policy::UiButtonPolicy;
use super::authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId};
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Forwards composite button activations to the configured child controls.
pub(super) fn forward_authored_composite_button_activations_to_projected_child_controls(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    composites: Query<(&UiButtonPolicy, &UiDocumentOwner)>,
    nodes: Query<(Entity, &UiNodeId, &UiDocumentOwner)>,
) {
    for activation in activations.read() {
        let Ok((policy, owner)) = composites.get(activation.node) else {
            continue;
        };
        let target = match activation.trigger {
            UiTrigger::Press | UiTrigger::Submit | UiTrigger::On | UiTrigger::Off
                if policy.child_button != openzt2_game_data::AssetId::default() =>
            {
                policy.child_button
            }
            UiTrigger::Enter | UiTrigger::Leave
                if policy.hover_child != openzt2_game_data::AssetId::default() =>
            {
                policy.hover_child
            }
            _ => continue,
        };
        let Some(node) = nodes.iter().find_map(|(entity, id, candidate_owner)| {
            (candidate_owner.0 == owner.0 && id.id == target).then_some(entity)
        }) else {
            continue;
        };
        // A malformed self-link cannot create an activation loop. The native
        // validator rejects unknown references; this guards the one remaining
        // degenerate relation without retaining a visited set.
        if node == activation.node {
            continue;
        }
        let routed = UiNodeActivated {
            source: activation.source,
            node,
            trigger: activation.trigger,
        };
        commands.queue(move |world: &mut World| {
            world.write_message(routed);
        });
    }
}
