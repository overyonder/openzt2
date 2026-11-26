use bevy::{
    ecs::{entity::EntityHashSet, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::{
    action::simulation_time::UiSimulationAction, node_property_binding::UiTextPropertyBindingSource,
};
use openzt2_interface_state::ui::{
    authored_ui_action_projection_components::UiSimulationActions,
    authored_ui_node_projection_components::{UiDocumentMembers, UiDocumentOwner, UiDocumentRoot},
    authored_ui_selection_state::UiSelected,
};
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use super::{
    simulation_clock_types::UiZooDateText, simulation_control_types::UiSimulationPauseToggle,
};

pub(super) fn bind_authored_zoo_date_text(
    mut commands: Commands,
    bindings: Query<(Entity, &UiTextBinding, Has<UiZooDateText>), Changed<UiTextBinding>>,
    old_dates: Query<(), (With<UiZooDateText>, Without<UiTextBinding>)>,
    mut removed_bindings: RemovedComponents<UiTextBinding>,
) {
    for (entity, binding, date) in &bindings {
        match (
            matches!(binding.0, UiTextPropertyBindingSource::ZooDate),
            date,
        ) {
            (true, false) => {
                commands.entity(entity).insert_if_new(UiZooDateText);
            }
            (false, true) => {
                commands.entity(entity).remove::<UiZooDateText>();
            }
            _ => {}
        }
    }
    for entity in removed_bindings
        .read()
        .filter(|&entity| old_dates.contains(entity))
    {
        commands.entity(entity).try_remove::<UiZooDateText>();
    }
}

type ChangedPauseActions = (
    With<UiSimulationActions>,
    Or<(
        Changed<UiSimulationActions>,
        Changed<UiDocumentOwner>,
        Added<UiSelected>,
    )>,
);
type NodePauseBinding = (&'static UiSimulationActions, &'static UiDocumentOwner);

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct PauseToggleBindings<'w, 's> {
    actions: Query<'w, 's, NodePauseBinding, With<UiSelected>>,
    changed: Query<'w, 's, Entity, ChangedPauseActions>,
    marked: Query<'w, 's, (), With<UiSimulationPauseToggle>>,
    roots: Query<'w, 's, (&'static UiDocumentRoot, &'static UiDocumentMembers)>,
    changed_roots: Query<'w, 's, &'static UiDocumentMembers, Changed<UiDocumentRoot>>,
    removed_roots: RemovedComponents<'w, 's, UiDocumentRoot>,
    retained_members: Query<'w, 's, &'static UiDocumentMembers, Without<UiDocumentRoot>>,
}

pub(super) fn bind_authored_pause_toggles(
    mut commands: Commands,
    bindings: PauseToggleBindings,
    assets: Res<Assets<UiDocumentAsset>>,
    mut events: MessageReader<AssetEvent<UiDocumentAsset>>,
    (mut removed_actions, mut removed_owners, mut removed_selection): (
        RemovedComponents<UiSimulationActions>,
        RemovedComponents<UiDocumentOwner>,
        RemovedComponents<UiSelected>,
    ),
) {
    let mut bindings = bindings;
    let mut dirty: EntityHashSet = bindings
        .changed
        .iter()
        .chain(removed_actions.read())
        .chain(removed_owners.read())
        .chain(removed_selection.read())
        .collect();
    for members in &bindings.changed_roots {
        dirty.extend(members.iter());
    }
    for root in bindings.removed_roots.read() {
        if let Ok(members) = bindings.retained_members.get(root) {
            dirty.extend(members.iter());
        }
    }

    let changed_assets: bevy::platform::collections::HashSet<_> = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            AssetEvent::Unused { .. } => None,
        })
        .collect();
    if !changed_assets.is_empty() {
        for (document, members) in &bindings.roots {
            if changed_assets.contains(&document.document.id()) {
                dirty.extend(members.iter());
            }
        }
    }
    for entity in dirty {
        let is_toggle = bindings
            .actions
            .get(entity)
            .ok()
            .and_then(|(actions, owner)| {
                let (document, _) = bindings.roots.get(owner.0).ok()?;
                let document = assets.get(&document.document)?;
                Some(actions.authored_action_records(document).any(|record| {
                    matches!(
                        record.action,
                        UiSimulationAction::SetSimulationPaused { .. }
                            | UiSimulationAction::ToggleSimulationPaused
                    )
                }))
            })
            .unwrap_or(false);
        match (is_toggle, bindings.marked.contains(entity)) {
            (true, false) => {
                commands
                    .entity(entity)
                    .insert_if_new(UiSimulationPauseToggle);
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .try_remove::<UiSimulationPauseToggle>();
            }
            _ => {}
        }
    }
}
