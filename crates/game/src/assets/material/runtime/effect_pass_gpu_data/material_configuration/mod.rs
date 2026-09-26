//! Texture, terrain-decal and world-light configuration.

use super::{
    uniform_buffer_upload::shader_buffer_from_uniform_value, EffectPassMaterial,
    FIXED_FUNCTION_SHADER,
};
use bevy::{
    prelude::*,
    render::{
        render_resource::{
            BlendComponent, BlendFactor, BlendOperation, BlendState, ColorWrites, CompareFunction,
            Face,
        },
        storage::ShaderBuffer,
    },
};

impl EffectPassMaterial {
    pub(crate) fn configure_for_authored_blue_fang_line(&mut self, texture: Handle<Image>) -> bool {
        if self.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER
            || !self.replace_texture_asset_at_d3d9_stage(0, texture)
        {
            return false;
        }

        self.fixed_function.values[0] = Vec4::new(4.0, 2.0, 1.0, 4.0);
        self.fixed_function.values[1] = Vec4::new(2.0, 1.0, 0.0, 0.0);
        self.fixed_function.values[5].x = 1.0;
        self.fixed_function.values[46].x = 0.0;
        self.pipeline_specialization_key.cull_face = None;
        self.pipeline_specialization_key.depth_write_enabled = true;
        self.pipeline_specialization_key.depth_compare = CompareFunction::LessEqual;
        self.pipeline_specialization_key.blend_state = None;
        self.pipeline_specialization_key.color_writes = ColorWrites::ALL;
        true
    }

    pub(crate) fn configure_for_authored_single_texture_terrain_decal(
        &mut self,
        texture: Handle<Image>,
    ) -> bool {
        if self.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER
            || !self.replace_texture_asset_at_d3d9_stage(0, texture)
        {
            return false;
        }
        self.fixed_function.values[5].x = 1.0; // D3DTOP_DISABLE for stage one.
        self.pipeline_specialization_key.cull_face = None;
        self.pipeline_specialization_key.depth_write_enabled = false;
        self.pipeline_specialization_key.blend_state = Some(BlendState::ALPHA_BLENDING);
        true
    }

    pub(crate) fn configure_for_authored_terrain_decal(
        &mut self,
        base_texture: Handle<Image>,
        detail_texture: Handle<Image>,
        alpha_blend: bool,
        double_sided: bool,
        rotate_detail: bool,
        detail_vertical_scroll_per_second: f32,
        elapsed_seconds: f32,
    ) -> bool {
        if self.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER
            || !self.replace_texture_asset_at_d3d9_stage(0, base_texture)
            || !self.replace_texture_asset_at_d3d9_stage(1, detail_texture)
        {
            return false;
        }

        // Terrain decals use MODULATE2X for the detail texture on stage one.
        self.fixed_function.values[5].x = 5.0; // D3DTOP_MODULATE2X
        self.fixed_function.values[5].y = 2.0; // D3DTA_TEXTURE
        self.fixed_function.values[5].z = 1.0; // D3DTA_CURRENT
        self.fixed_function.values[5].w = 5.0; // D3DTOP_MODULATE2X
        self.fixed_function.values[6].x = 2.0; // D3DTA_TEXTURE
        self.fixed_function.values[6].y = 1.0; // D3DTA_CURRENT
        self.fixed_function.values[8].y = 1.0; // TexCoordIndex[1] = UV1
        self.fixed_function.values[9].x = 2.0; // D3DTTFF_COUNT2
        self.pipeline_specialization_key.cull_face = (!double_sided).then_some(Face::Back);
        self.pipeline_specialization_key.depth_write_enabled = false;
        self.pipeline_specialization_key.blend_state = alpha_blend.then_some(BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::OneMinusSrcAlpha,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::OneMinusSrcAlpha,
                operation: BlendOperation::Add,
            },
        });
        self.advance_authored_terrain_decal_detail_texture_coordinates(
            rotate_detail,
            detail_vertical_scroll_per_second,
            elapsed_seconds,
        );
        true
    }

    pub(crate) fn advance_authored_terrain_decal_detail_texture_coordinates(
        &mut self,
        rotate_detail: bool,
        detail_vertical_scroll_per_second: f32,
        elapsed_seconds: f32,
    ) -> bool {
        let vertical_offset = detail_vertical_scroll_per_second * elapsed_seconds;
        let next = if rotate_detail {
            Mat4::from_cols(
                Vec4::new(0.0, -1.0, 0.0, 0.0),
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::Z,
                Vec4::new(0.0, 1.0 + vertical_offset, 0.0, 1.0),
            )
        } else {
            Mat4::from_translation(Vec3::new(0.0, vertical_offset, 0.0))
        };
        if self.fixed_transforms.values[1]
            .to_cols_array()
            .map(f32::to_bits)
            == next.to_cols_array().map(f32::to_bits)
        {
            return false;
        }
        self.fixed_transforms.values[1] = next;
        true
    }

    pub(crate) fn clone_with_fixed_function_world_lighting_context(
        &self,
        lighting_override: Option<bool>,
        preserve_authored_material_lighting: bool,
        world_light_context: usize,
        buffers: &mut Assets<ShaderBuffer>,
    ) -> Option<Self> {
        if self.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER {
            return None;
        }
        let mut projected = self.clone();
        if let Some(enabled) = lighting_override
            .filter(|_| !preserve_authored_material_lighting || !self.lighting_state_is_authored)
        {
            projected.fixed_function.values[46].x = if enabled { 1.0 } else { 0.0 };
        }
        projected.fixed_function.values[47].x = world_light_context as f32;
        projected.uniform_buffer_assets.fixed_function =
            buffers.add(shader_buffer_from_uniform_value(&projected.fixed_function));
        if !projected.texture_coordinate_animations.is_empty() {
            projected.uniform_buffer_assets.fixed_transforms = buffers.add(
                shader_buffer_from_uniform_value(&projected.fixed_transforms),
            );
        }
        Some(projected)
    }

    pub(crate) fn replace_texture_asset_at_d3d9_stage(
        &mut self,
        texture_stage: usize,
        texture_asset: Handle<Image>,
    ) -> bool {
        let target = match texture_stage {
            0 => &mut self.texture_assets.texture0,
            1 => &mut self.texture_assets.texture1,
            2 => &mut self.texture_assets.texture2,
            3 => &mut self.texture_assets.texture3,
            4 => &mut self.texture_assets.texture4,
            5 => &mut self.texture_assets.texture5,
            6 => &mut self.texture_assets.texture6,
            7 => &mut self.texture_assets.texture7,
            _ => return false,
        };
        *target = Some(texture_asset);
        true
    }

    pub(crate) fn replace_texture_asset_for_effect_semantic(
        &mut self,
        semantic_name: &str,
        texture_asset: Handle<Image>,
    ) -> bool {
        let texture_stages = self
            .dynamic_texture_semantics
            .iter()
            .enumerate()
            .filter_map(|(texture_stage, semantic)| {
                semantic
                    .as_deref()
                    .is_some_and(|semantic| semantic.eq_ignore_ascii_case(semantic_name))
                    .then_some(texture_stage)
            })
            .collect::<Vec<_>>();
        for texture_stage in &texture_stages {
            self.replace_texture_asset_at_d3d9_stage(*texture_stage, texture_asset.clone());
        }
        !texture_stages.is_empty()
    }
}
