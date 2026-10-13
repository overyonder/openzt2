//! Bevy material-pipeline specialization for evaluated D3D9 effect passes.

use std::borrow::Cow;

use bevy::{
    material::AlphaMode,
    pbr::{Material, MaterialPipeline, MaterialPipelineKey},
    render::render_resource::{RenderPipelineDescriptor, SpecializedMeshPipelineError},
};

use super::{
    d3d9_shader_and_vertex_layout_mapping::map_d3d9_shader_input_to_bevy_vertex_attribute,
    effect_pass_gpu_data::{EffectPassMaterial, FIXED_FUNCTION_SHADER},
};

impl Material for EffectPassMaterial {
    fn alpha_mode(&self) -> AlphaMode {
        // All passes need stable authored ordering in Bevy's sorted phase.
        // specialize() still installs the exact D3D9 blend and depth states.
        AlphaMode::Blend
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn vertex_shader() -> bevy::shader::ShaderRef {
        FIXED_FUNCTION_SHADER.into()
    }
    fn fragment_shader() -> bevy::shader::ShaderRef {
        FIXED_FUNCTION_SHADER.into()
    }
    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.shader = key.bind_group_data.vertex_shader;
        descriptor.vertex.entry_point = Some(Cow::Borrowed(key.bind_group_data.vertex_entry_point));
        if !key.bind_group_data.vertex_inputs.is_empty() {
            let attributes = key
                .bind_group_data
                .vertex_inputs
                .iter()
                .filter_map(map_d3d9_shader_input_to_bevy_vertex_attribute)
                .map(|(attribute, location)| attribute.at_shader_location(location))
                .collect::<Vec<_>>();
            descriptor.vertex.buffers = vec![layout.0.get_layout(&attributes)?];
        }
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = key.bind_group_data.fragment_shader;
            fragment.entry_point = Some(Cow::Borrowed(key.bind_group_data.fragment_entry_point));
            fragment.targets.iter_mut().flatten().for_each(|target| {
                target.blend = key.bind_group_data.blend_state;
                target.write_mask = key.bind_group_data.color_writes;
            });
        }
        descriptor.primitive.cull_mode = key.bind_group_data.cull_face;
        descriptor.primitive.polygon_mode = key.bind_group_data.polygon_mode;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(key.bind_group_data.depth_write_enabled);
            depth.depth_compare = Some(key.bind_group_data.depth_compare);
        }
        Ok(())
    }
}
