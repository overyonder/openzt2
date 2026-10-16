//! Creates, hydrates, synchronizes, and animates object-placement previews.

use bevy::prelude::*;
use openzt2_game_data::species::LifeStage;
use openzt2_game_data::species::SpeciesVariant;
use openzt2_game_data::AssetId;

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
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
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
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewOwner;
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
    previews: Query<(Entity, &ConstructionPreview)>,
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
    if previews
        .iter()
        .any(|(_, preview)| preview.definition == definition)
    {
        return;
    }
    let mut retired_stale_preview = false;
    for (entity, _) in &previews {
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
    let relocation = relocating
        .iter()
        .find(|(_, placeable, _, _)| placeable.definition == definition);
    let mut transform = Transform::from_translation(cursor.world);
    if let Some((_, _, source, rotation)) = relocation {
        transform.rotation = source.rotation;
        if let Some(rotation) = rotation {
            transform.rotate_y(f32::from(rotation.0) * core::f32::consts::FRAC_PI_4);
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
    if let Some((source, ..)) = relocation {
        commands
            .entity(preview)
            .insert(PlacedObjectRelocationSource(source));
    }
}

pub(super) fn hydrate_object_placement_preview_prefab_render_tree(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    asset_server: Res<AssetServer>,
    species_assets: Option<Res<Assets<SpeciesAsset>>>,
    species_index: Option<Res<SpeciesAssets>>,
    previews: Query<
        (Entity, &ConstructionPreview, &ObjectPlacementPrefabSource),
        (
            With<ConstructionPreview>,
            Without<ObjectPlacementPreviewPrefabHydrated>,
        ),
    >,
    presentations: Query<(Entity, &ChildOf), With<ObjectPlacementPreviewModelPresentation>>,
) {
    let species = species_index
        .as_ref()
        .zip(species_assets.as_ref())
        .and_then(|(index, assets)| index.get(assets));
    for (entity, preview, source) in &previews {
        let Some(prefab) = prefabs.get(&source.0) else {
            continue;
        };
        let Some(presentation) = presentations
            .iter()
            .find_map(|(candidate, parent)| (parent.parent() == entity).then_some(candidate))
        else {
            continue;
        };
        let (_, renderables, _) =
            spawn_prefab_render_tree(&mut commands, prefab, presentation, false);
        for (renderable, authored) in renderables.into_iter().zip(
            prefab
                .canonical_scene_prefab_document()
                .entities
                .iter()
                .flat_map(|row| &row.renderables),
        ) {
            commands.entity(renderable).insert((
                ObjectPlacementPreviewRenderable,
                ObjectPlacementPreviewOwner(entity),
                PrefabModelTint(Color::WHITE),
            ));
            // Native model scenes do not embed the animal's BFM. Attach its
            // canonical initial pose before the animationless glTF can settle.
            let matched_variant = species.and_then(|species| {
                species_variant_for_placement_preview_model(
                    species,
                    preview.definition,
                    authored.model,
                )
            });
            let requested_animation =
                species.zip(matched_variant).and_then(|(species, variant)| {
                    species.load_variant_animation_set(&asset_server, variant)
                });
            // This attempt is not retried after the render tree is hydrated.
            info!(target: "openzt2_verification", preview = ?entity,
                definition = ?preview.definition, authored_model = ?authored.model,
                model_path = ?prefab.model_asset_path(authored.model),
                species_available = species.is_some(),
                canonical_definition = species.is_some_and(|species| species.find(preview.definition).is_some() || species.find_variant(preview.definition).is_some()),
                matched_variant = ?matched_variant.map(|variant| variant.id),
                canonical_model = ?matched_variant.map(|variant| AssetId::from_virtual_path(&variant.model)),
                initial_clip = ?matched_variant.and_then(|variant| variant.initial_animation_clip_asset_key.as_deref()),
                requested_bfm = ?requested_animation.as_ref().and_then(|handle| asset_server.get_path(handle.id())),
                "placement preview hydration animation attempt");
            if let Some((variant, animation)) = matched_variant.zip(requested_animation) {
                commands
                    .entity(presentation)
                    .insert(AnimationPresentationOwner {
                        gameplay_entity: entity,
                    });
                commands
                    .entity(renderable)
                    .insert(PendingModelAnimationAssets {
                        pending_model_animation_set_asset_handles: vec![animation]
                            .into_boxed_slice(),
                        requested_initial_animation_clip_asset_key: variant
                            .initial_animation_clip_asset_key
                            .clone(),
                    });
            }
        }
        commands
            .entity(entity)
            .insert(ObjectPlacementPreviewPrefabHydrated);
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
        (
            Entity,
            &PrefabModel,
            &ObjectPlacementPreviewOwner,
            &AnimationPlaybackController,
        ),
        Added<ModelAnimationAttachmentResolved>,
    >,
    previews: Query<&ConstructionPreview>,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    let Some(species) = species_index
        .as_ref()
        .zip(species_assets.as_ref())
        .and_then(|(index, assets)| index.get(assets))
    else {
        return;
    };
    for (entity, model, owner, _) in &controllers {
        let Ok(preview) = previews.get(owner.0) else {
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
    mut previews: Query<(
        &mut ConstructionPreview,
        &mut Transform,
        &mut ObjectPlacementPreviewMotion,
        &mut Visibility,
    )>,
) {
    let Ok(cursor) = cursors.single() else { return };
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (mut preview, mut transform, mut motion, mut visibility) in &mut previews {
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
            requested_transform.translation = cursor.world;
        }
        let snapped = if let Some(definition) =
            resolve_object_placeable_definition(catalogue, preview.definition)
        {
            let Some(snapped) =
                snap_object_placement_transform_to_authored_grid(definition, requested_transform)
            else {
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
        (Entity, &Transform, &mut ObjectPlacementPreviewMotion),
        Without<ObjectPlacementPreviewModelPresentation>,
    >,
    mut presentations: Query<
        (&ChildOf, &mut Transform),
        (
            With<ObjectPlacementPreviewModelPresentation>,
            Without<ConstructionPreview>,
        ),
    >,
) {
    for (entity, authoritative, mut motion) in &mut previews {
        motion.advance(time.delta_secs());
        for (parent, mut presentation) in &mut presentations {
            if parent.parent() == entity {
                presentation.translation = authoritative
                    .rotation
                    .inverse()
                    .mul_vec3(motion.current - authoritative.translation);
            }
        }
    }
}
