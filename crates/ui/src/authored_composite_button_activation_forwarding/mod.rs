use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use super::authored_button_runtime_policy::UiButtonPolicy;
use super::authored_ui_node_projection_components::UiDocumentOwner;
use super::authored_ui_target_lookup::AuthoredUiTargetLookup;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Forwards composite button activations to the configured child controls.
pub(super) fn forward_authored_composite_button_activations_to_projected_child_controls(
    mut messages: ParamSet<(
        MessageReader<UiNodeActivated>,
        MessageWriter<UiNodeActivated>,
    )>,
    composites: Query<(&UiButtonPolicy, &UiDocumentOwner)>,
    targets: AuthoredUiTargetLookup,
) {
    let mut forwarded = Vec::new();
    for activation in messages.p0().read() {
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
        let Some(node) = targets.candidates(owner.0, target).next() else {
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
        forwarded.push(routed);
    }
    let _ = messages.p1().write_batch(forwarded);
}
