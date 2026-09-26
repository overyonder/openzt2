//! Creates, hydrates, synchronizes, and animates object-placement previews.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionCursor;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_interaction_types::PlacementValidity;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use crate::plugins::habitat::habitat_types::HabitatLocatable;
use crate::plugins::world_spawn::prefab_model_tint::PrefabModelTint;
use crate::plugins::world_spawn::prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection;
use crate::plugins::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    object_placement_definition_queries::resolve_object_placeable_definition,
    object_placement_validation::snap_object_placement_transform_to_authored_grid,
    ObjectPlacementPrefabSource, ObjectPlacementPreviewModelPresentation,
    ObjectPlacementPreviewMotion, ObjectPlacementPreviewOwner,
    ObjectPlacementPreviewPermissionFacts, ObjectPlacementPreviewPrefabHydrated,
    ObjectPlacementPreviewRenderable, ObjectPlacementPreviewRequest,
    PlacedObjectDefinitionReference, PlacedObjectRelocationSource, PlacementRotationRequest,
    RelocatingPlacedObject,
};

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
    // Objects without a mainObj model, such as food, show their first
    // required named presentation when newly placed.
    let preview_scene = if object.prefab == AssetId::default() {
        object
            .named_physical_presentations
            .iter()
            .find(|presentation| presentation.required && presentation.prefab != AssetId::default())
            .map(|presentation| presentation.prefab)
    } else {
        Some(object.prefab)
    };
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
    previews: Query<
        (Entity, &ObjectPlacementPrefabSource),
        (
            With<ConstructionPreview>,
            Without<ObjectPlacementPreviewPrefabHydrated>,
        ),
    >,
    presentations: Query<(Entity, &ChildOf), With<ObjectPlacementPreviewModelPresentation>>,
) {
    for (entity, source) in &previews {
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
        for renderable in renderables {
            commands.entity(renderable).insert((
                ObjectPlacementPreviewRenderable,
                ObjectPlacementPreviewOwner(entity),
                PrefabModelTint(Color::WHITE),
            ));
        }
        commands
            .entity(entity)
            .insert(ObjectPlacementPreviewPrefabHydrated);
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
