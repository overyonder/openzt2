//! State selection and queued transitions for authored child presentations.

pub(super) mod child_lifecycle;

use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectPresentationControllerDefinition, AssetId,
};
use std::collections::VecDeque;

#[derive(Component, Default)]
pub(super) struct PhysicalPresentationControllers {
    controllers: Vec<ControllerPlayback>,
}

#[derive(Default)]
struct ControllerPlayback {
    selected: Option<usize>,
    projected_child: Option<Entity>,
    queued: VecDeque<usize>,
    dirty: bool,
}

#[derive(Component)]
pub(super) struct PendingPhysicalPresentationProjection;

#[derive(Message, Clone, Copy)]
pub(crate) struct PhysicalPresentationRequest {
    pub(crate) owner: Entity,
    pub(crate) operation: PhysicalPresentationOperation,
}

#[derive(Clone, Copy)]
pub(crate) enum PhysicalPresentationOperation {
    Set(AssetId),
    Push(AssetId),
    Completed(Entity),
    Looped(Entity),
}

impl PhysicalPresentationControllers {
    pub(super) fn from_definitions(
        definitions: &[WorldObjectPresentationControllerDefinition],
    ) -> Self {
        Self {
            controllers: definitions
                .iter()
                .map(|definition| ControllerPlayback {
                    selected: definition.initial_state.and_then(|initial| {
                        definition
                            .states
                            .iter()
                            .position(|state| state.state == initial)
                    }),
                    dirty: true,
                    ..default()
                })
                .collect(),
        }
    }

    pub(super) fn apply(
        &mut self,
        definitions: &[WorldObjectPresentationControllerDefinition],
        operation: &PhysicalPresentationOperation,
        animations_enabled: bool,
    ) -> bool {
        let mut changed = false;
        for (playback, definition) in self.controllers.iter_mut().zip(definitions) {
            match *operation {
                PhysicalPresentationOperation::Set(state)
                | PhysicalPresentationOperation::Push(state) => {
                    if !animations_enabled && !definition.overrides_animation_setting {
                        continue;
                    }
                    let Some(index) = definition
                        .states
                        .iter()
                        .position(|entry| entry.state == state)
                    else {
                        continue;
                    };
                    if matches!(operation, PhysicalPresentationOperation::Push(_)) {
                        playback.queued.push_back(index);
                    } else {
                        playback.selected = Some(index);
                        playback.dirty = true;
                    }
                    changed = true;
                }
                PhysicalPresentationOperation::Completed(child)
                | PhysicalPresentationOperation::Looped(child) => {
                    // Ignore late notifications from a replaced child presentation.
                    if playback.dirty || playback.projected_child != Some(child) {
                        continue;
                    }
                    let next = playback.queued.pop_front();
                    if next.is_some()
                        || matches!(operation, PhysicalPresentationOperation::Completed(_))
                    {
                        playback.selected = next.or_else(|| {
                            definition.default_state.and_then(|default_state| {
                                definition
                                    .states
                                    .iter()
                                    .position(|entry| entry.state == default_state)
                            })
                        });
                        playback.dirty = true;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    pub(super) fn pending(
        &self,
    ) -> impl Iterator<Item = (usize, Option<usize>, Option<Entity>)> + '_ {
        self.controllers
            .iter()
            .enumerate()
            .filter(|(_, state)| state.dirty)
            .map(|(index, state)| (index, state.selected, state.projected_child))
    }

    pub(super) fn projected(&mut self, controller: usize, child: Option<Entity>) {
        let playback = &mut self.controllers[controller];
        playback.projected_child = child;
        playback.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::world_definitions::world_objects::WorldObjectPresentationAttachmentDefinition;

    fn sign_controller() -> WorldObjectPresentationControllerDefinition {
        WorldObjectPresentationControllerDefinition {
            initial_state: Some(AssetId::from_key("idle")),
            default_state: Some(AssetId::from_key("idle")),
            overrides_animation_setting: false,
            states: ["idle", "used", "extra"]
                .map(|name| WorldObjectPresentationAttachmentDefinition {
                    state: AssetId::from_key(name),
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
        }
    }

    #[test]
    fn set_is_immediate_push_is_fifo_and_completion_returns_to_default() {
        let definitions = [sign_controller()];
        let mut playback = PhysicalPresentationControllers::from_definitions(&definitions);
        let mut world = World::new();
        let initial = world.spawn_empty().id();
        playback.projected(0, Some(initial));
        assert!(playback.apply(
            &definitions,
            &PhysicalPresentationOperation::Push(AssetId::from_key("extra")),
            true
        ));
        assert_eq!(playback.pending().count(), 0);
        assert!(playback.apply(
            &definitions,
            &PhysicalPresentationOperation::Set(AssetId::from_key("used")),
            true
        ));
        assert_eq!(playback.pending().next().expect("switch").1, Some(1));
        assert!(!playback.apply(
            &definitions,
            &PhysicalPresentationOperation::Completed(initial),
            true
        ));
        let used = world.spawn_empty().id();
        playback.projected(0, Some(used));
        assert!(playback.apply(
            &definitions,
            &PhysicalPresentationOperation::Looped(used),
            true
        ));
        assert_eq!(playback.pending().next().expect("queue").1, Some(2));
        let extra = world.spawn_empty().id();
        playback.projected(0, Some(extra));
        assert!(!playback.apply(
            &definitions,
            &PhysicalPresentationOperation::Looped(extra),
            true
        ));
        assert!(playback.apply(
            &definitions,
            &PhysicalPresentationOperation::Completed(extra),
            true
        ));
        assert_eq!(playback.pending().next().expect("default").1, Some(0));
    }
}

pub(super) fn apply_physical_presentation_requests(
    mut commands: Commands,
    mut requests: MessageReader<PhysicalPresentationRequest>,
    mut pending: Local<Vec<PhysicalPresentationRequest>>,
    definitions: Res<crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>,
    assets: Res<Assets<crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>,
    settings: Option<Res<crate::plugins::settings::graphics_settings_types::GraphicsSettings>>,
    mut owners: Query<(&super::prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection, &mut PhysicalPresentationControllers)>,
    presentation_owners: Query<(), With<super::prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection>>,
) {
    pending.extend(requests.read().copied());
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    pending.retain(|request| {
        let Ok((identity, mut controllers)) = owners.get_mut(request.owner) else {
            return presentation_owners.contains(request.owner);
        };
        let Some(definition) = definitions.find_object(identity.0) else {
            return true;
        };
        if controllers.apply(
            &definition.presentation_attachments,
            &request.operation,
            settings
                .as_ref()
                .is_none_or(|settings| settings.physical_animations),
        ) {
            commands
                .entity(request.owner)
                .insert(PendingPhysicalPresentationProjection);
        }
        false
    });
}
