use bevy::prelude::*;
use bevy_hanabi::prelude::{
    Attribute, BoxedModifier, ExprError, Modifier, ModifierContext, Module, RenderContext,
    RenderModifier, ShaderWriter,
};
use bevy_hanabi::EffectProperties;

#[derive(Reflect, Clone, Debug)]
pub(super) struct WaveSurfaceRenderModifier {
    pub minimum_uv: f32,
}

impl Modifier for WaveSurfaceRenderModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Render
    }
    fn attributes(&self) -> &[Attribute] {
        &[Attribute::AGE, Attribute::LIFETIME, Attribute::F32_0]
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(self.clone())
    }
    fn apply(&self, _: &mut Module, _: &mut ShaderWriter) -> Result<(), ExprError> {
        Ok(())
    }
    fn as_render(&self) -> Option<&dyn RenderModifier> {
        Some(self)
    }
    fn as_render_mut(&mut self) -> Option<&mut dyn RenderModifier> {
        Some(self)
    }
    fn into_boxed_render(self: Box<Self>) -> Option<Box<dyn RenderModifier>> {
        Some(self)
    }
}

impl RenderModifier for WaveSurfaceRenderModifier {
    fn apply_render(&self, _: &mut Module, context: &mut RenderContext) -> Result<(), ExprError> {
        context.set_needs_uv();
        context.set_needs_particle_fragment();
        let fog_varying = context.add_vec4_varying();
        context.vertex_code += r#"
let wave = properties[properties_array_index];
let delta_x = vec4<f32>(vertex_position.x) - wave.wave_pos_x;
let delta_y = vec4<f32>(-vertex_position.z) - wave.wave_pos_y;
let inverse_distance = inverseSqrt(delta_x * delta_x + delta_y * delta_y);
let distance = vec4<f32>(1.0) / inverse_distance;
let dir_x = delta_x * inverse_distance;
let dir_y = delta_y * inverse_distance;
let angle = fract((distance * wave.wave_frequency - wave.wave_phase + vec4<f32>(3.14159265)) / 6.2831853) * 6.2831853 - vec4<f32>(3.14159265);
let angle2 = angle * angle;
let angle3 = angle2 * angle;
let angle4 = angle2 * angle2;
let angle5 = angle3 * angle2;
let angle6 = angle3 * angle3;
let angle7 = angle4 * angle3;
let amplitude = wave.wave_amplitude * (vec4<f32>(1.0) - clamp((distance - wave.wave_ramp_offset) * wave.wave_ramp_scale, vec4<f32>(0.0), vec4<f32>(1.0)));
let sines = (angle - angle3 / 6.0 + angle5 / 120.0 - angle7 / 5040.0) * amplitude;
let cosines = (vec4<f32>(1.0) - angle2 / 2.0 + angle4 / 24.0 - angle6 / 720.0) * amplitude * wave.wave_frequency;
position = vec3<f32>(vertex_position.x + dot(cosines, dir_x * wave.wave_k) * vertex_position.y, dot(sines, vec4<f32>(1.0)) + wave.wave_depth_offset.w, vertex_position.z - dot(cosines, dir_y * wave.wave_k) * vertex_position.y);
size = vec3<f32>(0.0);
"#;
        context.vertex_code += &format!(
            "out.{fog_varying} = vec4<f32>(wave.wave_fog_colour.rgb, clamp((wave.wave_fog_range.z - (dot(position, wave.wave_fog_plane.xyz) - wave.wave_fog_plane.w)) * wave.wave_fog_range.y, 0.0, 1.0));\n"
        );
        context.fragment_code += &format!(
            r#"
let normalized_age = particle.age / particle.lifetime;
let uv_scale = 1.0 / ({minimum} + (1.0 - {minimum}) * normalized_age);
let uv_angle = particle.f32_0 * particle.age;
let rotated_uv = vec2<f32>(cos(uv_angle) * uv.x - sin(uv_angle) * uv.y, sin(uv_angle) * uv.x + cos(uv_angle) * uv.y);
let ripple = textureSample(material_texture_0, material_sampler_0, -uv * uv_scale * 0.5 + vec2<f32>(0.5));
let mask = textureSample(material_texture_1, material_sampler_1, rotated_uv * 0.5 + vec2<f32>(0.5));
color = vec4<f32>(vec3<f32>(1.0) - (vec3<f32>(1.0) - ripple.rgb) * (vec3<f32>(1.0) - color.rgb), ripple.a * color.a * mask.a);
if color.a <= 0.0 {{ discard; }}
color = vec4<f32>(mix(in.{fog_varying}.rgb, color.rgb, in.{fog_varying}.a), color.a);
"#,
            minimum = self.minimum_uv
        );
        Ok(())
    }
    fn boxed_render_clone(&self) -> Box<dyn RenderModifier> {
        Box::new(self.clone())
    }
    fn as_modifier(&self) -> &dyn Modifier {
        self
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ParticleEffectWaveSurfaceFog {
    registers: [Vec4; 2],
    colour: Vec4,
}

impl ParticleEffectWaveSurfaceFog {
    pub fn from_camera(
        transform: &GlobalTransform,
        projection: &Projection,
        fog: &bevy::pbr::DistanceFog,
    ) -> Option<Self> {
        let view = crate::material::runtime::resolved_effect_transform_bindings::EffectRenderViewTransforms::from_camera(transform, projection);
        let registers = view.fog_shader_registers(fog, projection)?;
        let colour = fog.color.to_srgba();
        Some(Self {
            registers,
            colour: Vec4::new(colour.red, colour.green, colour.blue, colour.alpha),
        })
    }
    fn uniforms(self) -> [(&'static str, Vec4); 3] {
        [
            ("wave_fog_range", self.registers[0]),
            ("wave_fog_plane", self.registers[1]),
            ("wave_fog_colour", self.colour),
        ]
    }
}

/// Live wave uniforms for an absolute terrain mesh rendered by a particle.
/// Mesh positions are `(source_x, displacement_weight, -source_y)`;
/// UV0 retains the unscaled, centred authored projection.
#[derive(Component, Clone, Copy, Debug)]
pub struct ParticleEffectWaveSurface {
    pub frequency: Vec4,
    pub phase: Vec4,
    pub amplitude: Vec4,
    pub pos_x: Vec4,
    pub pos_y: Vec4,
    pub k: Vec4,
    pub ramp_offset: Vec4,
    pub ramp_scale: Vec4,
    pub depth_offset: Vec4,
    pub depth_scale: Vec4,
}

impl ParticleEffectWaveSurface {
    pub(super) fn uniforms(self) -> [(&'static str, Vec4); 10] {
        [
            ("wave_frequency", self.frequency),
            ("wave_phase", self.phase),
            ("wave_amplitude", self.amplitude),
            ("wave_pos_x", self.pos_x),
            ("wave_pos_y", self.pos_y),
            ("wave_k", self.k),
            ("wave_ramp_offset", self.ramp_offset),
            ("wave_ramp_scale", self.ramp_scale),
            ("wave_depth_offset", self.depth_offset),
            ("wave_depth_scale", self.depth_scale),
        ]
    }
}

pub(crate) fn upload_particle_wave_surface_uniforms(
    mut commands: Commands,
    mut emitters: Query<(
        Entity,
        &ParticleEffectWaveSurface,
        Option<&ParticleEffectWaveSurfaceFog>,
        Option<&mut EffectProperties>,
    )>,
) {
    for (entity, wave, fog, properties) in &mut emitters {
        let Some(fog) = fog else {
            commands.entity(entity).insert(Visibility::Hidden);
            continue;
        };
        commands.entity(entity).insert(Visibility::Inherited);
        if let Some(mut properties) = properties {
            for (name, value) in wave.uniforms().into_iter().chain(fog.uniforms()) {
                properties = EffectProperties::set_if_changed(properties, name, value.into());
            }
        } else {
            let mut properties = EffectProperties::default();
            for (name, value) in wave.uniforms().into_iter().chain(fog.uniforms()) {
                properties.set(name, value.into());
            }
            commands.entity(entity).insert(properties);
        }
    }
}
