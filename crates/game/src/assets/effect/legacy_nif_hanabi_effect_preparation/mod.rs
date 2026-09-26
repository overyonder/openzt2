//! Converts legacy NIF particle records to a Hanabi effect asset.

use bevy::prelude::*;
use bevy_hanabi::prelude::{
    AccelModifier, Attribute, BoxedModifier, ColorOverLifetimeModifier,
    EffectAsset as HanabiEffectAsset, ExprWriter, Gradient, ModifierContext, OrientMode,
    OrientModifier, RenderModifier, ScalarType, SetAttributeModifier, SetColorModifier,
    SetSizeModifier, SizeOverLifetimeModifier, SpawnerSettings, ValueType, VectorType,
};
use openzt2_game_data::particle::legacy_nif_particle_effect::{
    LegacyNifParticleEffect, LegacyNifParticleModifier,
};

use super::hanabi_custom_particle_modifiers::{
    PlaneCollisionParticleUpdateModifier, PointGravityParticleUpdateModifier,
};

pub(super) fn prepare_legacy_nif_particle_effect_as_hanabi_effect_asset(
    legacy_nif_particle_effect: &LegacyNifParticleEffect,
) -> HanabiEffectAsset {
    let expression_writer = ExprWriter::new();
    let randomized_initial_position =
        (expression_writer.rand(ValueType::Vector(VectorType::VEC3F)) * expression_writer.lit(2.0)
            - expression_writer.lit(Vec3::ONE))
            * expression_writer.lit(Vec3::from_array(legacy_nif_particle_effect.start_random));
    let randomized_particle_lifetime = expression_writer
        .lit(legacy_nif_particle_effect.lifetime_seconds[0])
        + expression_writer.rand(ScalarType::Float)
            * expression_writer.lit(
                legacy_nif_particle_effect.lifetime_seconds[1]
                    - legacy_nif_particle_effect.lifetime_seconds[0],
            );
    let horizontal_direction_radians = legacy_nif_particle_effect.horizontal_direction;
    let vertical_direction_radians = legacy_nif_particle_effect.vertical_direction;
    let central_velocity_direction = Vec3::new(
        horizontal_direction_radians.sin() * vertical_direction_radians.cos(),
        vertical_direction_radians.sin(),
        horizontal_direction_radians.cos() * vertical_direction_radians.cos(),
    );
    let randomized_angular_spread = expression_writer.rand(ValueType::Vector(VectorType::VEC3F))
        - expression_writer.lit(Vec3::splat(0.5));
    let randomized_velocity_direction = (expression_writer.lit(central_velocity_direction)
        + randomized_angular_spread
            * expression_writer.lit(Vec3::new(
                legacy_nif_particle_effect.horizontal_angle,
                legacy_nif_particle_effect.vertical_angle,
                legacy_nif_particle_effect.horizontal_angle,
            )))
    .normalized();
    let randomized_initial_speed = expression_writer.lit(legacy_nif_particle_effect.speed[0])
        + expression_writer.rand(ScalarType::Float)
            * expression_writer
                .lit(legacy_nif_particle_effect.speed[1] - legacy_nif_particle_effect.speed[0]);
    let mut initialization_modifiers: Vec<BoxedModifier> = vec![
        Box::new(SetAttributeModifier::new(
            Attribute::POSITION,
            randomized_initial_position.expr(),
        )),
        Box::new(SetAttributeModifier::new(
            Attribute::LIFETIME,
            randomized_particle_lifetime.expr(),
        )),
        Box::new(SetAttributeModifier::new(
            Attribute::VELOCITY,
            (randomized_velocity_direction * randomized_initial_speed).expr(),
        )),
    ];
    let mut update_modifiers: Vec<BoxedModifier> = Vec::new();
    let mut render_modifiers: Vec<Box<dyn RenderModifier>> = vec![
        Box::new(SetColorModifier::new(Vec4::from_array(
            legacy_nif_particle_effect.initial_color,
        ))),
        Box::new(SetSizeModifier {
            size: Vec3::splat(legacy_nif_particle_effect.initial_size).into(),
        }),
    ];

    for modifier in &legacy_nif_particle_effect.modifiers {
        match modifier {
            LegacyNifParticleModifier::Rotation {
                radians_per_second, ..
            } => {
                initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::F32_0,
                    expression_writer.lit(*radians_per_second).expr(),
                )));
                render_modifiers.push(Box::new(
                    OrientModifier::new(OrientMode::ParallelCameraDepthPlane).with_rotation(
                        (expression_writer.attr(Attribute::F32_0)
                            * expression_writer.attr(Attribute::AGE))
                        .expr(),
                    ),
                ));
            }
            LegacyNifParticleModifier::ColorOverLifetime { keys, .. } => {
                let mut gradient = Gradient::new();
                keys.iter().for_each(|key| {
                    gradient.add_key(key.time, Vec4::from_array(key.value));
                });
                render_modifiers.push(Box::new(ColorOverLifetimeModifier::new(gradient)));
            }
            LegacyNifParticleModifier::GrowFade {
                grow_seconds,
                fade_seconds,
            } => {
                let randomized_particle_lifetime =
                    legacy_nif_particle_effect.lifetime_seconds[1].max(f32::EPSILON);
                let mut gradient = Gradient::new();
                gradient.add_key(0.0, Vec3::ZERO);
                gradient.add_key(
                    (grow_seconds / randomized_particle_lifetime).clamp(0.0, 1.0),
                    Vec3::ONE,
                );
                gradient.add_key(
                    (1.0 - fade_seconds / randomized_particle_lifetime).clamp(0.0, 1.0),
                    Vec3::ONE,
                );
                gradient.add_key(1.0, Vec3::ZERO);
                render_modifiers.push(Box::new(SizeOverLifetimeModifier {
                    gradient,
                    screen_space_size: false,
                }));
            }
            LegacyNifParticleModifier::DirectionalGravity {
                direction: gravity_direction,
                acceleration,
            } => update_modifiers.push(Box::new(AccelModifier::new(
                expression_writer
                    .lit(Vec3::from_array(*gravity_direction).normalize_or_zero() * *acceleration)
                    .expr(),
            ))),
            LegacyNifParticleModifier::PointGravity {
                position,
                force,
                decay,
            } => update_modifiers.push(Box::new(PointGravityParticleUpdateModifier {
                gravity_position: Vec3::from_array(*position),
                gravity_force: *force,
                distance_decay: *decay,
            })),
            LegacyNifParticleModifier::PlanarCollision {
                bounce,
                die_on_collide,
                plane_normal,
                plane_constant,
                ..
            } => update_modifiers.push(Box::new(PlaneCollisionParticleUpdateModifier {
                plane_normal: Vec3::from_array(*plane_normal).normalize_or_zero(),
                plane_constant: *plane_constant,
                velocity_rebound: *bounce,
                kill_after_contact: *die_on_collide,
            })),
            LegacyNifParticleModifier::MeshSelection { .. } => {}
        }
    }

    let expression_module = expression_writer.finish();
    let mut hanabi_effect_asset = HanabiEffectAsset::new(
        legacy_nif_particle_effect.capacity.max(1),
        SpawnerSettings::rate(legacy_nif_particle_effect.emit_rate.max(0.0).into()),
        expression_module,
    )
    .with_name(format!(
        "particle-block-{}",
        legacy_nif_particle_effect.source_block
    ));
    for modifier in initialization_modifiers {
        hanabi_effect_asset = hanabi_effect_asset.add_modifier(ModifierContext::Init, modifier);
    }
    for modifier in update_modifiers {
        hanabi_effect_asset = hanabi_effect_asset.add_modifier(ModifierContext::Update, modifier);
    }
    for modifier in render_modifiers {
        hanabi_effect_asset = hanabi_effect_asset.add_render_modifier(modifier);
    }
    hanabi_effect_asset
}
