//! Creates, hydrates, synchronizes, and animates object-placement previews.

use bevy::prelude::*;
use openzt2_game_data::species::LifeStage;
use openzt2_game_data::species::SpeciesVariant;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::species::species_asset_types::SpeciesView;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAssetLibrary;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_zoo_state::habitat::habitat_types::HabitatLocatable;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;
use openzt2_world_state::world_spawn::prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection;
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::snap_object_placement_transform_to_authored_grid;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPrefabSource;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewModelPresentation;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewMotion;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPermissionFacts;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPrefabHydrated;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewRenderable;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewRequest;
use openzt2_zoo_state::placement::placement_preview_types::PlacementRotationRequest;
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectRelocationSource;

pub(super) fn create_active_object_placement_preview(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    cursors: Query<(&ConstructionCursor, Option<&ObjectPlacementPreviewRequest>)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut previews: Query<(
        Entity,
        &mut ConstructionPreview,
        Option<&PlacedObjectRelocationSource>,
    )>,
    roots: Query<Entity, With<WorldRoot>>,
    relocating: Query<
        (
            Entity,
            &PlacedObjectDefinitionReference,
            &Transform,
            Option<&PlacementRotationRequest>,
        ),
        With<RelocatingPlacedObject>,
    >,
) {
    let (Ok((cursor, requested)), Ok(root)) = (cursors.single(), roots.single()) else {
        return;
    };
    let definition = match *tool {
        ConstructionTool::Place(definition) => definition,
        ConstructionTool::Placement => {
            let Some(requested) = requested else { return };
            requested.0
        }
        _ => return,
    };
    let relocation = relocating
        .iter()
        .find(|(_, placeable, _, _)| placeable.definition == definition);
    let relocation_source = relocation.map(|(entity, ..)| entity);
    if let Some((_, mut preview, _)) = previews.iter_mut().find(|(_, preview, source)| {
        preview.definition == definition && source.map(|source| source.0) == relocation_source
    }) {
        if let Some((source, _, _, Some(rotation))) = relocation {
            if let Some(placeable) = active_definitions
                .get(&definitions)
                .and_then(|catalogue| resolve_object_placeable_definition(catalogue, definition))
            {
                super::object_placement_preview_rotation::rotate_object_placement_transform_by_authored_steps(
                    &mut preview.transform, placeable, i32::from(rotation.0),
                );
                commands.entity(source).remove::<PlacementRotationRequest>();
            }
        }
        return;
    }
    let mut retired_stale_preview = false;
    for (entity, ..) in &previews {
        commands.entity(entity).despawn();
        retired_stale_preview = true;
    }
    if retired_stale_preview {
        return;
    }
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let placeable = resolve_object_placeable_definition(catalogue, definition);
    if matches!(*tool, ConstructionTool::Place(_)) && placeable.is_none() {
        return;
    }
    let Some(object) = catalogue.find_object(definition) else {
        return;
    };
    let preview_scene = openzt2_zoo_state::placement::object_placement_definition_queries::object_placement_prefab_identifier(object);
    let Some(prefab) = preview_scene.and_then(|scene| catalogue.scene(scene)) else {
        return;
    };
    let mut transform =
        Transform::from_translation(cursor.world + Vec3::Y * object.terrain_fit_height_offset_m);
    if let Some((_, _, source, rotation)) = relocation {
        transform.rotation = source.rotation;
        if let (Some(rotation), Some(placeable)) = (rotation, placeable) {
            super::object_placement_preview_rotation::rotate_object_placement_transform_by_authored_steps(
                &mut transform,
                placeable,
                i32::from(rotation.0),
            );
        }
    }
    let Some(transform) = placeable
        .map(|placeable| snap_object_placement_transform_to_authored_grid(placeable, transform))
        .unwrap_or(Some(transform))
    else {
        return;
    };
    let mut preview = commands.spawn((
        ConstructionPreview {
            definition,
            transform,
            validity: PlacementValidity::Pending,
        },
        transform,
        ObjectPlacementPrefabSource(prefab),
        ObjectPlacementPreviewMotion {
            current: transform.translation,
            start: transform.translation,
            target: transform.translation,
            progress: 1.0,
            progress_per_second: ObjectPlacementPreviewMotion::progress_per_second(
                placeable.map_or(0.0, |placeable| placeable.weight),
            ),
        },
        HabitatLocatable,
        if cursor.over_terrain {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
        WorldMember { root },
    ));
    if placeable.is_some() {
        preview.insert(ObjectPlacementPreviewPermissionFacts::default());
    }
    let preview = preview.id();
    commands.spawn((
        ObjectPlacementPreviewModelPresentation,
        PrefabObjectPresentationAttachmentProjection(definition),
        Transform::IDENTITY,
        Visibility::Inherited,
        ChildOf(preview),
    ));
    if let Some((source, _, _, rotation)) = relocation {
        commands
            .entity(preview)
            .insert(PlacedObjectRelocationSource(source));
        if rotation.is_some() {
            commands.entity(source).remove::<PlacementRotationRequest>();
        }
    }
}

#[derive(Component)]
pub(super) struct PendingAnimalPreviewAnimation;

pub(super) fn hydrate_object_placement_preview_prefab_render_tree(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    previews: Query<
        (Entity, &ObjectPlacementPrefabSource, &Children),
        (
            With<ConstructionPreview>,
            Without<ObjectPlacementPreviewPrefabHydrated>,
        ),
    >,
    presentations: Query<(), With<ObjectPlacementPreviewModelPresentation>>,
) {
    for (entity, source, children) in &previews {
        let Some(prefab) = prefabs.get(&source.0) else {
            continue;
        };
        let Some(presentation) = children.iter().find(|child| presentations.contains(*child))
        else {
            continue;
        };
        let (_, renderables, _) =
            spawn_prefab_render_tree(&mut commands, prefab, presentation, false);
        for renderable in renderables {
            commands.entity(renderable).insert((
                ObjectPlacementPreviewRenderable,
                PrefabModelTint(Color::WHITE),
                PendingAnimalPreviewAnimation,
            ));
        }
        commands
            .entity(entity)
            .insert(ObjectPlacementPreviewPrefabHydrated);
    }
}

pub(super) fn attach_pending_animal_placement_preview_animations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    species_assets: Option<Res<Assets<SpeciesAsset>>>,
    species_index: Option<Res<SpeciesAssets>>,
    parents: Query<&ChildOf>,
    presentations: Query<&ChildOf, With<ObjectPlacementPreviewModelPresentation>>,
    previews: Query<&ConstructionPreview>,
    mut renderables: Query<(
        Entity,
        &PrefabModel,
        Ref<PendingAnimalPreviewAnimation>,
        Option<&mut PendingModelAnimationAssets>,
    )>,
) {
    let (Some(species_index), Some(species_assets)) = (species_index, species_assets) else {
        return;
    };
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    let catalogue_changed = species_index.is_changed() || species_assets.is_changed();
    for (entity, model, pending, attachment) in &mut renderables {
        if !pending.is_added() && !catalogue_changed {
            continue;
        }
        let Some((presentation, owner)) =
            parents
                .iter_ancestors::<ChildOf>(entity)
                .find_map(|ancestor| {
                    presentations
                        .get(ancestor)
                        .ok()
                        .map(|parent| (ancestor, parent.parent()))
                })
        else {
            continue;
        };
        let Ok(preview) = previews.get(owner) else {
            continue;
        };
        let Some(variant) =
            species_variant_for_placement_preview_model(species, preview.definition, model.model)
        else {
            continue;
        };
        let Some(animation) = species.load_variant_animation_set(&asset_server, variant) else {
            continue;
        };
        let assets = PendingModelAnimationAssets {
            pending_model_animation_set_asset_handles: vec![animation].into_boxed_slice(),
            requested_initial_animation_clip_asset_key: variant
                .initial_animation_clip_asset_key
                .clone(),
        };
        if let Some(mut attachment) = attachment {
            *attachment = assets;
        } else {
            commands.entity(entity).insert(assets);
        }
        commands
            .entity(presentation)
            .insert_if_new(AnimationPresentationOwner {
                gameplay_entity: owner,
            });
        commands.entity(entity).remove::<(
            PendingAnimalPreviewAnimation,
            ModelAnimationAttachmentResolved,
            AnimationPlaybackController,
            AnimationGraphPlaybackState,
            ModelAnimationAssetLibrary,
        )>();
    }
}

fn species_variant_for_placement_preview_model<'a>(
    species: SpeciesView<'a>,
    definition: AssetId,
    model: AssetId,
) -> Option<&'a SpeciesVariant> {
    if let Some(variant) = species.find_variant(definition) {
        return (AssetId::from_virtual_path(&variant.model) == model).then_some(variant);
    }
    let root = species.find(definition)?;
    // A species-root preview already has its authored model. Borrow its
    // matching adult's pose without selecting the eventual adopted variant.
    species.variants(root.id).find(|variant| {
        variant.life_stage == LifeStage::Adult
            && variant.probability != 0
            && AssetId::from_virtual_path(&variant.model) == model
    })
}

pub(super) fn apply_authored_initial_animal_placement_preview_animation_repetition_policy(
    species_assets: Option<Res<Assets<SpeciesAsset>>>,
    species_index: Option<Res<SpeciesAssets>>,
    controllers: Query<
        (Entity, &PrefabModel, &AnimationPlaybackController),
        (
            Added<ModelAnimationAttachmentResolved>,
            With<ObjectPlacementPreviewRenderable>,
        ),
    >,
    previews: Query<&ConstructionPreview>,
    parents: Query<&ChildOf>,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    let Some(species) = species_index
        .as_ref()
        .zip(species_assets.as_ref())
        .and_then(|(index, assets)| index.get(assets))
    else {
        return;
    };
    for (entity, model, _) in &controllers {
        let Some(preview) = parents
            .iter_ancestors::<ChildOf>(entity)
            .find_map(|ancestor| previews.get(ancestor).ok())
        else {
            continue;
        };
        let Some(variant) =
            species_variant_for_placement_preview_model(species, preview.definition, model.model)
        else {
            continue;
        };
        let Some(clip) = &variant.initial_animation_clip_asset_key else {
            continue;
        };
        requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: entity,
            animation_clip_asset_key: clip.clone(),
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

pub(super) fn synchronize_object_placement_preview_with_construction_cursor(
    cursors: Query<Ref<ConstructionCursor>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    mut previews: Query<(
        &mut ConstructionPreview,
        &mut Transform,
        &mut ObjectPlacementPreviewMotion,
        &mut Visibility,
        Option<&ObjectPlacementPrefabSource>,
    )>,
) {
    let Ok(cursor) = cursors.single() else { return };
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (mut preview, mut transform, mut motion, mut visibility, prefab) in &mut previews {
        let was_hidden = *visibility == Visibility::Hidden;
        let next_visibility = if cursor.over_terrain {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != next_visibility {
            *visibility = next_visibility;
        }
        if !cursor.over_terrain {
            let invalid = PlacementValidity::Invalid(PlacementFailure::OutsideMap);
            if preview.validity != invalid {
                preview.validity = invalid;
            }
            continue;
        }
        let mut requested_transform = preview.transform;
        if cursor.is_changed() || preview.is_added() {
            requested_transform.translation = cursor.world
                + Vec3::Y
                    * catalogue
                        .find_object(preview.definition)
                        .map_or(0.0, |object| object.terrain_fit_height_offset_m);
        }
        let snapped = if let Some(definition) =
            resolve_object_placeable_definition(catalogue, preview.definition)
        {
            let Some(snapped) = snap_object_placement_transform_to_authored_grid(
                footprints.resolve(definition, prefab.map(|source| &source.0)),
                requested_transform,
            ) else {
                continue;
            };
            snapped
        } else {
            requested_transform
        };
        if preview.transform != snapped {
            preview.transform = snapped;
        }
        if was_hidden {
            // A catalogue selection creates its preview while the pointer is
            // captured by authored UI, so the cursor's retained world point
            // is not a visible motion origin. The source first presents the
            // model at the terrain point acquired after leaving the UI, then
            // applies its weighted interpolation to later visible movement.
            motion.current = snapped.translation;
            motion.start = snapped.translation;
            motion.target = snapped.translation;
            motion.progress = 1.0;
        } else if motion.target != snapped.translation {
            motion.start = motion.current;
            motion.target = snapped.translation;
            motion.progress = 0.0;
        }
        if *transform != preview.transform {
            *transform = preview.transform;
        }
    }
}

pub(super) fn animate_object_placement_preview_presentation_toward_authoritative_transform(
    time: Res<Time>,
    mut previews: Query<
        (&Children, &Transform, &mut ObjectPlacementPreviewMotion),
        Without<ObjectPlacementPreviewModelPresentation>,
    >,
    mut presentations: Query<
        &mut Transform,
        (
            With<ObjectPlacementPreviewModelPresentation>,
            Without<ConstructionPreview>,
        ),
    >,
) {
    for (children, authoritative, mut motion) in &mut previews {
        motion.advance(time.delta_secs());
        let offset = authoritative
            .rotation
            .inverse()
            .mul_vec3(motion.current - authoritative.translation);
        for child in children.iter() {
            let Ok(mut presentation) = presentations.get_mut(child) else {
                continue;
            };
            if presentation.translation != offset {
                presentation.translation = offset;
            }
        }
    }
}
