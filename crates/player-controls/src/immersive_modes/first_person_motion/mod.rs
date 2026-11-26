//! Player-driven walking and surface fitting for the immersive camera.

use avian3d::prelude::{
    Collider, LinearVelocity, RigidBody, ShapeCastConfig, SpatialQuery, SpatialQueryFilter,
};
use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::world_definitions::camera_definitions::{
    FirstPersonCameraMotion, FirstPersonSurfaceMotion,
};
use openzt2_interface_state::{
    camera::camera_runtime_state_types::{
        CameraDefinition, CameraMode, FirstPersonLocomotionTuning, ZooCamera,
    },
    ui::{
        input_frame_capture_types::AuthoredModalInputCapture,
        pointer_capture_types::UiPointerCapture,
    },
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    locomotion::locomotion_types::DirectLocomotion,
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
        terrain_world_sampling_types::TerrainPoint,
    },
};

use crate::{
    camera::camera_execution_types::{CameraBounds, CameraIntent, CameraTransition},
    input::input_execution_types::DeviceControlAxes,
};

use super::{
    first_person_mode_control_operations::allowed_first_person_movement_axes,
    immersive_control_execution_types::FirstPersonViewAngles,
    immersive_mode_entry_and_policy_operations::find_authored_immersive_mode_policy,
    immersive_mode_policy_types::AllowedInteractionActions,
    immersive_mode_state_types::{
        ActiveImmersiveMode, ImmersiveCamera, ImmersiveMode, ImmersiveSubject,
    },
};

#[derive(Component, Default)]
pub(super) struct FirstPersonMotion {
    pointer_pan: bool,
    movement: Vec2,
    planar_velocity: Vec2,
    vertical_velocity: f32,
    surface: Option<Surface>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Surface {
    #[default]
    Land,
    Water,
    Underwater,
}

impl Surface {
    fn tuning(self, authored: &FirstPersonCameraMotion) -> FirstPersonSurfaceMotion {
        match self {
            Self::Land => authored.land,
            Self::Water => authored.on_water,
            Self::Underwater => authored.underwater,
        }
    }
}

pub(super) fn sample_first_person_movement_input(
    mut sessions: Query<(
        &ActiveImmersiveMode,
        &ImmersiveCamera,
        Option<&AllowedInteractionActions>,
        &mut FirstPersonMotion,
    )>,
    cameras: Query<&CameraIntent, With<ZooCamera>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    axes: Res<DeviceControlAxes>,
    mouse: Res<ButtonInput<MouseButton>>,
    pointer: Res<UiPointerCapture>,
    modal: Res<AuthoredModalInputCapture>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (mode, camera, allowed, mut motion) in &mut sessions {
        let Some(policy) = find_authored_immersive_mode_policy(definitions, mode.mode)
            .and_then(|policy| policy.first_person_movement.as_ref())
        else {
            continue;
        };
        if !mouse.pressed(MouseButton::Left) || modal.0.is_some() {
            motion
                .reborrow()
                .map_unchanged(|motion| &mut motion.pointer_pan)
                .set_if_neq(false);
        } else if mouse.just_pressed(MouseButton::Left) && !pointer.over_ui {
            motion
                .reborrow()
                .map_unchanged(|motion| &mut motion.pointer_pan)
                .set_if_neq(true);
        }
        let input = if modal.0.is_none() {
            let pointer_axes = if motion.pointer_pan {
                policy
                    .primary_pointer_pan
                    .map_or(Vec2::ZERO, Vec2::from_array)
            } else {
                Vec2::ZERO
            };
            let mut pan = axes.pan;
            if let Ok(intent) = cameras.get(camera.0) {
                let commands = intent.first_person_commands;
                if commands.forward || commands.backward {
                    pan.y = commands.forward_axis();
                }
                if commands.turn_left {
                    pan.x = pan.x.max(0.0);
                }
                if commands.turn_right {
                    pan.x = pan.x.min(0.0);
                }
                pan += intent.ui_pan;
            }
            allowed_first_person_movement_axes(pan + pointer_axes, allowed)
        } else {
            Vec2::ZERO
        };
        motion
            .map_unchanged(|motion| &mut motion.movement)
            .set_if_neq(input);
    }
}

#[derive(SystemParam)]
pub(super) struct FirstPersonSurfaces<'w, 's> {
    indices: Query<'w, 's, &'static TerrainIndex>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    assets: Res<'w, Assets<TerrainAsset>>,
}

impl FirstPersonSurfaces<'_, '_> {
    fn at(&self, root: Entity, position: Vec2, radius: f32) -> Option<TerrainPoint> {
        let index = self.indices.get(root).ok()?;
        let sample = |point| {
            let chunk = terrain_chunk_at(index, point)?;
            let (chunk, edited) = self.chunks.get(chunk).ok()?;
            sample_terrain(chunk, self.assets.get(&chunk.asset)?, edited, point)
        };
        let mut center = sample(position)?;
        for axis in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
            if let Some(edge) = sample(position + axis * radius) {
                center.height_m = center.height_m.max(edge.height_m);
            }
        }
        Some(center)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn advance_first_person_horizontal_motion(
    time: Res<Time<Real>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    surfaces: FirstPersonSurfaces,
    spatial: SpatialQuery,
    parents: Query<&ChildOf>,
    terrain_chunks: Query<(), With<TerrainChunk>>,
    mut bodies: Query<(&RigidBody, &mut LinearVelocity)>,
    mut sessions: Query<(
        &ActiveImmersiveMode,
        &ImmersiveCamera,
        Option<&ImmersiveSubject>,
        Option<&FirstPersonViewAngles>,
        &mut FirstPersonMotion,
    )>,
    mut cameras: Query<
        (
            &mut Transform,
            &WorldMember,
            &CameraDefinition,
            &CameraMode,
            &CameraBounds,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
    mut controlled: Query<(
        &GlobalTransform,
        &mut DirectLocomotion,
        &mut FirstPersonLocomotionTuning,
    )>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let elapsed = time.delta_secs();
    if elapsed <= 0.0 {
        return;
    }
    for (mode, camera, subject, angles, motion) in &mut sessions {
        let Ok((pose, world, selected, camera_mode, bounds)) = cameras.get_mut(camera.0) else {
            continue;
        };
        let (Some(camera_definition), Some(mode_policy)) = (
            definitions.find_camera(selected.0),
            find_authored_immersive_mode_policy(definitions, mode.mode)
                .and_then(|policy| policy.first_person_movement.as_ref()),
        ) else {
            continue;
        };
        let Some(camera_policy) = camera_definition.first_person_motion.as_ref() else {
            continue;
        };
        let movement = motion.movement;
        if mode.mode == ImmersiveMode::FirstPerson {
            let Some(subject) = subject else {
                continue;
            };
            if let Ok((subject_pose, drive, mut tuning)) = controlled.get_mut(subject.0) {
                let tuning_surface = surfaces
                    .at(
                        world.root,
                        subject_pose.translation().xz(),
                        camera_policy.fit_radius_m,
                    )
                    .map_or(Surface::Land, |surface| {
                        surface_at(surface, pose.translation.y, camera_policy)
                    });
                let surface = tuning_surface.tuning(camera_policy);
                let speed = mode_policy.move_speed_mps * surface.speed_multiplier;
                let yaw = angles.map_or(0.0, |angles| angles.yaw);
                drive
                    .map_unchanged(|drive| &mut drive.local_axes)
                    .set_if_neq(Mat2::from_angle(yaw) * movement);
                tuning.set_if_neq(FirstPersonLocomotionTuning {
                    maximum_speed_mps: speed,
                    damping_per_second: surface.damping_per_second,
                    acceleration_mps2: surface.max_acceleration_mps2,
                });
            }
            continue;
        }
        if !matches!(
            (mode.mode, *camera_mode),
            (ImmersiveMode::Photo, CameraMode::Free)
                | (ImmersiveMode::SuperStaff, CameraMode::FirstPerson)
        ) {
            continue;
        }
        let Some(surface) = motion.surface else {
            continue;
        };
        let surface = surface.tuning(camera_policy);
        let (yaw, _, _) = pose.rotation.to_euler(EulerRot::YXZ);
        let desired = (Quat::from_rotation_y(yaw) * Vec3::new(movement.x, 0.0, -movement.y)).xz();
        let damp = (1.0 + surface.damping_per_second * elapsed).max(0.0);
        let limit = mode_policy.move_speed_mps * surface.speed_multiplier;
        let next_velocity = (motion.planar_velocity * damp
            + desired * (surface.max_acceleration_mps2 * elapsed))
            .clamp_length_max(limit.max(0.0));
        let start = pose.translation;
        let target = (start.xz() + next_velocity * elapsed).clamp(bounds.min, bounds.max);
        let delta = Vec3::new(target.x - start.x, 0.0, target.y - start.z);
        let mut travel = delta;
        let mut velocity = next_velocity;
        if let Ok(direction) = Dir3::new(delta) {
            let subject = subject.map(|subject| subject.0);
            let radius = (f32::from(camera_definition.collision_radius_cm) * 0.01).max(0.01);
            let hit = spatial.cast_shape_predicate(
                &Collider::sphere(radius),
                start,
                Quat::IDENTITY,
                direction,
                &ShapeCastConfig {
                    ignore_origin_penetration: true,
                    ..ShapeCastConfig::from_max_distance(delta.length())
                },
                &SpatialQueryFilter::DEFAULT,
                &|entity| {
                    Some(entity) != subject
                        && entity != camera.0
                        && !terrain_chunks.contains(entity)
                        && !parents.iter_ancestors::<ChildOf>(entity).any(|ancestor| {
                            Some(ancestor) == subject || terrain_chunks.contains(ancestor)
                        })
                },
            );
            if let Some(hit) = hit {
                travel = direction.as_vec3() * (hit.distance - 0.001).max(0.0);
                let normal = hit.normal1.xz().normalize_or_zero();
                velocity -= normal * velocity.dot(normal).min(0.0);
                if let Ok((RigidBody::Dynamic, mut body)) = bodies.get_mut(hit.entity) {
                    body.0 += direction.as_vec3() * (camera_policy.push_force * elapsed);
                }
            }
        }
        motion
            .map_unchanged(|motion| &mut motion.planar_velocity)
            .set_if_neq(velocity);
        pose.map_unchanged(|pose| &mut pose.translation)
            .set_if_neq(start + travel);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn fit_walking_camera_to_surfaces(
    time: Res<Time<Real>>,
    modal: Res<AuthoredModalInputCapture>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    surfaces: FirstPersonSurfaces,
    mut sessions: Query<(
        &ActiveImmersiveMode,
        &ImmersiveCamera,
        Option<&ImmersiveSubject>,
        &mut FirstPersonMotion,
    )>,
    mut cameras: Query<
        (
            &mut Transform,
            &WorldMember,
            &CameraDefinition,
            &CameraMode,
            &CameraIntent,
        ),
        (With<ZooCamera>, Without<CameraTransition>),
    >,
    mut staff_avatars: Query<&mut Transform, Without<ZooCamera>>,
    subjects: Query<(&GlobalTransform, Has<FirstPersonLocomotionTuning>)>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let dt = time.delta_secs();
    for (mode, camera, subject, mut motion) in &mut sessions {
        let Ok((mut pose, world, selected, camera_mode, intent)) = cameras.get_mut(camera.0) else {
            continue;
        };
        let attached = subject.and_then(|subject| subjects.get(subject.0).ok());
        let attached_walker = mode.mode == ImmersiveMode::FirstPerson
            && *camera_mode == CameraMode::FirstPerson
            && attached.is_some_and(|(_, walker)| walker);
        if !attached_walker
            && !matches!(
                (mode.mode, *camera_mode),
                (ImmersiveMode::Photo, CameraMode::Free)
                    | (ImmersiveMode::SuperStaff, CameraMode::FirstPerson)
            )
        {
            continue;
        }
        let Some(policy) = definitions
            .find_camera(selected.0)
            .and_then(|definition| definition.first_person_motion.as_ref())
        else {
            continue;
        };
        let Some(mut terrain) = surfaces.at(world.root, pose.translation.xz(), policy.fit_radius_m)
        else {
            continue;
        };
        if let Some((subject, true)) = attached.filter(|_| attached_walker) {
            terrain.height_m = terrain.height_m.max(subject.translation().y);
        }
        let is_photo = mode.mode == ImmersiveMode::Photo;
        let commands_active = !is_photo && modal.0.is_none();
        let inputs = intent.first_person_commands;
        let base_height = terrain.height_m + eye_height(policy, commands_active && inputs.crouch);
        let mut position = pose.translation;
        let mut vertical = motion.vertical_velocity;
        if motion.surface.is_none() {
            position.y = floating_surface(terrain, policy).map_or(base_height, |water| {
                (water + policy.water_offset_m).max(base_height)
            });
        }
        let mut surface = surface_at(terrain, position.y, policy);
        let forward_pitch = pose.forward().y.asin();
        let dive = commands_active
            && policy.can_go_underwater
            && (inputs.down
                || (forward_pitch < -policy.dive_angle_radians && motion.movement.y > 0.0));
        let rise = commands_active && inputs.up;
        if let Some(water) = floating_surface(terrain, policy) {
            let water_eye = water + policy.water_offset_m;
            if dive && surface == Surface::Water {
                position.y = water - policy.underwater_offset_m;
                surface = Surface::Underwater;
            }
            if surface == Surface::Underwater {
                let dynamics = policy.underwater;
                let stroke = f32::from(rise) - f32::from(dive);
                let gravity = if policy.no_underwater_gravity {
                    0.0
                } else {
                    policy.gravity_mps2
                };
                vertical = vertical * (1.0 + dynamics.damping_per_second * dt).max(0.0)
                    + (stroke * dynamics.max_acceleration_mps2 - gravity) * dt;
                position.y = (position.y + vertical * dt)
                    .max(terrain.height_m + policy.underwater_offset_m)
                    .min(water_eye);
                if position.y >= water - policy.underwater_offset_m && rise {
                    position.y = water_eye;
                    vertical = 0.0;
                }
            } else if surface == Surface::Water && !rise {
                position.y = water_eye.max(base_height);
                vertical = 0.0;
            } else {
                (position.y, vertical) = land_vertical_fit(
                    position.y,
                    vertical,
                    base_height.max(water_eye),
                    rise,
                    policy,
                    dt,
                );
            }
        } else {
            (position.y, vertical) =
                land_vertical_fit(position.y, vertical, base_height, rise, policy, dt);
        }
        let surface = surface_at(terrain, position.y, policy);
        motion
            .reborrow()
            .map_unchanged(|motion| &mut motion.vertical_velocity)
            .set_if_neq(vertical);
        motion
            .map_unchanged(|motion| &mut motion.surface)
            .set_if_neq(Some(surface));
        pose.reborrow()
            .map_unchanged(|pose| &mut pose.translation)
            .set_if_neq(position);
        if let Some(mut avatar) = subject
            .filter(|_| mode.mode == ImmersiveMode::SuperStaff)
            .and_then(|subject| staff_avatars.get_mut(subject.0).ok())
        {
            let mut avatar_position = position;
            avatar_position.y -= eye_height(policy, commands_active && inputs.crouch);
            avatar
                .reborrow()
                .map_unchanged(|avatar| &mut avatar.translation)
                .set_if_neq(avatar_position);
        }
    }
}

fn floating_surface(point: TerrainPoint, policy: &FirstPersonCameraMotion) -> Option<f32> {
    point.water_height_m.filter(|water| {
        policy.float_on_water && *water > point.height_m + policy.underwater_offset_m
    })
}

fn surface_at(point: TerrainPoint, eye_y: f32, policy: &FirstPersonCameraMotion) -> Surface {
    match point.water_height_m.filter(|_| policy.float_on_water) {
        Some(water) if policy.can_go_underwater && eye_y < water => Surface::Underwater,
        Some(water) if eye_y <= water + policy.water_offset_m => Surface::Water,
        _ => Surface::Land,
    }
}

pub(super) fn eye_height(policy: &FirstPersonCameraMotion, crouch: bool) -> f32 {
    policy.height_offset_m
        + if crouch {
            policy.crouch_height_m - policy.stand_height_m
        } else {
            0.0
        }
}

fn land_vertical_fit(
    height: f32,
    velocity: f32,
    surface: f32,
    jump: bool,
    policy: &FirstPersonCameraMotion,
    dt: f32,
) -> (f32, f32) {
    let velocity = if height <= surface && velocity <= 0.0 {
        if jump {
            policy.jump_velocity_mps
        } else {
            return (surface, 0.0);
        }
    } else {
        velocity - policy.gravity_mps2 * dt
    };
    let next = (height.max(surface) + velocity * dt).max(surface);
    (next, if next > surface { velocity } else { 0.0 })
}
