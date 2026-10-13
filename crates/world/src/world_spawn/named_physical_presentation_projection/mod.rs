use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::world_objects::WorldObjectNamedPhysicalPresentationDefinition;
use openzt2_game_data::AssetId;

#[cfg(test)]
mod tests;

#[derive(Component)]
pub(super) struct NamedPhysicalPresentationsHydrated;

/// A named physical object of its parent entity, such as a food stand's vendor.
#[derive(Component)]
pub(super) struct NamedPhysicalPresentation;

pub(super) fn hydrate_required_named_physical_presentations(
    mut commands: Commands,
    active_definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    owners: Query<(Entity, &DefinitionId), Without<NamedPhysicalPresentationsHydrated>>,
) {
    let Some(definitions) = active_definitions.get(&assets) else {
        return;
    };
    for (owner, definition) in &owners {
        let Some(object) = definitions.find_object(definition.0) else {
            continue;
        };
        // An unregistered scene never appears later. Skip that binder with a
        // warning instead of retrying the whole object every frame.
        let presentations = object
            .named_physical_presentations
            .iter()
            .filter(|presentation| presentation.required)
            .filter_map(|presentation| {
                if presentation.prefab == AssetId::default() {
                    return Some((presentation, None));
                }
                let Some(prefab) = definitions.scene(presentation.prefab) else {
                    warn!(
                        definition = ?definition.0,
                        binder = ?presentation.name,
                        "named physical presentation has no registered scene; spawning the object without it"
                    );
                    return None;
                };
                Some((presentation, Some(prefab)))
            })
            .collect::<Vec<_>>();
        spawn_required_named_physical_presentations(&mut commands, owner, presentations);
    }
}

pub(super) fn spawn_required_named_physical_presentations<'a>(
    commands: &mut Commands,
    owner: Entity,
    presentations: impl IntoIterator<
        Item = (
            &'a WorldObjectNamedPhysicalPresentationDefinition,
            Option<Handle<ScenePrefabAsset>>,
        ),
    >,
) {
    for (presentation, prefab) in presentations {
        let mut child = commands.spawn((
            Transform::from_scale(Vec3::splat(presentation.scale)),
            if presentation.initially_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            ChildOf(owner),
            PrefabAuthoredAttachmentIdentifier(presentation.name),
            NamedPhysicalPresentation,
        ));
        if let Some(prefab) = prefab {
            child.insert(PrefabPresentation::new(prefab));
        }
    }
    commands
        .entity(owner)
        .insert(NamedPhysicalPresentationsHydrated);
}

/// A named physical object performs its owner's behaviors, as a food stand's
/// vendor hands over money and food, so its animations belong to the owner.
/// Runs after animation attachment has given the controller its direct parent.
pub(super) fn project_named_presentation_animation_owners(
    mut controllers: Query<
        (Entity, &mut AnimationPresentationOwner),
        Changed<AnimationPresentationOwner>,
    >,
    parents: Query<&ChildOf>,
    presentations: Query<&ChildOf, With<NamedPhysicalPresentation>>,
) {
    for (controller, mut owner) in &mut controllers {
        let Some(presentation_owner) = parents
            .iter_ancestors::<ChildOf>(controller)
            .find_map(|ancestor| presentations.get(ancestor).ok())
            .map(ChildOf::parent)
        else {
            continue;
        };
        if owner.gameplay_entity != presentation_owner {
            owner.gameplay_entity = presentation_owner;
        }
    }
}
