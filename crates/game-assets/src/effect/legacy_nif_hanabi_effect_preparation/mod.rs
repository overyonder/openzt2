//! Converts legacy NIF particle records to a Hanabi effect asset.

use std::borrow::Cow;

use bevy::prelude::*;
use bevy_hanabi::prelude::{
    AccelModifier, Attribute, BoxedModifier, ColorOverLifetimeModifier,
    EffectAsset as HanabiEffectAsset, ExprError, ExprWriter, Gradient, Modifier, ModifierContext,
    Module, RenderContext, RenderModifier, ScalarType, SetAttributeModifier, SetColorModifier,
    ShaderWriter, SpawnerSettings, ValueType, VectorType,
};
use bevy_hanabi::ToWgslString;
use openzt2_game_data::particle::legacy_nif_particle_effect::{
    LegacyNifParticleEffect, LegacyNifParticleModifier, LegacyNifParticleSpawnPolicy,
};

use super::hanabi_custom_particle_modifiers::{
    PlaneCollisionParticleUpdateModifier, PointGravityParticleUpdateModifier,
};

pub(super) fn prepare_legacy_nif_particle_effect_as_hanabi_effect_asset(
    legacy_nif_particle_effect: &LegacyNifParticleEffect,
    inherits_parent: bool,
    spawns_children: bool,
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
        Box::new(SetAttributeModifier::new(
            Attribute::SIZE,
            expression_writer
                .lit(legacy_nif_particle_effect.initial_size)
                .expr(),
        )),
    ];
    let mut update_modifiers: Vec<BoxedModifier> = Vec::new();
    if inherits_parent {
        initialization_modifiers.push(Box::new(LegacyGenerationModifier::Inherit(
            legacy_nif_particle_effect.spawn_policy,
        )));
    }
    if spawns_children {
        update_modifiers.push(Box::new(LegacyGenerationModifier::BeginStep));
    }
    let mut render_modifiers: Vec<Box<dyn RenderModifier>> = vec![Box::new(SetColorModifier::new(
        Vec4::from_array(legacy_nif_particle_effect.initial_color),
    ))];

    for modifier in &legacy_nif_particle_effect.modifiers {
        match modifier {
            LegacyNifParticleModifier::Rotation {
                random_initial_axis,
                initial_axis,
                radians_per_second,
            } => {
                let axis = if *random_initial_axis {
                    let height = expression_writer.rand(ScalarType::Float)
                        * expression_writer.lit(2.0)
                        - expression_writer.lit(1.0);
                    let azimuth = expression_writer.rand(ScalarType::Float)
                        * expression_writer.lit(std::f32::consts::TAU);
                    let radius =
                        (expression_writer.lit(1.0) - height.clone() * height.clone()).sqrt();
                    (radius.clone() * azimuth.clone().cos()).vec3(radius * azimuth.sin(), height)
                } else {
                    let axis = Vec3::from_array(*initial_axis)
                        .try_normalize()
                        .unwrap_or(Vec3::Z);
                    expression_writer.lit(axis)
                };
                initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::AXIS_Z,
                    axis.expr(),
                )));
                render_modifiers.push(Box::new(ParticleAxisRotation {
                    radians_per_second: *radians_per_second,
                    billboard: !legacy_nif_particle_effect.mesh_particles,
                }));
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
                let zero = expression_writer.lit(0.0);
                let one = expression_writer.lit(1.0);
                let age = expression_writer.attr(Attribute::AGE);
                let grow = if *grow_seconds > 0.0 {
                    (age.clone() / expression_writer.lit(*grow_seconds))
                        .clamp(zero.clone(), one.clone())
                } else {
                    one.clone()
                };
                let fade = if *fade_seconds > 0.0 {
                    ((expression_writer.attr(Attribute::LIFETIME) - age)
                        / expression_writer.lit(*fade_seconds))
                    .clamp(zero, one)
                } else {
                    one
                };
                let initial_size = if *grow_seconds > 0.0 {
                    0.0
                } else {
                    legacy_nif_particle_effect.initial_size
                };
                initialization_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::SIZE,
                    expression_writer.lit(initial_size).expr(),
                )));
                update_modifiers.push(Box::new(SetAttributeModifier::new(
                    Attribute::SIZE,
                    (expression_writer.lit(legacy_nif_particle_effect.initial_size)
                        * grow.min(fade))
                    .expr(),
                )));
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
                spawn_on_collide,
                die_on_collide,
                plane_normal,
                plane_constant,
                position,
                x_axis,
                y_axis,
                width,
                height,
                ..
            } => update_modifiers.push(Box::new(PlaneCollisionParticleUpdateModifier {
                plane_normal: Vec3::from_array(*plane_normal),
                plane_constant: *plane_constant,
                center: Vec3::from_array(*position),
                x_axis: Vec3::from_array(*x_axis).normalize_or_zero(),
                y_axis: Vec3::from_array(*y_axis).normalize_or_zero(),
                half_width: *width * 0.5,
                half_height: *height * 0.5,
                velocity_rebound: *bounce,
                kill_after_contact: *die_on_collide,
                request_spawn_after_contact: spawns_children && *spawn_on_collide,
            })),
            LegacyNifParticleModifier::MeshSelection { .. } => {}
        }
    }

    if spawns_children {
        update_modifiers.push(Box::new(LegacyGenerationModifier::Spawn(
            legacy_nif_particle_effect.spawn_policy,
        )));
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

#[derive(Clone, Debug, Reflect)]
enum LegacyGenerationModifier {
    Inherit(#[reflect(ignore)] LegacyNifParticleSpawnPolicy),
    BeginStep,
    Spawn(#[reflect(ignore)] LegacyNifParticleSpawnPolicy),
}

impl Modifier for LegacyGenerationModifier {
    fn context(&self) -> ModifierContext {
        match self {
            Self::Inherit(_) => ModifierContext::Init,
            _ => ModifierContext::Update,
        }
    }

    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[
            Attribute::POSITION,
            Attribute::VELOCITY,
            Attribute::AGE,
            Attribute::LIFETIME,
        ])
    }

    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(self.clone())
    }

    fn apply(&self, _: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        match self {
            Self::BeginStep => context
                .main_code
                .push_str("var legacy_spawn_triggers = 0u;\n"),
            Self::Inherit(policy) => {
                let direction_chaos = policy.direction_chaos.to_wgsl_string();
                let speed_chaos = policy.speed_chaos.to_wgsl_string();
                context.main_code += &format!(
                    r#"
particle.position = parent_particle.position;
let parent_speed = length(parent_particle.velocity);
let heading = select(vec3<f32>(0.0, 1.0, 0.0), parent_particle.velocity / max(parent_speed, 0.000001), parent_speed > 0.0);
let spread = (frand3() * 2.0 - vec3<f32>(1.0)) * {direction_chaos};
let speed = parent_speed * max(0.0, 1.0 + (frand() * 2.0 - 1.0) * {speed_chaos});
let scattered = heading + spread;
particle.velocity = scattered * (speed / max(length(scattered), 0.000001));
"#
                );
            }
            Self::Spawn(policy) => {
                let probability = policy.probability.to_wgsl_string();
                let multiplier = policy.multiplier;
                if policy.on_death {
                    context
                        .main_code
                        .push_str("if was_alive && !is_alive { legacy_spawn_triggers += 1u; }\n");
                }
                context.main_code += &format!(
                    r#"
for (var trigger_index = 0u; trigger_index < legacy_spawn_triggers; trigger_index += 1u) {{
    if frand() < {probability} {{
        append_spawn_events_0((*effect_metadata).base_child_index, particle_index, {multiplier}u);
    }}
}}
"#
                );
                context.set_emits_gpu_spawn_events(true)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Reflect)]
struct ParticleAxisRotation {
    radians_per_second: f32,
    billboard: bool,
}

impl Modifier for ParticleAxisRotation {
    fn context(&self) -> ModifierContext {
        ModifierContext::Render
    }

    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::AGE, Attribute::AXIS_Z])
    }

    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(self.clone())
    }

    fn apply(&self, _: &mut Module, _: &mut ShaderWriter) -> Result<(), ExprError> {
        Err(ExprError::TypeError("Wrong modifier context".to_owned()))
    }
}

impl RenderModifier for ParticleAxisRotation {
    fn apply_render(&self, _: &mut Module, context: &mut RenderContext) -> Result<(), ExprError> {
        context.vertex_code += "{\n";
        if self.billboard {
            context.vertex_code += "let base = get_camera_rotation_effect_space();\n";
        } else {
            context.vertex_code += "let base = mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));\n";
        }
        context.vertex_code += &format!(
            "let angle = particle.age * {};\n",
            self.radians_per_second.to_wgsl_string()
        );
        context.vertex_code += r#"
let axis = particle.axis_z;
let c = cos(angle);
let s = sin(angle);
let skew = mat3x3<f32>(vec3<f32>(0.0, axis.z, -axis.y), vec3<f32>(-axis.z, 0.0, axis.x), vec3<f32>(axis.y, -axis.x, 0.0));
let projection = mat3x3<f32>(axis * axis.x, axis * axis.y, axis * axis.z);
let identity = mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
let rotated = base * (identity * c + skew * s + projection * (1.0 - c));
axis_x = rotated[0];
axis_y = rotated[1];
axis_z = rotated[2];
}
"#;
        Ok(())
    }

    fn boxed_render_clone(&self) -> Box<dyn RenderModifier> {
        Box::new(self.clone())
    }

    fn as_modifier(&self) -> &dyn Modifier {
        self
    }
}
