use super::{
    prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
    prefab_presentation_types::PrefabPresentation, world_membership_types::DefinitionId,
};
use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectNamedPhysicalPresentationDefinition, AssetId,
};

#[derive(Component)]
pub(super) struct NamedPhysicalPresentationsHydrated;

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
        let Some(presentations) = object
            .named_physical_presentations
            .iter()
            .filter(|presentation| presentation.required)
            .map(|presentation| {
                let prefab = if presentation.prefab == AssetId::default() {
                    None
                } else {
                    Some(definitions.scene(presentation.prefab)?)
                };
                Some((presentation, prefab))
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
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
            Visibility::Inherited,
            ChildOf(owner),
            PrefabAuthoredAttachmentIdentifier(presentation.name),
        ));
        if let Some(prefab) = prefab {
            child.insert(PrefabPresentation::new(prefab));
        }
    }
    commands
        .entity(owner)
        .insert(NamedPhysicalPresentationsHydrated);
}
