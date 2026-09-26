//! Translation from authored particle-system emitters to Hanabi effect assets.

use bevy::prelude::*;
use bevy_hanabi::prelude::{
    AccelModifier, Attribute, BoxedModifier, ColorOverLifetimeModifier,
    EffectAsset as HanabiEffectAsset, ExprWriter, FlipbookModifier, Gradient, KillSphereModifier,
    LinearDragModifier, ModifierContext, OrientMode, OrientModifier, RenderModifier, ScalarType,
    SetAttributeModifier, SetColorModifier, SetPositionCircleModifier, SetPositionSphereModifier,
    SetSizeModifier, ShapeDimension, SizeOverLifetimeModifier, SpawnerSettings, ValueType,
    VectorType,
};
use openzt2_game_data::particle::authored_particle_system::{
    AuthoredParticleEmitterShape, AuthoredParticleRendererKind, AuthoredParticleSpawnPolicy,
    AuthoredParticleSystemEmitter, AuthoredParticleSystemModifier,
};

use super::hanabi_custom_particle_modifiers::{
    AuthoredParticleUpdateModifier, CollisionSpawnParticleInitialization,
    CollisionSpawnParticleInitializationModifier, PointGravityParticleUpdateModifier,
};

pub(super) fn prepare_authored_particle_system_emitter_as_hanabi_effect_asset(
    authored_particle_system_emitter: &AuthoredParticleSystemEmitter,
    collision_spawn_initialization: Option<CollisionSpawnParticleInitialization>,
) -> HanabiEffectAsset {
    let expression_writer = ExprWriter::new();
    let mut particle_initialization_modifiers: Vec<BoxedModifier> = Vec::new();
    let mut particle_update_modifiers: Vec<BoxedModifier> = Vec::new();
    let mut particle_render_modifiers: Vec<Box<dyn RenderModifier>> = Vec::new();
    match authored_particle_system_emitter.shape {
        AuthoredParticleEmitterShape::Point
        | AuthoredParticleEmitterShape::Decal
        | AuthoredParticleEmitterShape::Proxy
        | AuthoredParticleEmitterShape::Mesh => {
            particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                Attribute::POSITION,
                expression_writer.lit(Vec3::ZERO).expr(),
            )));
        }
        AuthoredParticleEmitterShape::Sphere { radius } => {
            particle_initialization_modifiers.push(Box::new(SetPositionSphereModifier {
                center: expression_writer.lit(Vec3::ZERO).expr(),
                radius: expression_writer.lit(radius).expr(),
                dimension: ShapeDimension::Volume,
            }))
        }
        AuthoredParticleEmitterShape::Circle { radius } => {
            particle_initialization_modifiers.push(Box::new(SetPositionCircleModifier {
                center: expression_writer.lit(Vec3::ZERO).expr(),
                axis: expression_writer.lit(Vec3::Y).expr(),
                radius: expression_writer.lit(radius).expr(),
                dimension: ShapeDimension::Volume,
            }))
        }
        AuthoredParticleEmitterShape::Box { half_extents } => {
            let randomized_box_position = (expression_writer
                .rand(ValueType::Vector(VectorType::VEC3F))
                * expression_writer.lit(2.0)
                - expression_writer.lit(Vec3::ONE))
                * expression_writer.lit(Vec3::from_array(half_extents));
            particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                Attribute::POSITION,
                randomized_box_position.expr(),
            )));
        }
        AuthoredParticleEmitterShape::Cone {
            origin,
            direction,
            up,
            out,
            min_radius,
            max_radius,
        } => {
            let randomized_cone_axis_fraction = expression_writer.rand(ScalarType::Float);
            let randomized_cone_rotation_radians = expression_writer.rand(ScalarType::Float)
                * expression_writer.lit(std::f32::consts::TAU);
            let randomized_cone_radius = expression_writer.lit(min_radius)
                + randomized_cone_axis_fraction.clone()
                    * expression_writer.lit(max_radius - min_radius);
            let randomized_cone_position = expression_writer.lit(Vec3::from_array(origin))
                + expression_writer.lit(Vec3::from_array(direction))
                    * randomized_cone_axis_fraction
                + expression_writer.lit(Vec3::from_array(up))
                    * randomized_cone_rotation_radians.clone().cos()
                    * randomized_cone_radius.clone()
                + expression_writer.lit(Vec3::from_array(out))
                    * randomized_cone_rotation_radians.sin()
                    * randomized_cone_radius;
            particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                Attribute::POSITION,
                randomized_cone_position.expr(),
            )));
        }
    }
    if let Some(collision_spawn_initialization) = collision_spawn_initialization {
        particle_initialization_modifiers.push(Box::new(
            CollisionSpawnParticleInitializationModifier {
                spawn_radius: collision_spawn_initialization.spawn_radius,
                initial_speed: collision_spawn_initialization.initial_speed,
                outward_velocity: collision_spawn_initialization.outward_velocity,
                upward_velocity: collision_spawn_initialization.upward_velocity,
                base_velocity_direction: collision_spawn_initialization.base_velocity_direction,
                velocity_scale: collision_spawn_initialization.velocity_scale,
                position_offset: collision_spawn_initialization.position_offset,
            },
        ));
    }
    let mut collision_spawn_child_index = 0;
    for modifier in &authored_particle_system_emitter.modifiers {
        match modifier {
            AuthoredParticleSystemModifier::InitialPosition { lifetime_seconds } => {
                particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::LIFETIME,
                    expression_writer.lit(*lifetime_seconds).expr(),
                )))
            }
            AuthoredParticleSystemModifier::InitialVelocity { velocity } => {
                particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::VELOCITY,
                    expression_writer.lit(Vec3::from_array(*velocity)).expr(),
                )))
            }
            AuthoredParticleSystemModifier::Acceleration { acceleration } => {
                particle_update_modifiers.push(Box::new(AccelModifier::new(
                    expression_writer
                        .lit(Vec3::from_array(*acceleration))
                        .expr(),
                )))
            }
            AuthoredParticleSystemModifier::Drag { coefficient } => {
                particle_update_modifiers.push(Box::new(LinearDragModifier::new(
                    expression_writer.lit(*coefficient).expr(),
                )))
            }
            AuthoredParticleSystemModifier::ColorOverLifetime { points } => {
                let mut gradient = Gradient::new();
                points
                    .iter()
                    .for_each(|point| gradient.add_key(point.time, Vec4::from_array(point.value)));
                particle_render_modifiers.push(Box::new(ColorOverLifetimeModifier::new(gradient)));
            }
            AuthoredParticleSystemModifier::SizeOverLifetime { points } => {
                let mut gradient = Gradient::new();
                points.iter().for_each(|point| {
                    gradient.add_key(
                        point.time,
                        Vec3::from_array(point.value[..3].try_into().unwrap_or([1.0; 3])),
                    )
                });
                particle_render_modifiers.push(Box::new(SizeOverLifetimeModifier {
                    gradient,
                    screen_space_size: false,
                }));
            }
            AuthoredParticleSystemModifier::KillSphere {
                center,
                radius,
                kill_inside,
            } => particle_update_modifiers.push(Box::new(
                KillSphereModifier::new(
                    expression_writer.lit(Vec3::from_array(*center)).expr(),
                    expression_writer.lit(radius.powi(2)).expr(),
                )
                .with_kill_inside(*kill_inside),
            )),
            AuthoredParticleSystemModifier::OrientToVelocity => particle_render_modifiers
                .push(Box::new(OrientModifier::new(OrientMode::AlongVelocity))),
            AuthoredParticleSystemModifier::BirthRateCurve { .. } => {}
            AuthoredParticleSystemModifier::LifetimeRange {
                min_seconds,
                max_seconds,
            } => {
                let randomized_particle_lifetime = expression_writer.lit(*min_seconds)
                    + expression_writer.rand(ScalarType::Float)
                        * expression_writer.lit(*max_seconds - *min_seconds);
                particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::LIFETIME,
                    randomized_particle_lifetime.expr(),
                )));
            }
            AuthoredParticleSystemModifier::InitialVelocityCone {
                base,
                direction,
                vertical_angle,
                horizontal_angle,
                min_speed,
                max_speed,
                ..
            } => {
                let central_velocity_direction = Vec3::from_array(*direction).normalize_or_zero();
                let horizontal_spread_direction = Vec3::from_array(*base).normalize_or_zero();
                let vertical_spread_direction = central_velocity_direction
                    .cross(horizontal_spread_direction)
                    .normalize_or_zero();
                let randomized_direction_components =
                    expression_writer.rand(ValueType::Vector(VectorType::VEC3F));
                let randomized_angular_spread = expression_writer
                    .lit(horizontal_spread_direction * *horizontal_angle)
                    * randomized_direction_components.clone()
                    + expression_writer.lit(vertical_spread_direction * *vertical_angle)
                        * randomized_direction_components;
                let randomized_initial_speed = expression_writer.lit(*min_speed)
                    + expression_writer.rand(ScalarType::Float)
                        * expression_writer.lit(*max_speed - *min_speed);
                particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::VELOCITY,
                    ((expression_writer.lit(central_velocity_direction)
                        + randomized_angular_spread)
                        .normalized()
                        * randomized_initial_speed)
                        .expr(),
                )));
            }
            AuthoredParticleSystemModifier::InitialColor { rgba } => particle_render_modifiers
                .push(Box::new(SetColorModifier::new(Vec4::from_array(*rgba)))),
            AuthoredParticleSystemModifier::InitialSizeRange { min, max } => {
                particle_render_modifiers.push(Box::new(SetSizeModifier {
                    size: (Vec3::splat(*min), Vec3::splat(*max)).into(),
                }))
            }
            AuthoredParticleSystemModifier::DirectionalGravity {
                direction,
                acceleration,
            } => particle_update_modifiers.push(Box::new(AccelModifier::new(
                expression_writer
                    .lit(Vec3::from_array(*direction).normalize_or_zero() * *acceleration)
                    .expr(),
            ))),
            AuthoredParticleSystemModifier::PointGravity {
                position,
                force,
                decay,
            } => particle_update_modifiers.push(Box::new(PointGravityParticleUpdateModifier {
                gravity_position: Vec3::from_array(*position),
                gravity_force: *force,
                distance_decay: *decay,
            })),
            AuthoredParticleSystemModifier::Spin { min_rate, max_rate } => {
                let randomized_angular_rate = expression_writer.lit(*min_rate)
                    + expression_writer.rand(ScalarType::Float)
                        * expression_writer.lit(*max_rate - *min_rate);
                particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::F32_0,
                    randomized_angular_rate.expr(),
                )));
                particle_render_modifiers.push(Box::new(
                    OrientModifier::new(OrientMode::ParallelCameraDepthPlane).with_rotation(
                        (expression_writer.attr(Attribute::F32_0)
                            * expression_writer.attr(Attribute::AGE))
                        .expr(),
                    ),
                ));
            }
            AuthoredParticleSystemModifier::AngularDrag { coefficient } => {
                particle_update_modifiers.push(Box::new(
                    AuthoredParticleUpdateModifier::AngularDrag {
                        coefficient: *coefficient,
                    },
                ))
            }
            AuthoredParticleSystemModifier::FlutterVelocity { acceleration } => {
                particle_update_modifiers.push(Box::new(
                    AuthoredParticleUpdateModifier::FlutterVelocity {
                        acceleration: *acceleration,
                    },
                ))
            }
            AuthoredParticleSystemModifier::FlutterPosition { acceleration } => {
                particle_update_modifiers.push(Box::new(
                    AuthoredParticleUpdateModifier::FlutterPosition {
                        acceleration: *acceleration,
                    },
                ))
            }
            AuthoredParticleSystemModifier::Drift { velocity } => {
                particle_update_modifiers.push(Box::new(AuthoredParticleUpdateModifier::Drift {
                    velocity: Vec3::from_array(*velocity),
                }))
            }
            AuthoredParticleSystemModifier::KillPlane { normal, distance } => {
                particle_update_modifiers.push(Box::new(
                    AuthoredParticleUpdateModifier::KillPlane {
                        normal: Vec3::from_array(*normal),
                        distance: *distance,
                    },
                ))
            }
            AuthoredParticleSystemModifier::SpawnOnPlane {
                normal,
                distance,
                probability,
                min_count,
                max_count,
                ..
            } => {
                particle_update_modifiers.push(Box::new(
                    AuthoredParticleUpdateModifier::SpawnOnPlane {
                        normal: Vec3::from_array(*normal),
                        distance: *distance,
                        probability: *probability,
                        min_count: *min_count,
                        max_count: *max_count,
                        child_index: collision_spawn_child_index,
                    },
                ));
                collision_spawn_child_index += 1;
            }
            AuthoredParticleSystemModifier::TileRandomizer { columns, rows } => {
                let sprite_sheet_cell_count = u32::from(*columns) * u32::from(*rows);
                particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::SPRITE_INDEX,
                    (expression_writer.rand(ScalarType::Float)
                        * expression_writer.lit(sprite_sheet_cell_count as f32))
                    .floor()
                    .cast(ScalarType::Uint)
                    .expr(),
                )));
                particle_render_modifiers.push(Box::new(FlipbookModifier {
                    sprite_grid_size: UVec2::new(u32::from(*columns), u32::from(*rows)),
                }));
            }
            AuthoredParticleSystemModifier::DecalUv { minimum } => {
                particle_initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::SPRITE_INDEX,
                    (expression_writer.rand(ScalarType::Float)
                        * expression_writer.lit((1.0 - *minimum).max(f32::EPSILON)))
                    .floor()
                    .cast(ScalarType::Uint)
                    .expr(),
                )))
            }
            AuthoredParticleSystemModifier::Renderer {
                kind,
                width,
                height,
                columns,
                rows,
                ..
            } => {
                particle_render_modifiers.push(Box::new(SetSizeModifier {
                    size: Vec3::new(*width, *height, 1.0).into(),
                }));
                if *columns > 1 || *rows > 1 {
                    particle_render_modifiers.push(Box::new(FlipbookModifier {
                        sprite_grid_size: UVec2::new(u32::from(*columns), u32::from(*rows)),
                    }));
                }
                match kind {
                    AuthoredParticleRendererKind::Billboard => particle_render_modifiers.push(
                        Box::new(OrientModifier::new(OrientMode::ParallelCameraDepthPlane)),
                    ),
                    AuthoredParticleRendererKind::VelocityAligned => particle_render_modifiers
                        .push(Box::new(OrientModifier::new(OrientMode::AlongVelocity))),
                    AuthoredParticleRendererKind::Decal | AuthoredParticleRendererKind::Mesh => {}
                }
            }
            AuthoredParticleSystemModifier::EmitWindow { .. }
            | AuthoredParticleSystemModifier::DieWhenEmpty { .. }
            | AuthoredParticleSystemModifier::Overflow { .. } => {}
        }
    }
    let authored_spawner_settings = match authored_particle_system_emitter.spawn {
        AuthoredParticleSpawnPolicy::Rate { per_second } => {
            SpawnerSettings::rate(per_second.into())
        }
        AuthoredParticleSpawnPolicy::Burst { count } => {
            SpawnerSettings::once((count as f32).into())
        }
        AuthoredParticleSpawnPolicy::Once => SpawnerSettings::once(1.0_f32.into()),
        AuthoredParticleSpawnPolicy::Curve { .. } | AuthoredParticleSpawnPolicy::Manual => {
            SpawnerSettings::once(1.0_f32.into()).with_starts_active(false)
        }
    };
    let expression_module = expression_writer.finish();
    let mut hanabi_effect_asset = HanabiEffectAsset::new(
        authored_particle_system_emitter.capacity.max(1),
        authored_spawner_settings,
        expression_module,
    )
    .with_name(format!(
        "psys-emitter-{}",
        authored_particle_system_emitter
            .emitter_id
            .to_lowercase_hexadecimal_string()
    ));
    for modifier in particle_initialization_modifiers {
        hanabi_effect_asset = hanabi_effect_asset.add_modifier(ModifierContext::Init, modifier);
    }
    for modifier in particle_update_modifiers {
        hanabi_effect_asset = hanabi_effect_asset.add_modifier(ModifierContext::Update, modifier);
    }
    for modifier in particle_render_modifiers {
        hanabi_effect_asset = hanabi_effect_asset.add_render_modifier(modifier);
    }
    hanabi_effect_asset
}
