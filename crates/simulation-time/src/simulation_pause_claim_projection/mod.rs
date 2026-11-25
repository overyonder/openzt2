use bevy::{
    app::{HierarchyPropagatePlugin, Propagate, PropagateSet},
    prelude::*,
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_world_state::{
    simulation_time::simulation_control_types::{
        AuthoredUiTemporarySimulationPause, ImmersiveSimulationPauseClaim, SimulationControl,
        SimulationControlChanged, SimulationPaused, SimulationSpeedTier, UiDocumentPauseTransfer,
    },
    world_spawn::world_membership_types::WorldRoot,
};

#[derive(Component, Clone, PartialEq)]
struct PauseAncestorHidden;

#[derive(Component)]
struct AuthoredUiForcedVisible;

type TemporaryPauseClaim = Or<(
    With<AuthoredUiTemporarySimulationPause>,
    With<UiDocumentPauseTransfer>,
)>;
type VisibilitySource = Or<(With<Children>, TemporaryPauseClaim)>;
type ChangedVisibilitySource = Or<(
    Changed<Visibility>,
    Added<Children>,
    Added<AuthoredUiTemporarySimulationPause>,
    Added<UiDocumentPauseTransfer>,
)>;

pub(super) fn configure_pause_claim_projection(app: &mut App) {
    let propagation = PropagateSet::<PauseAncestorHidden>::default();
    app.add_plugins(HierarchyPropagatePlugin::<PauseAncestorHidden>::new(
        PostUpdate,
    ))
    .add_systems(
        PostUpdate,
        (
            project_hidden_ancestor_sources
                .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                .before(propagation.clone()),
            project_forced_visible_authored_nodes
                .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            project_active_simulation_pause_claims
                .after(propagation)
                .after(project_forced_visible_authored_nodes)
                .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                .run_if(pause_claim_inputs_changed),
        ),
    );
}

fn project_hidden_ancestor_sources(
    mut commands: Commands,
    changed: Query<
        (Entity, &Visibility, Has<Propagate<PauseAncestorHidden>>),
        (VisibilitySource, ChangedVisibilitySource),
    >,
    sources: Query<
        (),
        (
            With<Propagate<PauseAncestorHidden>>,
            With<Visibility>,
            VisibilitySource,
        ),
    >,
    added_sources: Query<(), With<Propagate<PauseAncestorHidden>>>,
    (mut dropped_visibility, mut dropped_children, mut dropped_claim, mut dropped_transfer): (
        RemovedComponents<Visibility>,
        RemovedComponents<Children>,
        RemovedComponents<AuthoredUiTemporarySimulationPause>,
        RemovedComponents<UiDocumentPauseTransfer>,
    ),
) {
    for (entity, visibility, has_source) in &changed {
        match (*visibility == Visibility::Hidden, has_source) {
            (true, false) => {
                commands
                    .entity(entity)
                    .insert(Propagate(PauseAncestorHidden));
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .remove::<Propagate<PauseAncestorHidden>>();
            }
            _ => {}
        }
    }
    for entity in dropped_visibility
        .read()
        .chain(dropped_children.read())
        .chain(dropped_claim.read())
        .chain(dropped_transfer.read())
    {
        if added_sources.contains(entity) && !sources.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<Propagate<PauseAncestorHidden>>();
        }
    }
}

fn project_forced_visible_authored_nodes(
    mut commands: Commands,
    changed: Query<
        (Entity, &Visibility, Has<AuthoredUiForcedVisible>),
        (
            With<UiDocumentOwner>,
            Or<(Changed<Visibility>, Added<UiDocumentOwner>)>,
        ),
    >,
    valid: Query<
        (),
        (
            With<Visibility>,
            With<UiDocumentOwner>,
            With<AuthoredUiForcedVisible>,
        ),
    >,
    marked: Query<(), With<AuthoredUiForcedVisible>>,
    mut removed_visibility: RemovedComponents<Visibility>,
    mut removed_owners: RemovedComponents<UiDocumentOwner>,
) {
    for (entity, visibility, marked) in &changed {
        match (*visibility == Visibility::Visible, marked) {
            (true, false) => {
                commands.entity(entity).insert(AuthoredUiForcedVisible);
            }
            (false, true) => {
                commands.entity(entity).remove::<AuthoredUiForcedVisible>();
            }
            _ => {}
        }
    }
    for entity in removed_visibility.read().chain(removed_owners.read()) {
        if marked.contains(entity) && !valid.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<AuthoredUiForcedVisible>();
        }
    }
}

type ChangedClaims = Or<(
    Changed<AuthoredUiTemporarySimulationPause>,
    Changed<UiDocumentPauseTransfer>,
    Added<ImmersiveSimulationPauseClaim>,
    Added<AuthoredUiForcedVisible>,
    (
        TemporaryPauseClaim,
        Or<(Changed<InheritedVisibility>, Added<PauseAncestorHidden>)>,
    ),
    (With<WorldRoot>, Changed<SimulationControl>),
)>;
type RemovedClaims<'w, 's> = (
    RemovedComponents<'w, 's, AuthoredUiTemporarySimulationPause>,
    RemovedComponents<'w, 's, UiDocumentPauseTransfer>,
    RemovedComponents<'w, 's, ImmersiveSimulationPauseClaim>,
    RemovedComponents<'w, 's, AuthoredUiForcedVisible>,
    RemovedComponents<'w, 's, PauseAncestorHidden>,
    RemovedComponents<'w, 's, InheritedVisibility>,
);

fn pause_claim_inputs_changed(
    changed: Query<(), ChangedClaims>,
    (mut claims, mut transfers, mut immersive, mut forced, mut hidden, mut inherited): RemovedClaims,
    current_claims: Query<(), TemporaryPauseClaim>,
) -> bool {
    let removed_claim = (claims.read().count() != 0)
        | (transfers.read().count() != 0)
        | (immersive.read().count() != 0)
        | (forced.read().count() != 0);
    let changed_visibility = (hidden
        .read()
        .filter(|&entity| current_claims.contains(entity))
        .count()
        != 0)
        | (inherited
            .read()
            .filter(|&entity| current_claims.contains(entity))
            .count()
            != 0);
    removed_claim || changed_visibility || !changed.is_empty()
}

type PauseClaim = (
    AnyOf<(
        &'static AuthoredUiTemporarySimulationPause,
        &'static UiDocumentPauseTransfer,
    )>,
    &'static InheritedVisibility,
    Has<PauseAncestorHidden>,
);

fn project_active_simulation_pause_claims(
    temporary_holds: Query<PauseClaim, TemporaryPauseClaim>,
    immersive_holds: Query<(), With<ImmersiveSimulationPauseClaim>>,
    authored_forced_visibility: Query<(), With<AuthoredUiForcedVisible>>,
    mut roots: Query<
        (
            &mut SimulationControl,
            &SimulationSpeedTier,
            &mut SimulationPaused,
        ),
        With<WorldRoot>,
    >,
    mut changes: MessageWriter<SimulationControlChanged>,
) {
    let Ok((mut control, tier, mut paused)) = roots.single_mut() else {
        return;
    };
    let forced_visibility = !authored_forced_visibility.is_empty();
    let temporarily_held = temporary_holds
        .iter()
        .any(|((claim, transfer), visibility, hidden)| {
            visibility.get()
                && (!forced_visibility || !hidden)
                && (claim.is_some_and(AuthoredUiTemporarySimulationPause::hold_is_requested)
                    || transfer.is_some_and(|transfer| transfer.0))
        });
    let mut next = *control;
    next.project_pause_claims(temporarily_held, !immersive_holds.is_empty());
    control.set_if_neq(next);
    if paused.set_if_neq(next.effective_pause()) {
        changes.write(SimulationControlChanged {
            speed_tier: tier.0,
            paused: paused.0,
        });
    }
}

#[cfg(test)]
mod tests;
