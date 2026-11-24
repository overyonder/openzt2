use bevy::{prelude::*, time::Virtual};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;

use super::simulation_control_types::{
    AuthoredUiTemporarySimulationPause, ImmersiveSimulationPauseClaim, SetSimulationSpeed,
    SimulationControl, SimulationControlChanged, SimulationPauseRequest, UiDocumentPauseTransfer,
};
use super::simulation_speed_override::SimulationSpeedOverride;

pub(super) fn apply_simulation_control_requests(
    mut speed_requests: MessageReader<SetSimulationSpeed>,
    mut pause_requests: MessageReader<SimulationPauseRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut roots: Query<&mut SimulationControl, With<WorldRoot>>,
    mut control_changes: MessageWriter<SimulationControlChanged>,
) {
    let (Some(definitions), Ok(mut control)) =
        (active_definitions.get(&definitions), roots.single_mut())
    else {
        return;
    };
    let mut next = *control;
    let speed_count = definitions.timing().speed_multipliers.len();
    for request in speed_requests.read() {
        if usize::from(request.tier) < speed_count {
            next.speed_tier = request.tier;
        }
    }
    for request in pause_requests.read() {
        next.apply_pause_request(*request);
    }
    publish_control_change(*control, next, &mut control_changes);
    control.set_if_neq(next);
}

pub(super) fn simulation_speed_inputs_changed(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    controls: Query<(), (With<WorldRoot>, Changed<SimulationControl>)>,
    speed_override: Res<SimulationSpeedOverride>,
) -> bool {
    active_definitions.is_changed()
        || definitions.is_changed()
        || speed_override.is_changed()
        || !controls.is_empty()
}

pub(super) fn project_simulation_speed(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    roots: Query<&SimulationControl, With<WorldRoot>>,
    mut virtual_time: ResMut<Time<Virtual>>,
    speed_override: Res<SimulationSpeedOverride>,
) {
    let Ok(control) = roots.single() else {
        return;
    };
    let Some(speed) = active_definitions
        .get(&definitions)
        .and_then(|definitions| {
            definitions
                .timing()
                .speed_multipliers
                .get(usize::from(control.speed_tier))
        })
    else {
        return;
    };
    let relative_speed =
        f32::from(speed.numerator) / f32::from(speed.denominator) * speed_override.0;
    if virtual_time.relative_speed() != relative_speed {
        virtual_time.set_relative_speed(relative_speed);
    }
    // Paused worlds still run fixed player-owned transactions.
    if virtual_time.is_paused() {
        virtual_time.unpause();
    }
}

type TemporaryPauseClaim = Or<(
    With<AuthoredUiTemporarySimulationPause>,
    With<UiDocumentPauseTransfer>,
)>;
type PauseVisibilityNode = Or<(
    With<Node>,
    With<Children>,
    With<UiDocumentOwner>,
    TemporaryPauseClaim,
)>;
type ChangedPauseClaims = Or<(
    Changed<AuthoredUiTemporarySimulationPause>,
    Changed<UiDocumentPauseTransfer>,
    Added<ImmersiveSimulationPauseClaim>,
    (TemporaryPauseClaim, Changed<InheritedVisibility>),
    (With<WorldRoot>, Changed<SimulationControl>),
)>;
type ChangedPauseAncestry = (
    PauseVisibilityNode,
    Or<(
        Changed<Visibility>,
        Changed<ChildOf>,
        Added<UiDocumentOwner>,
    )>,
);
type RemovedPauseClaims<'w, 's> = (
    RemovedComponents<'w, 's, AuthoredUiTemporarySimulationPause>,
    RemovedComponents<'w, 's, UiDocumentPauseTransfer>,
    RemovedComponents<'w, 's, ImmersiveSimulationPauseClaim>,
    RemovedComponents<'w, 's, Visibility>,
    RemovedComponents<'w, 's, InheritedVisibility>,
    RemovedComponents<'w, 's, ChildOf>,
    RemovedComponents<'w, 's, UiDocumentOwner>,
);

pub(super) fn pause_claim_inputs_changed(
    claims: Query<(), ChangedPauseClaims>,
    ancestry: Query<(), ChangedPauseAncestry>,
    (
        mut removed_holds,
        mut removed_transfers,
        mut removed_immersive,
        mut removed_visibility,
        mut removed_inherited,
        mut detached,
        mut removed_owners,
    ): RemovedPauseClaims,
) -> bool {
    let removed = (removed_holds.read().count() != 0)
        | (removed_transfers.read().count() != 0)
        | (removed_immersive.read().count() != 0)
        | (removed_visibility.read().count() != 0)
        | (removed_inherited.read().count() != 0)
        | (detached.read().count() != 0)
        | (removed_owners.read().count() != 0);
    removed || !claims.is_empty() || !ancestry.is_empty()
}

type PauseClaimVisibility = (
    Entity,
    AnyOf<(
        &'static AuthoredUiTemporarySimulationPause,
        &'static UiDocumentPauseTransfer,
    )>,
    &'static InheritedVisibility,
);

pub(super) fn project_active_simulation_pause_claims(
    temporary_holds: Query<PauseClaimVisibility, TemporaryPauseClaim>,
    immersive_holds: Query<(), With<ImmersiveSimulationPauseClaim>>,
    authored_visibility: Query<&Visibility, With<UiDocumentOwner>>,
    visibility: Query<&Visibility>,
    parents: Query<&ChildOf>,
    mut roots: Query<&mut SimulationControl, With<WorldRoot>>,
    mut control_changes: MessageWriter<SimulationControlChanged>,
) {
    let Ok(mut control) = roots.single_mut() else {
        return;
    };
    let mut requested = temporary_holds
        .iter()
        .filter(|(_, (claim, transfer), inherited)| {
            inherited.get()
                && (claim.is_some_and(AuthoredUiTemporarySimulationPause::hold_is_requested)
                    || transfer.is_some_and(|transfer| transfer.0))
        })
        .peekable();
    let held = if requested.peek().is_none() {
        false
    } else {
        // Explicitly visible nodes still lose their claim under a hidden authored ancestor.
        let forced = authored_visibility
            .iter()
            .any(|visibility| *visibility == Visibility::Visible);
        !forced
            || requested.any(|(node, _, _)| {
                std::iter::once(node)
                    .chain(parents.iter_ancestors::<ChildOf>(node))
                    .all(|ancestor| {
                        !visibility
                            .get(ancestor)
                            .is_ok_and(|visibility| *visibility == Visibility::Hidden)
                    })
            })
    };
    let mut next = *control;
    next.project_pause_claims(held, !immersive_holds.is_empty());
    publish_control_change(*control, next, &mut control_changes);
    control.set_if_neq(next);
}

fn publish_control_change(
    previous: SimulationControl,
    next: SimulationControl,
    changes: &mut MessageWriter<SimulationControlChanged>,
) {
    if previous.speed_tier != next.speed_tier || previous.paused != next.paused {
        changes.write(SimulationControlChanged {
            speed_tier: next.speed_tier,
            paused: next.paused,
        });
    }
}
