pub(super) mod child_lifecycle;

use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{world_definitions::world_objects::WorldObjectPresentationControllerDefinition, AssetId};
use openzt2_world_state::world_spawn::physical_presentation_state::{
    PhysicalPresentationOperation, PhysicalPresentationRequest,
};
use std::collections::VecDeque;

#[derive(Component)]
#[relationship(relationship_target = ObjectPhysicalPresentationControllers)]
pub(super) struct ObjectPhysicalPresentationControllerOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = ObjectPhysicalPresentationControllerOf, linked_spawn)]
pub(super) struct ObjectPhysicalPresentationControllers(Vec<Entity>);

#[derive(Component)]
pub(super) struct PhysicalPresentationControllerDefinition {
    pub(super) object: AssetId,
    pub(super) controller_index: usize,
}

#[derive(Component)]
pub(super) struct SelectedPhysicalPresentationState(pub(super) Option<usize>);

#[derive(Component, Default)]
pub(super) struct QueuedPhysicalPresentationStates(VecDeque<usize>);

#[derive(Component)]
pub(super) struct PendingPhysicalPresentationChild;

#[derive(Component)]
#[relationship(relationship_target = ActivePhysicalPresentationChild)]
pub(super) struct PhysicalPresentationChildOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = PhysicalPresentationChildOf, linked_spawn)]
pub(super) struct ActivePhysicalPresentationChild(pub(super) Entity);

pub(super) fn apply_physical_presentation_requests(
    mut commands: Commands,
    mut requests: MessageReader<PhysicalPresentationRequest>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    settings: Option<Res<GraphicsSettings>>,
    owners: Query<&ObjectPhysicalPresentationControllers>,
    mut controllers: Query<(
        &PhysicalPresentationControllerDefinition,
        &ObjectPhysicalPresentationControllerOf,
        &mut SelectedPhysicalPresentationState,
        &mut QueuedPhysicalPresentationStates,
        Option<&ActivePhysicalPresentationChild>,
        Has<PendingPhysicalPresentationChild>,
    )>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let animations_enabled = settings
        .as_ref()
        .is_none_or(|settings| settings.physical_animations);
    for request in requests.read() {
        let targets = match request.operation {
            PhysicalPresentationOperation::ChildPeriodFinished { ref controller, .. } => {
                std::slice::from_ref(controller)
            }
            _ => match owners.get(request.owner) {
                Ok(controllers) => &controllers.0,
                Err(_) => continue,
            },
        };
        for &target in targets {
            let Ok((identity, owner, mut selected, mut queued, active, pending)) =
                controllers.get_mut(target)
            else {
                continue;
            };
            if owner.0 != request.owner {
                continue;
            }
            let Some(definition) = definitions.find_object(identity.object).and_then(|object| {
                object
                    .presentation_attachments
                    .get(identity.controller_index)
            }) else {
                continue;
            };
            let previous_child = if pending || selected.is_changed() {
                None
            } else {
                active.map(|active| active.0)
            };
            if let Some(next) = apply_controller_operation(
                definition,
                &mut queued.0,
                previous_child,
                request.operation,
                animations_enabled,
            ) {
                selected.0 = next;
                if !pending {
                    commands
                        .entity(target)
                        .insert(PendingPhysicalPresentationChild);
                }
            }
        }
    }
}

fn apply_controller_operation(
    definition: &WorldObjectPresentationControllerDefinition,
    queued: &mut VecDeque<usize>,
    active_child: Option<Entity>,
    operation: PhysicalPresentationOperation,
    animations_enabled: bool,
) -> Option<Option<usize>> {
    match operation {
        PhysicalPresentationOperation::Set(state) | PhysicalPresentationOperation::Push(state) => {
            if !animations_enabled && !definition.overrides_animation_setting {
                return None;
            }
            let index = physical_presentation_state_index(definition, state)?;
            if matches!(operation, PhysicalPresentationOperation::Push(_)) {
                queued.push_back(index);
                None
            } else {
                Some(Some(index))
            }
        }
        PhysicalPresentationOperation::ChildPeriodFinished { child, looping, .. } => {
            if active_child != Some(child) {
                return None;
            }
            let next = queued.pop_front();
            if next.is_none() && looping {
                return None;
            }
            Some(next.or_else(|| {
                definition
                    .default_state
                    .and_then(|state| physical_presentation_state_index(definition, state))
            }))
        }
    }
}

pub(super) fn physical_presentation_state_index(
    controller: &WorldObjectPresentationControllerDefinition,
    state: AssetId,
) -> Option<usize> {
    controller
        .states
        .iter()
        .position(|entry| entry.state == state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::world_definitions::world_objects::WorldObjectPresentationAttachmentDefinition;

    #[test]
    fn set_is_immediate_push_is_fifo_and_completion_returns_to_default() {
        let definition = WorldObjectPresentationControllerDefinition {
            initial_state: Some(AssetId::from_key("idle")),
            default_state: Some(AssetId::from_key("idle")),
            overrides_animation_setting: false,
            states: ["idle", "used", "extra"]
                .map(|name| WorldObjectPresentationAttachmentDefinition {
                    state: AssetId::from_key(name),
                    child_binder: AssetId::from_key(name),
                    parent_attachment: AssetId::from_key("link_sign"),
                    inherit_parent_rotation: true,
                    prefab: AssetId::from_key(name),
                    child_attachment: None,
                    minimum_period_seconds: -1.0,
                    maximum_period_seconds: -1.0,
                    event_trigger: None,
                    child_animation: None,
                })
                .into(),
        };
        let mut world = World::new();
        let controller = world.spawn_empty().id();
        let initial = world.spawn_empty().id();
        let used = world.spawn_empty().id();
        let extra = world.spawn_empty().id();
        let mut queued = VecDeque::new();
        let mut operate = |child, operation| {
            apply_controller_operation(&definition, &mut queued, child, operation, true)
        };
        let finish = |child, looping| PhysicalPresentationOperation::ChildPeriodFinished {
            controller,
            child,
            looping,
        };
        assert_eq!(
            operate(
                Some(initial),
                PhysicalPresentationOperation::Push(AssetId::from_key("extra"))
            ),
            None
        );
        assert_eq!(
            operate(
                Some(initial),
                PhysicalPresentationOperation::Set(AssetId::from_key("used"))
            ),
            Some(Some(1))
        );
        assert_eq!(operate(Some(used), finish(initial, false)), None);
        assert_eq!(operate(Some(used), finish(used, true)), Some(Some(2)));
        assert_eq!(operate(Some(extra), finish(extra, true)), None);
        assert_eq!(operate(Some(extra), finish(extra, false)), Some(Some(0)));
    }
}
