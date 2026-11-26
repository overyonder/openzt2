mod carried_prefab_presentation;
mod fame_gate_prefab_replacement;
pub mod morph_fade_projection;

use super::prefab_presentation_render_tree::ReplacedPrefabReloaded;
use avian3d::prelude::RigidBody;
use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::query::Allow;
use bevy::platform::collections::HashSet;
use bevy::prelude::ChildOf;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::Handle;
use bevy::prelude::Transform;
use bevy::prelude::Visibility;
use bevy::prelude::{warn, Assets, Component, Has, MessageReader, Query, Res, With};
use openzt2_game_data::scene_prefab::PrefabEntity;
use openzt2_game_data::scene_prefab::ScenePrefabEntityFlags;
use openzt2_game_data::scene_prefab::ScenePrefabRenderableVisibilityFlags;
use openzt2_game_data::AssetId;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::{
    PrefabMaterialOverrides, PrefabModel, PrefabRenderable, PrefabShadowPolicy,
};

use crate::physics::collider_hydration::hydrate_scene_prefab_colliders_into_avian_physics;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_interface_state::information::entity_selection_types::Inspectable;

use crate::model_render::prefab_render_lod_tier_projection::bind_prefab_lod;
use crate::world_spawn::prefab_effect_trigger_hydration::spawn_prefab_effect_triggers;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;
use openzt2_world_state::world_spawn::prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabLocalRenderBound;
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;
use openzt2_world_state::world_spawn::prefab_transform_conversion::transform_from_authored;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

pub(crate) fn register_prefab_instance_animation(app: &mut bevy::prelude::App) {
    carried_prefab_presentation::register_carried_prefab_presentation(app);
    morph_fade_projection::register_prefab_morph_fades(app);
    fame_gate_prefab_replacement::register_fame_gate_prefab_replacement(app);
    super::named_physical_presentation_projection::animation_key_commands::register_animation_named_physical_presentation_commands(app);
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_loaded_scene_prefab_as_world_instance(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    prefab_handle: Handle<ScenePrefabAsset>,
    world_root: Entity,
    definition: AssetId,
    persistent_id: PersistentId,
    instance_transform: Transform,
    instance_visible: bool,
    instance_parent: Option<Entity>,
    prefab_collider_rigid_body: RigidBody,
) -> Entity {
    let root = commands.spawn_empty().id();
    hydrate_loaded_scene_prefab_into_world_instance_root(
        commands,
        root,
        prefab,
        prefab_handle,
        world_root,
        definition,
        persistent_id,
        instance_transform,
        instance_visible,
        instance_parent,
        prefab_collider_rigid_body,
    );
    root
}

/// Hydrate a newly admitted world entity without replacing its live identity.
/// The root must not already own a hydrated prefab hierarchy.
#[allow(clippy::too_many_arguments)]
pub fn hydrate_loaded_scene_prefab_into_world_instance_root(
    commands: &mut Commands,
    root: Entity,
    prefab: &ScenePrefabAsset,
    prefab_handle: Handle<ScenePrefabAsset>,
    world_root: Entity,
    definition: AssetId,
    persistent_id: PersistentId,
    instance_transform: Transform,
    instance_visible: bool,
    instance_parent: Option<Entity>,
    prefab_collider_rigid_body: RigidBody,
) {
    hydrate_world_instance_prefab_rows(
        commands,
        root,
        prefab,
        world_root,
        instance_transform,
        instance_visible,
        prefab_collider_rigid_body,
        Some(NewPrefabInstanceRoot {
            definition,
            persistent_id,
            prefab_handle,
            parent: instance_parent,
        }),
    );
}

struct NewPrefabInstanceRoot {
    definition: AssetId,
    persistent_id: PersistentId,
    prefab_handle: Handle<ScenePrefabAsset>,
    parent: Option<Entity>,
}

fn prefab_instance_transform(root_transform: Transform, authored_root: &PrefabEntity) -> Transform {
    let authored = transform_from_authored(&authored_root.transform);
    let scale = root_transform.scale / authored.scale;
    let rotation = root_transform.rotation * authored.rotation.inverse();
    Transform {
        translation: root_transform.translation - rotation * (scale * authored.translation),
        rotation,
        scale,
    }
}

#[allow(clippy::too_many_arguments)]
fn hydrate_world_instance_prefab_rows(
    commands: &mut Commands,
    root: Entity,
    prefab: &ScenePrefabAsset,
    world_root: Entity,
    instance_transform: Transform,
    instance_visible: bool,
    prefab_collider_rigid_body: RigidBody,
    new_root: Option<NewPrefabInstanceRoot>,
) {
    let document = prefab.canonical_scene_prefab_document();
    let mut entities = Vec::with_capacity(document.entities.len());

    for (index, row) in document.entities.iter().enumerate() {
        let local = transform_from_authored(&row.transform);
        let transform = if index == 0 {
            instance_transform.mul_transform(local)
        } else {
            local
        };
        let visible = instance_visible && row.flags.0 & ScenePrefabEntityFlags::VISIBLE.0 != 0;
        let mut entity = if index == 0 {
            commands.entity(root)
        } else {
            commands.spawn_empty()
        };
        if index != 0 || new_root.is_some() {
            entity.insert((
                PrefabInstanceRow(index),
                WorldMember { root: world_root },
                PrefabAuthoredAttachmentIdentifier(AssetId(row.attachment_id.0)),
                transform,
                if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
        }
        let spawned = entity.id();
        if let Some(bound) = row.local_render_bound {
            entity.insert(PrefabLocalRenderBound(bound));
        }
        if let Some(bound) = &row.skin_render_bound {
            entity.insert(
                openzt2_world_state::world_spawn::prefab_presentation_types::PrefabSkinRenderBound(
                    bound.clone(),
                ),
            );
        }
        if row.flags.0 & ScenePrefabEntityFlags::ACTIVE.0 == 0 {
            entity.insert(Disabled);
        }
        if let Some(root) = new_root.as_ref().filter(|_| index == 0) {
            entity.insert((
                DefinitionId(root.definition),
                root.persistent_id,
                PrefabObjectPresentationAttachmentProjection(root.definition),
                PrefabSourceAssetHandle(root.prefab_handle.clone()),
            ));
        } else if index != 0 {
            entity.insert(PrefabInstanceRowOf(root));
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
            super::prefab_authored_transform_animation_advancement::insert_prefab_motion(
                &mut entity,
                motion,
                transform.rotation,
            );
        }
        if let Some(root) = new_root.as_ref().filter(|_| index == 0) {
            entity.insert(Inspectable {
                definition: root.definition,
            });
        }
        drop(entity);
        attach_prefab_row_renderables(commands, prefab, row, index, spawned, world_root, visible);
        entities.push(spawned);
    }

    for (entity, row) in document.entities.iter().enumerate() {
        if let Some(joint) = &row.model_joint_binding {
            commands.entity(entities[entity]).insert(
                openzt2_world_state::animation_playback::model_joint_attachment_binding::ModelJointAttachment::new(entities[0], joint),
            );
        }
        for lod in &row.lods {
            bind_prefab_lod(commands, &entities, entities[entity], lod);
        }
    }

    // Preserve the prefab's parent-child hierarchy.
    if let Some(parent) = new_root.and_then(|root| root.parent) {
        commands.entity(entities[0]).insert(ChildOf(parent));
    }
    for (parent_index, row) in document.entities.iter().enumerate() {
        for child_index in &row.children {
            commands
                .entity(entities[*child_index as usize])
                .insert(ChildOf(entities[parent_index]));
        }
    }

    crate::model_render::scoped_prefab_lighting::spawn_authored_scene_lights(
        commands, prefab, &entities,
    );

    hydrate_scene_prefab_colliders_into_avian_physics(
        commands,
        prefab,
        &entities,
        world_root,
        prefab_collider_rigid_body,
    );

    spawn_prefab_effect_triggers(commands, prefab, &entities, world_root);
}

#[derive(Component)]
pub(super) struct PrefabInstanceRow(usize);

#[derive(Component)]
#[relationship(relationship_target = PrefabInstanceRows)]
pub(super) struct PrefabInstanceRowOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = PrefabInstanceRowOf, linked_spawn)]
pub(super) struct PrefabInstanceRows(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = PrefabRowRenderables)]
pub(super) struct PrefabRowRenderableOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = PrefabRowRenderableOf, linked_spawn)]
pub(super) struct PrefabRowRenderables(Vec<Entity>);

/// A renderable to release before its world instance's renderables are
/// rebuilt. `despawn` marks an entity that existed only to render.
#[derive(Component)]
pub(crate) struct ReleasePrefabRenderable {
    despawn: bool,
}

/// A world instance waiting for its released renderables before attaching
/// the renderables of its reloaded prefab.
#[derive(Component)]
pub(super) struct PrefabInstanceRenderablesRebuilding;

fn attach_prefab_row_renderables(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    row: &PrefabEntity,
    row_index: usize,
    spawned: Entity,
    world_root: Entity,
    visible: bool,
) {
    for renderable in &row.renderables {
        let renderable_visible = visible
            && renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::VISIBLE.0 != 0;
        let renderable_components =
            crate::world_spawn::prefab_renderable_components::create_prefab_renderable_components(
                prefab, renderable,
            );
        let renderable_is_the_authored_entity = row.renderables.len() == 1
            && prefab.motion_for_row(row_index).is_none()
            && renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::VISIBLE.0 != 0;
        let mut renderable_entity = if renderable_is_the_authored_entity {
            let mut authored_entity = commands.entity(spawned);
            authored_entity.insert(renderable_components);
            authored_entity
        } else {
            let mut child = commands.spawn((
                WorldMember { root: world_root },
                Transform::IDENTITY,
                if renderable_visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                ChildOf(spawned),
                PrefabRowRenderableOf(spawned),
            ));
            child.insert(renderable_components);
            child
        };
        super::prefab_renderable_components::attach_prefab_renderable_animations(
            &mut renderable_entity,
            prefab,
            row_index,
            renderable,
            openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackClock::Simulation,
        );
    }
}

/// Starts rebuilding the renderables of world instances whose prefab
/// reloaded, such as a model replaced from an editor. Gameplay components,
/// colliders and the instance's entities stay as they are.
pub(super) fn begin_rebuilding_world_instance_renderables_after_prefab_reload(
    mut commands: Commands,
    mut reloaded: MessageReader<ReplacedPrefabReloaded>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    instances: Query<(
        Entity,
        &PrefabSourceAssetHandle,
        Option<&PrefabInstanceRows>,
    )>,
    row_renderables: Query<&PrefabRowRenderables, Allow<Disabled>>,
    renderables: Query<(), (With<PrefabRenderable>, Allow<Disabled>)>,
) {
    let reloaded = reloaded.read().map(|event| event.0).collect::<HashSet<_>>();
    if reloaded.is_empty() {
        return;
    }
    for (root, handle, rows) in &instances {
        if !reloaded.contains(&handle.0.id()) {
            continue;
        }
        let Some(prefab) = prefabs.get(&handle.0) else {
            continue;
        };
        if prefab.canonical_scene_prefab_document().entities.len()
            != 1 + rows.map_or(0, |rows| rows.0.len())
        {
            warn!("reloaded prefab changed its entities; place the object again to see it");
            continue;
        }
        for row in
            std::iter::once(root).chain(rows.into_iter().flat_map(|rows| rows.0.iter().copied()))
        {
            if renderables.contains(row) {
                commands
                    .entity(row)
                    .insert(ReleasePrefabRenderable { despawn: false });
            }
            if let Ok(renderables) = row_renderables.get(row) {
                for &child in &renderables.0 {
                    commands
                        .entity(child)
                        .insert(ReleasePrefabRenderable { despawn: true });
                }
            }
        }
        commands
            .entity(root)
            .insert(PrefabInstanceRenderablesRebuilding);
    }
}

/// Once the renderer has released the old renderables, removes them and
/// attaches the reloaded prefab's renderables to the same row entities.
pub(super) fn finish_rebuilding_world_instance_renderables(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    instances: Query<
        (
            Entity,
            &PrefabSourceAssetHandle,
            Option<&PrefabInstanceRows>,
            &WorldMember,
        ),
        With<PrefabInstanceRenderablesRebuilding>,
    >,
    released: Query<(&ReleasePrefabRenderable, Has<ModelExpanded>), Allow<Disabled>>,
    authored_rows: Query<
        (
            &PrefabInstanceRow,
            &Visibility,
            Option<&PrefabRowRenderables>,
        ),
        Allow<Disabled>,
    >,
) {
    for (root, prefab_handle, rows, member) in &instances {
        let rows = || {
            std::iter::once(root).chain(rows.into_iter().flat_map(|rows| rows.0.iter().copied()))
        };
        let row_and_draws = |row| {
            std::iter::once(row).chain(
                authored_rows
                    .get(row)
                    .ok()
                    .and_then(|(_, _, draws)| draws)
                    .into_iter()
                    .flat_map(|draws| draws.0.iter().copied()),
            )
        };
        if rows()
            .flat_map(row_and_draws)
            .any(|entity| released.get(entity).is_ok_and(|(_, expanded)| expanded))
        {
            continue;
        }
        let Some(prefab) = prefabs.get(&prefab_handle.0) else {
            continue;
        };
        let document = prefab.canonical_scene_prefab_document();
        if document.entities.len() != rows().count() {
            warn!("reloaded prefab changed its entities; place the object again to see it");
            for entity in rows().flat_map(row_and_draws) {
                commands.entity(entity).remove::<ReleasePrefabRenderable>();
            }
            commands
                .entity(root)
                .remove::<PrefabInstanceRenderablesRebuilding>();
            continue;
        }
        for entity in rows().flat_map(row_and_draws) {
            let Ok((release, _)) = released.get(entity) else {
                continue;
            };
            if release.despawn {
                commands.entity(entity).despawn();
            } else {
                commands.entity(entity).remove::<(
                    ReleasePrefabRenderable,
                    PrefabRenderable,
                    PrefabModel,
                    PrefabMaterialOverrides,
                    PrefabShadowPolicy,
                )>();
            }
        }
        for spawned in rows() {
            let Ok((index, visibility, _)) = authored_rows.get(spawned) else {
                continue;
            };
            let Some(row) = document.entities.get(index.0) else {
                continue;
            };
            let visible = *visibility != Visibility::Hidden;
            attach_prefab_row_renderables(
                &mut commands,
                prefab,
                row,
                index.0,
                spawned,
                member.root,
                visible,
            );
        }
        commands
            .entity(root)
            .remove::<PrefabInstanceRenderablesRebuilding>();
    }
}
