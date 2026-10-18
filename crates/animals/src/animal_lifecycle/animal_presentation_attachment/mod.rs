use bevy::prelude::*;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::types::{Animal, AnimalVariant, SelectedAnimalPresentationModel};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AnimalPresentationAttached;

use super::types::AnimalPresentationOwner;

/// The animal's presentation prefab while it loads.
#[derive(Component)]
pub(super) struct AnimalPresentationPrefabDemand(Handle<ScenePrefabAsset>);

/// Reuse the authored prefab projection used by placement previews. A raw model
/// omits binder transforms and material overrides, including the fur effect.
pub(super) fn attach_selected_species_variant_models_to_animal_entities(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    asset_server: Res<AssetServer>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    animals: Query<
        (
            Entity,
            &SelectedAnimalPresentationModel,
            &AnimalVariant,
            &DefinitionId,
            Option<&AnimalPresentationPrefabDemand>,
        ),
        (With<Animal>, Without<AnimalPresentationAttached>),
    >,
) {
    let Some(species_index) = species_index.get(&species_assets) else {
        return;
    };
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (animal_entity, appearance, selected_variant, definition, demand) in &animals {
        let Some(variant) = species_index.find_variant(selected_variant.0) else {
            continue;
        };
        let Some(object) = catalogue
            .find_object(variant.id)
            .or_else(|| catalogue.find_object(definition.0))
        else {
            continue;
        };
        let Some(prefab_handle) = demand
            .map(|demand| demand.0.clone())
            .or_else(|| catalogue.scene(object.prefab))
        else {
            continue;
        };
        let Some(prefab) = prefabs.get(&prefab_handle) else {
            // Definitions return a fresh handle per lookup. Dropping it while
            // the prefab loads releases the load, so keep it until it lands.
            if demand.is_none() {
                commands
                    .entity(animal_entity)
                    .insert(AnimalPresentationPrefabDemand(prefab_handle));
            }
            continue;
        };
        let Some(animation_set_handle) =
            species_index.load_variant_animation_set(&asset_server, variant)
        else {
            continue;
        };
        let (root, renderables, _) =
            spawn_prefab_render_tree(&mut commands, prefab, animal_entity, false);
        commands
            .entity(root)
            .insert(PrefabSourceAssetHandle(prefab_handle));
        for (entity, authored) in renderables.into_iter().zip(
            prefab
                .canonical_scene_prefab_document()
                .entities
                .iter()
                .flat_map(|row| &row.renderables),
        ) {
            if authored.model.0 != appearance.model.0 {
                continue;
            }
            commands
                .entity(entity)
                .insert(AnimalPresentationOwner(animal_entity))
                .remove::<PendingModelAnimationAssets>();
        }
        // Every skin samples the animal's one playback clock, including skins
        // whose scene instances finish loading after playback has started.
        commands.entity(animal_entity).insert((
            AnimationPresentationOwner {
                gameplay_entity: animal_entity,
            },
            PendingModelAnimationAssets {
                pending_model_animation_set_asset_handles: vec![animation_set_handle.clone()]
                    .into_boxed_slice(),
                requested_initial_animation_clip_asset_key: variant
                    .initial_animation_clip_asset_key
                    .clone(),
            },
        ));
        commands
            .entity(animal_entity)
            .insert(AnimalPresentationAttached)
            .remove::<AnimalPresentationPrefabDemand>();
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
