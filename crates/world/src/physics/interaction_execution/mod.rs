//! Gameplay impulse application to Avian bodies.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::physics::interaction_types::ApplyPhysicsImpulseRequest;
use crate::physics::interaction_types::PhysicsInteractionFailure;
use crate::physics::interaction_types::PhysicsInteractionFailureReason;

/// Applies validated point impulses only to authoritative dynamic Avian bodies.
pub(super) fn apply_requested_impulses_to_dynamic_avian_bodies(
    mut requests: MessageReader<ApplyPhysicsImpulseRequest>,
    mut bodies: Query<(&RigidBody, Forces)>,
    mut failed: MessageWriter<PhysicsInteractionFailure>,
) {
    for request in requests.read() {
        if !request.impulse_ns.is_finite() || !request.point_world.is_finite() {
            failed.write(PhysicsInteractionFailure {
                entity: request.entity,
                reason: PhysicsInteractionFailureReason::InvalidTarget,
            });
            continue;
        }
        let Ok((body, mut forces)) = bodies.get_mut(request.entity) else {
            failed.write(PhysicsInteractionFailure {
                entity: request.entity,
                reason: PhysicsInteractionFailureReason::MissingBody,
            });
            continue;
        };
        if *body != RigidBody::Dynamic {
            failed.write(PhysicsInteractionFailure {
                entity: request.entity,
                reason: PhysicsInteractionFailureReason::InvalidTarget,
            });
            continue;
        }
        forces.apply_linear_impulse_at_point(request.impulse_ns, request.point_world);
    }
}

use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    animation_graph::animation_graph_playback_message_types::{
        AnimationClipActionOptions, AnimationClipPlaybackRequest,
        CancelAnimationClipPlaybackRequest,
    },
    animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    animal_behavior::behavior_world_release_types::{
        BehaviorWorldObjectReleased, BehaviorWorldReleaseMotion, FallReleaseAnimationClips,
    },
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_collision_rebuilding_types::TerrainChunkCollisionSurface,
        terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionQuery},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
};

use super::{
    authored_water_surface_impact_projection::sample_accepted_water_surface,
    interaction_types::FallingReleasedObject,
};

pub(super) fn release_behavior_object_into_world(
    released: On<BehaviorWorldObjectReleased>,
    mut commands: Commands,
    mut objects: Query<(
        &WorldMember,
        &mut Transform,
        Option<&DefinitionId>,
        Option<&RigidBody>,
        Has<RigidBodyDisabled>,
        Has<FallingReleasedObject>,
        Option<&FallReleaseAnimationClips>,
    )>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut clips: MessageWriter<AnimationClipPlaybackRequest>,
    mut options: MessageWriter<AnimationClipActionOptions>,
) {
    let Ok((member, mut transform, definition, body, disabled, falling, animation)) =
        objects.get_mut(released.object)
    else {
        return;
    };
    if member.root != released.world || falling || !released.pose.translation.is_finite() {
        return;
    }
    transform.set_if_neq(released.pose);
    let object =
        definition.and_then(|definition| definitions.get(&assets)?.find_object(definition.0));
    let mut entity = commands.entity(released.object);
    match released.motion {
        BehaviorWorldReleaseMotion::Drop => {
            if let Some(body) = body {
                if *body != RigidBody::Dynamic
                    && object.is_some_and(|object| object.real_physics_water_impact.is_some())
                {
                    entity.insert(RigidBody::Dynamic);
                }
                if disabled {
                    entity.remove::<RigidBodyDisabled>();
                }
            }
        }
        BehaviorWorldReleaseMotion::Fall => {
            if object.is_some_and(|object| {
                object
                    .behavior_type_identifiers
                    .contains(&AssetId::from_key("b_donotfall"))
            }) {
                return;
            }
            let resume_body = body.is_some() && !disabled;
            if resume_body {
                entity.insert(RigidBodyDisabled);
            }
            let FallReleaseAnimationClips {
                airborne,
                land_impact,
                water_impact,
            } = animation
                .cloned()
                .or_else(|| {
                    let clips = &object?.authored_runtime.fall_animation_clips;
                    Some(FallReleaseAnimationClips {
                        airborne: clips.airborne.clone(),
                        land_impact: clips.land_impact.clone(),
                        water_impact: clips.water_impact.clone(),
                    })
                })
                .unwrap_or_default();
            let airborne_request = airborne.map(|clip| {
                request_release_clip(
                    released.object,
                    clip,
                    AnimationPlaybackRepetitionPolicy::Loop,
                    &mut clips,
                    &mut options,
                )
            });
            entity.insert(FallingReleasedObject {
                downward_speed: 0.0,
                resume_body,
                airborne_request,
                land_impact_clip: land_impact,
                water_impact_clip: water_impact,
            });
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn advance_falling_released_objects(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    terrain_index: Option<Res<TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    memberships: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    spatial: SpatialQuery,
    colliders: Query<&ColliderOf>,
    nonsupporting_colliders: Query<(), Or<(With<TerrainChunkCollisionSurface>, With<Sensor>)>>,
    static_bodies: Query<&RigidBody>,
    parents: Query<&ChildOf>,
    mut falling: Query<(Entity, &mut FallingReleasedObject, &mut Transform)>,
    mut clips: MessageWriter<AnimationClipPlaybackRequest>,
    mut options: MessageWriter<AnimationClipActionOptions>,
    mut cancel: MessageWriter<CancelAnimationClipPlaybackRequest>,
) {
    let Some(index) = terrain_index.filter(|index| !index.chunks.is_empty()) else {
        return;
    };
    let delta = time.delta_secs();
    for (entity, mut fall, mut transform) in &mut falling {
        let position = transform.translation;
        let Some(chunk) = terrain_chunk_at(&index, position.xz()) else {
            commands.entity(entity).try_despawn();
            continue;
        };
        let ground = chunks.get(chunk).ok().and_then(|(chunk, edited)| {
            let terrain = terrain_assets.get(&chunk.asset)?;
            sample_terrain(chunk, terrain, edited, position.xz())
        });
        let Some(ground) = ground else { continue };
        fall.downward_speed += 9.0 * delta;
        let next_height = position.y - fall.downward_speed * delta;
        let water = sample_accepted_water_surface(
            &index,
            &terrain_assets,
            &memberships,
            &regions,
            position.xz(),
        )
        .map(|(height, _)| height);
        let supporting_collider = spatial
            .cast_ray_predicate(
                position,
                Dir3::NEG_Y,
                position.y - next_height,
                false,
                &SpatialQueryFilter::default(),
                &|collider| {
                    let body = colliders.get(collider).map_or(collider, |owner| owner.body);
                    body != entity
                        && !nonsupporting_colliders.contains(collider)
                        && !parents
                            .iter_ancestors(collider)
                            .any(|owner| owner == entity)
                        && static_bodies
                            .get(body)
                            .is_ok_and(|body| *body == RigidBody::Static)
                },
            )
            .map(|hit| position.y - hit.distance);
        if let Some((support, in_water)) = fall_landing_surface(
            position.y,
            next_height,
            ground.height_m,
            water,
            supporting_collider,
        ) {
            transform.translation.y = support;
            let mut object = commands.entity(entity);
            object.remove::<FallingReleasedObject>();
            if fall.resume_body {
                object.remove::<RigidBodyDisabled>();
            }
            if let Some(request_id) = fall.airborne_request {
                cancel.write(CancelAnimationClipPlaybackRequest {
                    animation_subject_entity: entity,
                    request_id,
                });
            }
            let impact_clip = if in_water {
                fall.water_impact_clip.take()
            } else {
                fall.land_impact_clip.take()
            };
            if let Some(impact_clip) = impact_clip {
                request_release_clip(
                    entity,
                    impact_clip,
                    AnimationPlaybackRepetitionPolicy::PlayOnce,
                    &mut clips,
                    &mut options,
                );
            }
        } else {
            transform.translation.y = next_height;
        }
    }
}

/// Raised solid surfaces take priority over the terrain cell's water and ground.
pub(super) fn fall_landing_surface(
    previous_height: f32,
    next_height: f32,
    ground: f32,
    water: Option<f32>,
    collider: Option<f32>,
) -> Option<(f32, bool)> {
    if let Some(height) =
        collider.filter(|height| next_height < *height && *height < previous_height)
    {
        return Some((height, false));
    }
    if let Some(height) = water.filter(|height| next_height < *height) {
        return Some((height, true));
    }
    (next_height < ground).then_some((ground, false))
}

fn request_release_clip(
    subject: Entity,
    clip: String,
    repetition: AnimationPlaybackRepetitionPolicy,
    clips: &mut MessageWriter<AnimationClipPlaybackRequest>,
    options: &mut MessageWriter<AnimationClipActionOptions>,
) -> usize {
    let request_id = clips
        .write(AnimationClipPlaybackRequest {
            animation_subject_entity: subject,
            animation_clip_asset_key: clip,
            blend_duration_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_repetition_policy: repetition,
        })
        .id;
    options.write(AnimationClipActionOptions {
        request_id,
        binder: None,
        ground_fit: Some(false),
        interrupts_current_animation: true,
        interruptible: false,
    });
    request_id
}
