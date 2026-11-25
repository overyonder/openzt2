use bevy::asset::{asset_changed::AssetChanged, AssetLoadFailedEvent};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_data::scene_prefab::ScenePrefabEntityFlags;
use openzt2_game_data::scene_prefab::ScenePrefabRenderableVisibilityFlags;
use openzt2_game_data::AssetId;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world_state::world_spawn::prefab_model_replacement::PrefabModelReplaced;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabLocalRenderBound;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentationHydrated;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabShadowPolicy;

use crate::model_render::prefab_render_lod_tier_projection::bind_prefab_lod;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;
use openzt2_world_state::world_spawn::prefab_transform_conversion::transform_from_authored;

/// Spawn a prefab for isolated render scenes, such as the shell globe.
pub fn spawn_prefab_render_tree(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    parent: Entity,
    authored_lights: bool,
) -> (Entity, Vec<Entity>, Vec<Entity>) {
    let document = prefab.canonical_scene_prefab_document();
    let entities = document
        .entities
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let transform = transform_from_authored(&row.transform);
            let mut entity = commands.spawn((
                PrefabAuthoredAttachmentIdentifier(AssetId(row.attachment_id.0)),
                transform,
                if row.flags.0 & ScenePrefabEntityFlags::VISIBLE.0 != 0 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
            if row.flags.0 & ScenePrefabEntityFlags::ACTIVE.0 == 0 {
                entity.insert(bevy::ecs::entity_disabling::Disabled);
            }
            if let Some(bound) = row.local_render_bound {
                entity.insert(PrefabLocalRenderBound(bound));
            }
            if let Some(bound) = &row.skin_render_bound {
                entity.insert(openzt2_world_state::world_spawn::prefab_presentation_types::PrefabSkinRenderBound(bound.clone()));
            }
            if let Some(billboard) = row.billboards.first() {
                entity.insert(
                    PrefabAuthoredBillboardOrientationMode::from_authored_mode_and_local_rotation(
                        billboard.mode,
                        transform.rotation,
                    )
                    .facing_positive_z(billboard.faces_positive_z),
                );
            }
            if let Some(motion) = prefab.motion_for_row(index) {
                super::prefab_authored_transform_animation_advancement::insert_prefab_motion(&mut entity, motion, transform.rotation);
            }
            entity.id()
        })
        .collect::<Vec<_>>();
    let root = entities
        .first()
        .copied()
        .expect("scene prefab asset must contain a root entity");
    commands.entity(root).insert(ChildOf(parent));
    let mut renderables = Vec::new();
    for (index, row) in document.entities.iter().enumerate() {
        let spawned = entities[index];
        if let Some(joint) = &row.model_joint_binding {
            commands.entity(spawned).insert(
                openzt2_world_state::animation_playback::model_joint_attachment_binding::ModelJointAttachment::new(root, joint),
            );
        }
        for renderable in &row.renderables {
            let renderable_components =
                crate::world_spawn::prefab_renderable_components::create_prefab_renderable_components(
                    prefab, renderable,
                );
            let renderable_is_the_authored_entity = row.renderables.len() == 1
                && prefab.motion_for_row(index).is_none()
                && renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::VISIBLE.0 != 0;
            let mut renderable_entity = if renderable_is_the_authored_entity {
                let mut authored_entity = commands.entity(spawned);
                authored_entity.insert(renderable_components);
                authored_entity
            } else {
                let mut child = commands.spawn((
                    Transform::IDENTITY,
                    if renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::VISIBLE.0
                        != 0
                    {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    ChildOf(spawned),
                ));
                child.insert(renderable_components);
                child
            };
            super::prefab_renderable_components::attach_prefab_renderable_animations(
                &mut renderable_entity,
                prefab,
                index,
                renderable,
                openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackClock::RealTime,
            );
            let renderable = renderable_entity.id();
            renderables.push(renderable);
        }
        for child in &row.children {
            commands
                .entity(entities[*child as usize])
                .insert(ChildOf(spawned));
        }
    }
    for (entity, row) in document.entities.iter().enumerate() {
        for lod in &row.lods {
            bind_prefab_lod(commands, &entities, entities[entity], lod);
        }
    }
    let lights = if authored_lights {
        crate::model_render::scoped_prefab_lighting::spawn_authored_scene_lights(
            commands, prefab, &entities,
        )
    } else {
        Vec::new()
    };
    (root, renderables, lights)
}

#[derive(Component)]
#[relationship(relationship_target = PrefabPresentationRenderRoot)]
pub(super) struct PrefabPresentationRenderOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = PrefabPresentationRenderOf, linked_spawn)]
pub(super) struct PrefabPresentationRenderRoot(Entity);

#[derive(Component)]
pub(super) struct PrefabPresentationHydrationFailed;

pub(super) fn hydrate_prefab_presentations(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut asset_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    mut released: RemovedComponents<PrefabPresentationHydrated>,
    mut failures: MessageReader<AssetLoadFailedEvent<ScenePrefabAsset>>,
    server: Res<AssetServer>,
    changed: Query<
        Entity,
        (
            Without<PrefabPresentationHydrated>,
            AssetChanged<PrefabPresentation>,
        ),
    >,
    pending: Query<
        (
            Entity,
            Ref<PrefabPresentation>,
            Option<&PrefabShadowPolicy>,
            Has<PrefabPresentationHydrationFailed>,
        ),
        Without<PrefabPresentationHydrated>,
    >,
) {
    let ready_assets = asset_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let released = released
        .read()
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let failed = failures
        .read()
        .map(|event| event.id)
        .collect::<HashSet<_>>();
    for (entity, request, inherited_shadow_policy, failed_previously) in &pending {
        if prefabs.get(&request.0).is_none()
            && (failed.contains(&request.0.id())
                || request.is_added()
                    && matches!(
                        server.load_state(request.0.id()),
                        bevy::asset::LoadState::Failed(_)
                    ))
        {
            if !failed_previously {
                warn!(?entity, asset = ?request.0.id(), "prefab presentation could not be loaded");
                commands
                    .entity(entity)
                    .try_insert(PrefabPresentationHydrationFailed);
            }
            continue;
        }
        if !request.is_changed()
            && !released.contains(&entity)
            && !ready_assets.contains(&request.0.id())
            && !changed.contains(entity)
        {
            continue;
        }
        let Some(prefab) = prefabs.get(&request.0) else {
            continue;
        };
        let (root, renderables, _) = spawn_prefab_render_tree(&mut commands, prefab, entity, false);
        for renderable in renderables {
            let mut renderable_commands = commands.entity(renderable);
            if let Some(shadow_policy) = inherited_shadow_policy {
                renderable_commands.insert(*shadow_policy);
            }
        }
        commands
            .entity(root)
            .insert(PrefabPresentationRenderOf(entity));
        commands
            .entity(entity)
            .try_insert(PrefabPresentationHydrated)
            .try_remove::<PrefabPresentationHydrationFailed>();
    }
}

#[derive(Message)]
pub(super) struct ReplacedPrefabReloaded(pub(super) bevy::asset::AssetId<ScenePrefabAsset>);

pub(super) fn publish_replaced_prefab_reloads(
    mut replacements: MessageReader<PrefabModelReplaced>,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    mut reloaded: MessageWriter<ReplacedPrefabReloaded>,
    asset_server: Res<AssetServer>,
    mut pending_paths: Local<HashSet<String>>,
) {
    pending_paths.extend(
        replacements
            .read()
            .map(|event| event.model_path.to_ascii_lowercase()),
    );
    for event in prefab_events.read() {
        let AssetEvent::Modified { id } = *event else {
            continue;
        };
        if pending_paths.is_empty() {
            continue;
        }
        if let Some(path) = asset_server.get_path(id) {
            if pending_paths.remove(&path.path().to_string_lossy().to_ascii_lowercase()) {
                reloaded.write(ReplacedPrefabReloaded(id));
            }
        }
    }
}

pub(super) fn rebuild_prefab_presentations_after_prefab_reload(
    mut commands: Commands,
    mut reloaded: MessageReader<ReplacedPrefabReloaded>,
    presentations: Query<(Entity, &PrefabPresentation, &PrefabPresentationRenderRoot)>,
) {
    let reloaded = reloaded.read().map(|event| event.0).collect::<HashSet<_>>();
    if reloaded.is_empty() {
        return;
    }
    for (entity, presentation, render_root) in &presentations {
        if reloaded.contains(&presentation.0.id()) {
            commands.entity(render_root.0).try_despawn();
            commands
                .entity(entity)
                .remove::<PrefabPresentationHydrated>();
        }
    }
}
