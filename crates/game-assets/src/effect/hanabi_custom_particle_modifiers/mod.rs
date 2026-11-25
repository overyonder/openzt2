//! Custom Hanabi initialization and update modifiers required by authored particle effects.

use std::borrow::Cow;

use bevy::prelude::*;
use bevy_hanabi::graph::EvalContext;
use bevy_hanabi::prelude::{
    Attribute, BoxedModifier, ExprError, ExprHandle, Modifier, ModifierContext, Module,
    ShaderWriter,
};
use bevy_hanabi::ToWgslString;

#[derive(Clone)]
pub(super) struct CollisionSpawnParticleInitialization {
    pub(super) spawn_radius: f32,
    pub(super) initial_speed: f32,
    pub(super) outward_velocity: Vec3,
    pub(super) upward_velocity: Vec3,
    pub(super) base_velocity_direction: Vec3,
    pub(super) velocity_scale: Vec3,
    pub(super) position_offset: f32,
}

#[derive(Reflect, Clone, Debug)]
pub(super) struct CollisionSpawnParticleInitializationModifier {
    pub(super) spawn_radius: f32,
    pub(super) initial_speed: f32,
    pub(super) outward_velocity: Vec3,
    pub(super) upward_velocity: Vec3,
    pub(super) base_velocity_direction: Vec3,
    pub(super) velocity_scale: Vec3,
    pub(super) position_offset: f32,
}

impl Modifier for CollisionSpawnParticleInitializationModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Init
    }

    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY])
    }

    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(self.clone())
    }

    fn apply(
        &self,
        _expression_module: &mut Module,
        shader_writer: &mut ShaderWriter,
    ) -> Result<(), ExprError> {
        shader_writer.main_code += &format!(
            "let source_radial = normalize(frand3() * 2.0 - vec3<f32>(1.0)); let collision_radial = vec3<f32>(source_radial.x, source_radial.z, -source_radial.y); particle.position = parent_particle.position + collision_radial * {} + vec3<f32>({},{},{}) * {}; let collision_velocity = vec3<f32>({},{},{}) + vec3<f32>({},{},{}) * source_radial.x + vec3<f32>({},{},{}) * source_radial.y; particle.velocity = collision_velocity * vec3<f32>({},{},{}) * {};\n",
            self.spawn_radius,
            self.base_velocity_direction.x,
            self.base_velocity_direction.y,
            self.base_velocity_direction.z,
            self.position_offset,
            self.base_velocity_direction.x,
            self.base_velocity_direction.y,
            self.base_velocity_direction.z,
            self.outward_velocity.x,
            self.outward_velocity.y,
            self.outward_velocity.z,
            self.upward_velocity.x,
            self.upward_velocity.y,
            self.upward_velocity.z,
            self.velocity_scale.x,
            self.velocity_scale.y,
            self.velocity_scale.z,
            self.initial_speed,
        );
        Ok(())
    }
}

#[derive(Reflect, Clone, Debug)]
pub(super) struct PointGravityParticleUpdateModifier {
    pub(super) gravity_position: Vec3,
    pub(super) gravity_force: f32,
    pub(super) distance_decay: f32,
}

impl Modifier for PointGravityParticleUpdateModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }

    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY])
    }

    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(self.clone())
    }

    fn apply(
        &self,
        _expression_module: &mut Module,
        shader_writer: &mut ShaderWriter,
    ) -> Result<(), ExprError> {
        shader_writer.main_code += &format!(
            "let gravity_offset = vec3<f32>({},{},{}) - particle.position; let gravity_distance = max(length(gravity_offset), 0.0001); particle.velocity += normalize(gravity_offset) * {} / pow(gravity_distance, {}) * effect_delta_time;\n",
            self.gravity_position.x,
            self.gravity_position.y,
            self.gravity_position.z,
            self.gravity_force,
            self.distance_decay,
        );
        Ok(())
    }
}

#[derive(Reflect, Clone, Debug)]
pub(super) struct PlaneCollisionParticleUpdateModifier {
    pub(super) plane_normal: Vec3,
    pub(super) plane_constant: f32,
    pub(super) center: Vec3,
    pub(super) x_axis: Vec3,
    pub(super) y_axis: Vec3,
    pub(super) half_width: f32,
    pub(super) half_height: f32,
    pub(super) velocity_rebound: f32,
    pub(super) kill_after_contact: bool,
}

impl Modifier for PlaneCollisionParticleUpdateModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }

    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY])
    }

    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(self.clone())
    }

    fn apply(
        &self,
        _expression_module: &mut Module,
        shader_writer: &mut ShaderWriter,
    ) -> Result<(), ExprError> {
        let Some(normal) = self.plane_normal.try_normalize() else {
            return Ok(());
        };
        let normal = normal.to_wgsl_string();
        let constant = (self.plane_constant / self.plane_normal.length()).to_wgsl_string();
        let center = self.center.to_wgsl_string();
        let x_axis = self.x_axis.to_wgsl_string();
        let y_axis = self.y_axis.to_wgsl_string();
        let half_width = self.half_width.to_wgsl_string();
        let half_height = self.half_height.to_wgsl_string();
        let bounce = self.velocity_rebound.to_wgsl_string();
        let kill = if self.kill_after_contact {
            "is_alive = false;"
        } else {
            ""
        };
        shader_writer.main_code += &format!(
            r#"
if is_alive {{
    let normal = {normal};
    let previous = position_before_integration;
    let from_distance = dot(previous, normal) + {constant};
    let to_distance = dot(particle.position, normal) + {constant};
    if from_distance >= 0.0 && to_distance < 0.0 {{
        let fraction = from_distance / (from_distance - to_distance);
        let contact = mix(previous, particle.position, fraction);
        let offset = contact - {center};
        if abs(dot(offset, {x_axis})) <= {half_width} && abs(dot(offset, {y_axis})) <= {half_height} {{
            particle.velocity = reflect(particle.velocity, normal) * {bounce};
            particle.position = contact + particle.velocity * effect_delta_time * (1.0 - fraction);
            {kill}
        }}
    }}
}}
"#
        );
        Ok(())
    }
}

#[derive(Reflect, Clone, Debug)]
pub(super) enum AuthoredParticleUpdateModifier {
    AngularDrag {
        coefficient: f32,
    },
    FlutterVelocity {
        acceleration: f32,
    },
    FlutterPosition {
        acceleration: f32,
    },
    Drift {
        velocity: Vec3,
    },
    KillPlane {
        normal: Vec3,
        distance: f32,
        override_plane: ExprHandle,
    },
    SpawnOnPlane {
        normal: Vec3,
        distance: f32,
        override_plane: ExprHandle,
        probability: f32,
        min_count: u32,
        max_count: u32,
        child_index: u32,
    },
}

impl Modifier for AuthoredParticleUpdateModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }

    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(match self {
            Self::AngularDrag { .. } => &[Attribute::F32_0],
            Self::FlutterVelocity { .. } => &[Attribute::VELOCITY],
            Self::FlutterPosition { .. } | Self::Drift { .. } | Self::KillPlane { .. } => {
                &[Attribute::POSITION]
            }
            Self::SpawnOnPlane { .. } => &[Attribute::POSITION, Attribute::VELOCITY],
        })
    }

    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(self.clone())
    }

    fn apply(
        &self,
        expression_module: &mut Module,
        shader_writer: &mut ShaderWriter,
    ) -> Result<(), ExprError> {
        match self {
            Self::AngularDrag { coefficient } => {
                shader_writer.main_code += &format!(
                    "particle.f32_0 *= max(0.0, 1.0 - {coefficient} * effect_delta_time);\n"
                );
            }
            Self::FlutterVelocity { acceleration } => {
                shader_writer.main_code += &format!(
                    "particle.velocity += (frand3() * 2.0 - vec3<f32>(1.0)) * {acceleration} * effect_delta_time;\n"
                );
            }
            Self::FlutterPosition { acceleration } => {
                shader_writer.main_code += &format!(
                    "particle.position += (frand3() * 2.0 - vec3<f32>(1.0)) * {acceleration} * effect_delta_time * effect_delta_time;\n"
                );
            }
            Self::Drift { velocity } => {
                shader_writer.main_code += &format!(
                    "particle.position += vec3<f32>({},{},{}) * effect_delta_time;\n",
                    velocity.x, velocity.y, velocity.z,
                );
            }
            Self::KillPlane {
                normal,
                distance,
                override_plane,
            } => {
                let plane = shader_writer.eval(expression_module, *override_plane)?;
                shader_writer.main_code += &format!(
                    "{{ let plane = select(vec4<f32>({},{},{},{}), {plane}, dot({plane}.xyz, {plane}.xyz) > 0.0); if dot(particle.position, plane.xyz) + plane.w >= 0.0 {{ particle.age = particle.lifetime; }} }}\n",
                    normal.x, normal.y, normal.z, -distance,
                );
            }
            Self::SpawnOnPlane {
                normal,
                distance,
                override_plane,
                probability,
                min_count,
                max_count,
                child_index,
            } => {
                let plane = shader_writer.eval(expression_module, *override_plane)?;
                shader_writer.main_code += &format!(
                    "let plane_{0} = select(vec4<f32>({1},{2},{3},{4}), {plane}, dot({plane}.xyz, {plane}.xyz) > 0.0); let previous_position_{0} = particle.position - particle.velocity * effect_delta_time; if dot(previous_position_{0}, plane_{0}.xyz) + plane_{0}.w < 0.0 && dot(particle.position, plane_{0}.xyz) + plane_{0}.w >= 0.0 && frand() <= {5} {{ let spawn_count_{0} = u32(floor(frand() * f32({6} - {7} + 1u))) + {7}u; append_spawn_events_{0}((*effect_metadata).base_child_index, particle_index, spawn_count_{0}); }}\n",
                    child_index,
                    normal.x,
                    normal.y,
                    normal.z,
                    -distance,
                    probability,
                    max_count,
                    min_count,
                );
                shader_writer.set_emits_gpu_spawn_events(true)?;
            }
        }
        Ok(())
    }
}
