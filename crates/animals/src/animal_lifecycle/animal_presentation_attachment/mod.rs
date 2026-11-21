use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    ecs::relationship::RelationshipTarget,
    platform::collections::HashSet,
    prelude::*,
};

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::{
    ModelAnimationAssetLibrary, ModelAnimationAttachmentResolved,
};
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_playback_controller_types::{
    AnimationPlaybackController, RetainedAnimationClipSource,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::types::{Animal, AnimalVariant, SelectedAnimalPresentationModel};

use super::types::AnimalPresentationOwner;

#[derive(Component)]
pub(super) struct AnimalPresentationProjection {
    model: AssetId,
    animation_set: Handle<AnimationSetAsset>,
    initial_clip: Option<String>,
    initial_clip_loops: bool,
}

/// Retains the requested prefab until the dependency is loaded or fails.
#[derive(Component)]
pub(super) struct AnimalPresentationPrefabDemand(Handle<ScenePrefabAsset>);

/// Use the authored prefab projection so binder transforms and materials
/// remain attached to the animal's presentation.
pub(super) fn attach_selected_species_variant_models_to_animal_entities(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    asset_server: Res<AssetServer>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    mut prefab_failures: MessageReader<AssetLoadFailedEvent<ScenePrefabAsset>>,
    mut definition_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut species_events: MessageReader<AssetEvent<SpeciesAsset>>,
    animals: Query<
        (
            Entity,
            &SelectedAnimalPresentationModel,
            &AnimalVariant,
            &DefinitionId,
            Option<&AnimalPresentationPrefabDemand>,
            Option<&Children>,
        ),
        With<Animal>,
    >,
    changed_animals: Query<
        Entity,
        (
            With<Animal>,
            Or<(
                Changed<SelectedAnimalPresentationModel>,
                Changed<AnimalVariant>,
                Changed<DefinitionId>,
            )>,
        ),
    >,
    presentation: Query<(
        Entity,
        &PrefabSourceAssetHandle,
        &AnimalPresentationProjection,
    )>,
) {
    let changed_prefabs: HashSet<_> = prefab_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            AssetEvent::LoadedWithDependencies { .. } | AssetEvent::Unused { .. } => None,
        })
        .chain(prefab_failures.read().map(|failure| failure.id))
        .collect();
    let definitions_changed = definition_events.read().any(|event| {
        matches!(
            event,
            AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
        )
    });
    let species_changed = species_events.read().any(|event| {
        matches!(
            event,
            AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
        )
    });
    definition_events.clear();
    species_events.clear();
    let all = species_index.is_changed()
        || active_definitions.is_changed()
        || definitions_changed
        || species_changed;
    let (Some(species), Some(catalogue)) = (
        species_index.get(&species_assets),
        active_definitions.get(&definitions),
    ) else {
        return;
    };
    let mut affected: Vec<_> = if all {
        animals.iter().map(|(animal, ..)| animal).collect()
    } else {
        changed_animals.iter().collect()
    };
    if !all && !changed_prefabs.is_empty() {
        affected.extend(
            animals
                .iter()
                .filter_map(|(animal, _, _, _, demand, children)| {
                    let pending_changed =
                        demand.is_some_and(|demand| changed_prefabs.contains(&demand.0.id()));
                    let projection_changed = presentation
                        .iter_many(children.into_iter().flat_map(RelationshipTarget::iter))
                        .flatten()
                        .any(|(_, source, _)| changed_prefabs.contains(&source.0.id()));
                    (pending_changed || projection_changed).then_some(animal)
                }),
        );
        affected.sort_unstable();
        affected.dedup();
    }
    for animal in affected {
        let Ok((_, appearance, selected_variant, definition, demand, children)) =
            animals.get(animal)
        else {
            continue;
        };
        let current = presentation
            .iter_many(children.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
            .next();
        let binding = species
            .find_variant(selected_variant.0)
            .and_then(|variant| {
                let object = catalogue
                    .find_object(variant.id)
                    .or_else(|| catalogue.find_object(definition.0))?;
                let prefab = catalogue.scene(object.prefab)?;
                let animation = species.load_variant_animation_set(&asset_server, variant)?;
                Some((variant, prefab, animation))
            });
        let Some((variant, prefab_handle, animation_set)) = binding else {
            if !active_definitions.is_complete() {
                continue;
            }
            if let Some((root, ..)) = current {
                commands.entity(root).try_despawn();
            }
            if demand.is_some() {
                commands
                    .entity(animal)
                    .remove::<AnimalPresentationPrefabDemand>();
            }
            continue;
        };
        let same_animation = current.is_some_and(|(_, _, projection)| {
            projection.model == appearance.model
                && projection.animation_set == animation_set
                && projection.initial_clip == variant.initial_animation_clip_asset_key
                && projection.initial_clip_loops == variant.initial_animation_loops
        });
        if same_animation
            && current.is_some_and(|(_, source, _)| {
                source.0 == prefab_handle && !changed_prefabs.contains(&source.0.id())
            })
        {
            continue;
        }
        let Some(prefab) = prefabs.get(&prefab_handle) else {
            if matches!(
                asset_server.load_state(prefab_handle.id()),
                LoadState::Failed(_)
            ) {
                if let Some((root, ..)) = current {
                    commands.entity(root).try_despawn();
                }
                if demand.is_some() {
                    commands
                        .entity(animal)
                        .remove::<AnimalPresentationPrefabDemand>();
                }
            } else if demand.is_none_or(|demand| demand.0 != prefab_handle) {
                commands
                    .entity(animal)
                    .insert(AnimalPresentationPrefabDemand(prefab_handle));
            }
            continue;
        };
        if let Some((root, ..)) = current {
            commands.entity(root).try_despawn();
        }
        let (root, renderables, _) = spawn_prefab_render_tree(&mut commands, prefab, animal, false);
        commands.entity(root).insert((
            PrefabSourceAssetHandle(prefab_handle),
            AnimalPresentationProjection {
                model: appearance.model,
                animation_set: animation_set.clone(),
                initial_clip: variant.initial_animation_clip_asset_key.clone(),
                initial_clip_loops: variant.initial_animation_loops,
            },
        ));
        for (entity, authored) in renderables.into_iter().zip(
            prefab
                .canonical_scene_prefab_document()
                .entities
                .iter()
                .flat_map(|row| &row.renderables),
        ) {
            if authored.model.0 == appearance.model.0 {
                commands
                    .entity(entity)
                    .insert(AnimalPresentationOwner(animal))
                    .remove::<PendingModelAnimationAssets>();
            }
        }
        if !same_animation {
            // All skins sample the one playback clock on the gameplay entity.
            commands
                .entity(animal)
                .remove::<(
                    ModelAnimationAttachmentResolved,
                    ModelAnimationAssetLibrary,
                    AnimationPlaybackController,
                    AnimationGraphPlaybackState,
                    AnimationPlayer,
                    RetainedAnimationClipSource,
                )>()
                .insert((
                    AnimationPresentationOwner {
                        gameplay_entity: animal,
                    },
                    PendingModelAnimationAssets {
                        pending_model_animation_set_asset_handles: vec![animation_set]
                            .into_boxed_slice(),
                        requested_initial_animation_clip_asset_key: variant
                            .initial_animation_clip_asset_key
                            .clone(),
                    },
                ));
        }
        if demand.is_some() {
            commands
                .entity(animal)
                .remove::<AnimalPresentationPrefabDemand>();
        }
    }
}

pub(super) fn apply_authored_initial_animal_animation_repetition_policy_after_attachment(
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    newly_resolved_animation_controllers: Query<
        (Entity, &AnimationPlaybackController),
        Added<ModelAnimationAttachmentResolved>,
    >,
    animals: Query<&AnimalVariant, With<Animal>>,
    mut animation_clip_playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    let Some(species_index) = species_index.get(&species_assets) else {
        return;
    };
    for (animation_controller_entity, _) in &newly_resolved_animation_controllers {
        let Ok(selected_variant) = animals.get(animation_controller_entity) else {
            continue;
        };
        let Some(variant) = species_index.find_variant(selected_variant.0) else {
            continue;
        };
        let Some(animation_clip_asset_key) = variant.initial_animation_clip_asset_key.as_ref()
        else {
            continue;
        };
        animation_clip_playback_requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: animation_controller_entity,
            animation_clip_asset_key: animation_clip_asset_key.clone(),
            blend_duration_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_repetition_policy: if variant.initial_animation_loops {
                AnimationPlaybackRepetitionPolicy::Loop
            } else {
                AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy
            },
        });
    }
}
