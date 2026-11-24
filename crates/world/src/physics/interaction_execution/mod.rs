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
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    animal_behavior::behavior_world_release_types::{
        BehaviorWorldObjectReleased, BehaviorWorldReleaseMotion, FallReleaseAnimationClips,
    },
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionQuery},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
        running_actions::{RunBehaviorFall, RunningBehaviorInstruction},
    },
    placement::placement_occupancy_index::PlacedObjectFootprintQueries,
    topology::topology_graph_types::TopologyGrid,
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
    authored_falls: Query<(&RunBehaviorFall, &RunningBehaviorInstruction, &BehaviorTaskExecutionState)>,
    clocks: Query<&ZooClock>,
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
        BehaviorWorldReleaseMotion::Fall { speed_mps } => {
            let authored = authored_falls.get(released.object).ok()
                .filter(|(_, instruction, task)| instruction.is_current(task));
            if object.is_some_and(|object| {
                object
                    .behavior_type_identifiers
                    .contains(&AssetId::from_key("b_donotfall"))
            }) {
                if let Some((_, instruction, _)) = authored {
                    if let Ok(clock) = clocks.get(released.world) {
                        instruction.complete(released.object, clock.tick, &mut commands);
                    }
                }
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
            let action = authored.map(|(action, _, _)| &action.0);
            let airborne = action.and_then(|action| action.airborne.clone()).or(airborne);
            let land_impact = action.and_then(|action| action.land_impact.clone()).or(land_impact);
            let water_impact = action.and_then(|action| action.water_impact.clone()).or(water_impact);
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
                downward_speed: speed_mps,
                resume_body,
                airborne_request,
                land_impact_clip: land_impact,
                water_impact_clip: water_impact,
                instruction: authored.map(|(_, instruction, _)| instruction.id()),
            });
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn advance_falling_released_objects(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    terrain_index: Option<Single<&TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    memberships: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    spatial: SpatialQuery,
    placed_objects: PlacedObjectFootprintQueries,
    topology: Query<&TopologyGrid>,
    (colliders, sensors, clocks): (Query<&ColliderOf>, Query<(), With<Sensor>>, Query<&ZooClock>),
    authored_falls: Query<(&RunningBehaviorInstruction, &BehaviorTaskExecutionState), With<RunBehaviorFall>>,
    mut falling: Query<(Entity, &WorldMember, &mut FallingReleasedObject, &mut Transform)>,
    mut clips: MessageWriter<AnimationClipPlaybackRequest>,
    mut options: MessageWriter<AnimationClipActionOptions>,
    mut cancel: MessageWriter<CancelAnimationClipPlaybackRequest>,
) {
    let terrain_index = terrain_index.map(Single::into_inner);
    let Some(index) = terrain_index.filter(|index| !index.chunks.is_empty()) else {
        return;
    };
    let delta = time.delta_secs();
    for (entity, world, mut fall, mut transform) in &mut falling {
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
        let supporting_collider = (next_height < position.y).then(|| {
            let grid = topology.get(world.root).ok()?;
            let cell = grid.position_cell(position).truncate();
            placed_objects.entities_occupying_cell(world.root, cell)
                .filter(|support| *support != entity)
                .find_map(|support| {
                    let hit = spatial.cast_ray_predicate(
                        position, Dir3::NEG_Y, position.y - next_height, false,
                        &SpatialQueryFilter::default(),
                        &|collider| !sensors.contains(collider)
                            && colliders.get(collider).map_or(collider, |owner| owner.body) == support,
                    )?;
                    let height = position.y - hit.distance;
                    (next_height < height && height < position.y).then_some(height)
                })
        }).flatten();
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
            let impact_request = impact_clip.map(|impact_clip| {
                let request_id = request_release_clip(
                    entity,
                    impact_clip.clone(),
                    AnimationPlaybackRepetitionPolicy::PlayOnce,
                    &mut clips,
                    &mut options,
                );
                (request_id, impact_clip)
            });
            if let Some((step, task)) = authored_falls.get(entity).ok()
                .filter(|(step, task)| Some(step.id()) == fall.instruction && step.is_current(task))
            {
                if let Some((request_id, clip_key)) = impact_request {
                    commands.entity(entity).insert(PendingBehaviorAnimationClipCompletion {
                        instruction: step.id(),
                        request_id,
                        clip_key,
                        target: task.target,
                        repeats: false,
                    });
                } else if let Ok(clock) = clocks.get(world.root) {
                    step.complete(entity, clock.tick, &mut commands);
                }
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
