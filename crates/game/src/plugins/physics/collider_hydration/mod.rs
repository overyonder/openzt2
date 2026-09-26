//! World-boundary, prefab, and deferred model-collider hydration.

use avian3d::prelude::*;
use bevy::{gltf::Gltf, prelude::*};
use openzt2_game_data::scene_prefab::{PrefabColliderSource, ScenePrefabColliderLayerFlags};

use crate::{
    assets::{
        model::model_asset_queries::loaded_gltf_model_mesh_asset_handles,
        scene_prefab::ScenePrefabAsset,
    },
    plugins::world_spawn::{
        world_membership_types::WorldMember, world_membership_types::WorldRoot,
        world_terrain_hydration::WorldTerrainHorizontalBounds,
    },
};

/// Installs the immutable world edges as ordinary Avian collision geometry.
///
/// Half-space colliders preserve radius-aware world boundaries while Avian
/// remains the sole owner of contact response and velocity.
pub(super) fn hydrate_world_boundary_colliders(
    mut commands: Commands,
    worlds: Query<
        (Entity, &WorldTerrainHorizontalBounds),
        (With<WorldRoot>, Added<WorldTerrainHorizontalBounds>),
    >,
) {
    for (root, bounds) in &worlds {
        let planes = [
            (Vec3::X, Vec3::new(bounds.min.x, 0.0, 0.0)),
            (Vec3::NEG_X, Vec3::new(bounds.max.x, 0.0, 0.0)),
            (Vec3::Z, Vec3::new(0.0, 0.0, bounds.min.y)),
            (Vec3::NEG_Z, Vec3::new(0.0, 0.0, bounds.max.y)),
        ];
        for (normal, translation) in planes {
            commands.spawn((
                RigidBody::Static,
                Collider::half_space(normal),
                Transform::from_translation(translation),
                WorldMember { root },
            ));
        }
    }
}

/// Convert supported prefab collision bits to Avian membership and filter masks.
#[inline]
fn convert_prefab_collision_layer_flags_to_avian_collision_layers(
    membership: ScenePrefabColliderLayerFlags,
    mask: ScenePrefabColliderLayerFlags,
) -> Option<CollisionLayers> {
    ((membership.0 | mask.0) & !ScenePrefabColliderLayerFlags::ALL_BITS == 0)
        .then(|| CollisionLayers::from_bits(u32::from(membership.0), u32::from(mask.0)))
}

/// Projects all collider rows for one prefab instance directly into Avian.
/// Collider children are ordinary world members, so standard world teardown
/// removes them and Avian owns all live shape/body/filter state.
pub(crate) fn hydrate_scene_prefab_colliders_into_avian_physics(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    entities: &[Entity],
    world_root: Entity,
    prefab_collider_rigid_body: RigidBody,
) {
    let document = prefab.canonical_scene_prefab_document();
    for (entity_index, entity) in document.entities.iter().enumerate() {
        let owner = entities[entity_index];
        for row in &entity.colliders {
            let membership = ScenePrefabColliderLayerFlags(row.layer.0);
            let layers = convert_prefab_collision_layer_flags_to_avian_collision_layers(
                membership,
                ScenePrefabColliderLayerFlags(row.mask.0),
            )
            .expect("validated prefab collision layers");
            let collider = match &row.source {
                PrefabColliderSource::Box { half_extent_m } => {
                    let half = Vec3::from_array(*half_extent_m);
                    Some(Collider::cuboid(half.x * 2.0, half.y * 2.0, half.z * 2.0))
                }
                PrefabColliderSource::Capsule {
                    radius_m,
                    half_height_m,
                } => Some(Collider::capsule(*radius_m, *half_height_m * 2.0)),
                PrefabColliderSource::Model { model, index } => {
                    let model = prefab
                        .loaded_model_asset_handle(*model)
                        .expect("resolved prefab collider model dependency")
                        .clone();
                    let mut pending = commands.spawn((
                        PendingModelCollider {
                            model,
                            primitive: usize::from(*index),
                        },
                        layers,
                        Transform::IDENTITY,
                        ChildOf(owner),
                        WorldMember { root: world_root },
                    ));
                    if membership.0 & ScenePrefabColliderLayerFlags::PROJECTILE.0 != 0 {
                        pending.insert(CollisionEventsEnabled);
                    }
                    None
                }
            };
            let projectile = membership.0 & ScenePrefabColliderLayerFlags::PROJECTILE.0 != 0;
            if let Some(collider) = collider {
                let mut collider_commands = commands.spawn((
                    collider,
                    layers,
                    Transform::IDENTITY,
                    ChildOf(owner),
                    WorldMember { root: world_root },
                ));
                if projectile {
                    collider_commands.insert(CollisionEventsEnabled);
                }
            }
            let mut owner_commands = commands.entity(owner);
            owner_commands.insert(prefab_collider_rigid_body);
            if projectile && prefab_collider_rigid_body.is_dynamic() {
                owner_commands.insert(SweptCcd::default());
            }
        }
    }
}

#[derive(Component)]
pub(super) struct PendingModelCollider {
    model: Handle<Gltf>,
    primitive: usize,
}

pub(super) fn replace_loaded_model_collider_requests_with_avian_triangle_meshes(
    mut commands: Commands,
    pending: Query<(Entity, &PendingModelCollider)>,
    models: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<bevy::gltf::GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
) {
    for (entity, pending) in &pending {
        let Some(model) = models.get(&pending.model) else {
            continue;
        };
        let Some(mesh) = loaded_gltf_model_mesh_asset_handles(model, &gltf_meshes)
            .nth(pending.primitive)
            .and_then(|handle| meshes.get(handle))
        else {
            continue;
        };
        let Some(collider) = Collider::trimesh_from_mesh(mesh) else {
            commands.entity(entity).despawn();
            continue;
        };
        commands
            .entity(entity)
            .insert(collider)
            .remove::<PendingModelCollider>();
    }
}
