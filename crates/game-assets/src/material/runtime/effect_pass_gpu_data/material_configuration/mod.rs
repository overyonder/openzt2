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
    pub fn has_fixed_function_reflection_projection(&self) -> bool {
        self.model_reflection_projections
            .iter()
            .any(Option::is_some)
    }

    pub fn set_fixed_function_texture_transform(&mut self, stage: usize, matrix: Mat4) -> bool {
        if stage >= 8 || self.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER {
            return false;
        }
        if self.fixed_transforms.values[stage] != matrix {
            self.fixed_transforms.values[stage] = matrix;
            self.dirty_fixed_transforms = true;
        }
        if self.fixed_function.values[stage * 5 + 4].x != 2.0 {
            self.fixed_function.values[stage * 5 + 4].x = 2.0;
            self.dirty_fixed_function = true;
        }
        true
    }

    pub fn set_fixed_function_ambient_colour(&mut self, colour: Vec4) -> bool {
        if self.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER {
            return false;
        }
        if self.fixed_function.values[48] != colour || self.fixed_function.values[47].y == 0.0 {
            self.fixed_function.values[48] = colour;
            self.fixed_function.values[47].y = 1.0;
            self.dirty_fixed_function = true;
        }
        true
    }
    /// Copies named texture, matrix and address inputs without changing vertex UV generation.
    pub fn copy_reflected_texture_sampling_from(
        &mut self,
        source: &Self,
        texture_semantic: &str,
        transform_semantic: &str,
    ) -> std::io::Result<bool> {
        let source_stage = source.unique_texture_semantic_stage(texture_semantic)?;
        let target_stage = self.unique_texture_semantic_stage(texture_semantic)?;
        let texture = match source_stage {
            0 => &source.texture_assets.texture0,
            1 => &source.texture_assets.texture1,
            2 => &source.texture_assets.texture2,
            3 => &source.texture_assets.texture3,
            4 => &source.texture_assets.texture4,
            5 => &source.texture_assets.texture5,
            6 => &source.texture_assets.texture6,
            7 => &source.texture_assets.texture7,
            _ => unreachable!(),
        };
        let Some(texture) = texture else {
            return Ok(false);
        };
        let matrix = source.reflected_float_matrix(transform_semantic)?;
        // Validate the destination before mutating any of its inputs.
        self.reflected_float_matrix(transform_semantic)?;
        let mut sampler = self.evaluated_sampler_states[target_stage];
        sampler.copy_addressing_from(&source.evaluated_sampler_states[source_stage])?;
        self.replace_texture_asset_at_d3d9_stage(target_stage, texture.clone());
        self.evaluated_sampler_states[target_stage] = sampler;
        self.bind_float_matrix_effect_semantic(transform_semantic, matrix);
        Ok(true)
    }

    fn unique_texture_semantic_stage(&self, name: &str) -> std::io::Result<usize> {
        let mut stages = self
            .dynamic_texture_semantics
            .iter()
            .enumerate()
            .filter_map(|(stage, semantic)| {
                semantic
                    .as_ref()
                    .is_some_and(|semantic| semantic.eq_ignore_ascii_case(name))
                    .then_some(stage)
            });
        let stage = stages.next().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("missing reflected texture semantic {name}"),
            )
        })?;
        if stages.next().is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("ambiguous reflected texture semantic {name}"),
            ));
        }
        Ok(stage)
    }

    fn reflected_float_matrix(&self, name: &str) -> std::io::Result<Mat4> {
        let invalid = || {
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("missing or incompatible reflected matrix semantic {name}"),
            )
        };
        let mut result = None;
        for binding in self
            .dynamic_parameter_register_bindings
            .iter()
            .filter(|binding| binding.semantic_name.eq_ignore_ascii_case(name))
        {
            if binding.register_set != 2 || binding.register_count != 4 {
                return Err(invalid());
            }
            let registers = match binding.shader_stage {
                super::D3d9ProgrammableShaderStage::Vertex => &self.vertex_floats.values,
                super::D3d9ProgrammableShaderStage::Pixel => &self.pixel_floats.values,
            };
            let first = binding.first_register as usize;
            let values = registers.get(first..first + 4).ok_or_else(invalid)?;
            let matrix = Mat4::from_cols(values[0], values[1], values[2], values[3]);
            let matrix = if binding.is_row_major_matrix {
                matrix
            } else {
                matrix.transpose()
            };
            if result.is_some_and(|prior| prior != matrix) {
                return Err(invalid());
            }
            result = Some(matrix);
        }
        result.ok_or_else(invalid)
    }

    pub fn has_vertex_float_effect_semantic(&self, name: &str) -> bool {
        self.dynamic_parameter_register_bindings
            .iter()
            .any(|binding| {
                matches!(
                    binding.shader_stage,
                    super::D3d9ProgrammableShaderStage::Vertex
                ) && binding.register_set == 2
                    && binding.semantic_name.eq_ignore_ascii_case(name)
            })
    }

    /// Copies evaluated sampling inputs while preserving this pass's combiners.
    /// Refresh after source animation and isolate this clone's uniform buffers first.
    pub fn copy_texture_stage_sampling_from(
        &mut self,
        source: &Self,
        source_stage: usize,
        target_stage: usize,
    ) -> std::io::Result<bool> {
        if source_stage >= 8 || target_stage >= 8 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "texture sampling stage exceeds the eight D3D9 stages",
            ));
        }
        if source.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER
            || self.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "sampling copy requires fixed-function texture coordinates",
            ));
        }
        let texture = match source_stage {
            0 => &source.texture_assets.texture0,
            1 => &source.texture_assets.texture1,
            2 => &source.texture_assets.texture2,
            3 => &source.texture_assets.texture3,
            4 => &source.texture_assets.texture4,
            5 => &source.texture_assets.texture5,
            6 => &source.texture_assets.texture6,
            7 => &source.texture_assets.texture7,
            _ => unreachable!(),
        };
        let Some(texture) = texture else {
            return Ok(false);
        };
        source.evaluated_sampler_states[source_stage].validate()?;
        self.replace_texture_asset_at_d3d9_stage(target_stage, texture.clone());
        self.evaluated_sampler_states[target_stage] = source.evaluated_sampler_states[source_stage];
        let coordinate_index = source.fixed_function.values[source_stage * 5 + 3].y;
        let transform_flags = source.fixed_function.values[source_stage * 5 + 4].x;
        if self.fixed_function.values[target_stage * 5 + 3].y != coordinate_index
            || self.fixed_function.values[target_stage * 5 + 4].x != transform_flags
        {
            self.fixed_function.values[target_stage * 5 + 3].y = coordinate_index;
            self.fixed_function.values[target_stage * 5 + 4].x = transform_flags;
            self.dirty_fixed_function = true;
        }
        let transform = source.fixed_transforms.values[source_stage];
        if self.fixed_transforms.values[target_stage] != transform {
            self.fixed_transforms.values[target_stage] = transform;
            self.dirty_fixed_transforms = true;
        }
        self.fixed_transform_bindings
            .retain(|binding| binding.stage() != target_stage);
        self.model_reflection_projections[target_stage] = None;
        Ok(true)
    }

    pub fn set_fixed_function_ambient_colour_argb(&mut self, argb: u32) -> bool {
        self.set_fixed_function_ambient_colour(fixed_function_ambient_colour_from_argb(argb))
    }

    pub fn clone_with_netimmerse_decal(
        &self,
        stage: usize,
        uv_set: u32,
        texture: Handle<Image>,
        buffers: &mut Assets<ShaderBuffer>,
    ) -> Self {
        let mut projected = self.clone();
        if self.has_fixed_function_reflection_projection() {
            return projected;
        }
        projected.replace_texture_asset_at_d3d9_stage(stage, texture);
        // BLENDTEXTUREALPHA(texture, current), preserving the surface alpha.
        projected.fixed_function.values[stage * 5] = Vec4::new(13.0, 2.0, 1.0, 2.0);
        projected.fixed_function.values[stage * 5 + 1].x = 1.0;
        projected.fixed_function.values[stage * 5 + 3].y = uv_set as f32;
        projected.uniform_buffer_assets.fixed_function =
            buffers.add(shader_buffer_from_uniform_value(&projected.fixed_function));
        projected
    }

    pub fn configure_for_authored_blue_fang_line(&mut self, texture: Handle<Image>) -> bool {
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

    pub fn configure_for_authored_terrain_surface(
        &mut self,
        base_texture: Handle<Image>,
        detail_texture: Handle<Image>,
    ) -> bool {
        if self.pipeline_specialization_key.vertex_shader != FIXED_FUNCTION_SHADER
            || !self.replace_texture_asset_at_d3d9_stage(0, base_texture)
            || !self.replace_texture_asset_at_d3d9_stage(1, detail_texture)
        {
            return false;
        }
        // The basic technique disables stage one; the detailed technique
        // multiplies the surface by twice the terrain detail texture.
        if self.fixed_function.values[5].x != 1.0 {
            self.fixed_function.values[5].x = 5.0; // D3DTOP_MODULATE2X
            self.dirty_fixed_function = true;
        }
        true
    }

    pub fn configure_for_authored_single_texture_terrain_decal(
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

    pub fn configure_for_authored_terrain_decal(
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

    pub fn advance_authored_terrain_decal_detail_texture_coordinates(
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

    pub fn clone_with_fixed_function_world_lighting_context(
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
        if !projected.texture_coordinate_animations.is_empty()
            || projected
                .model_reflection_projections
                .iter()
                .any(Option::is_some)
            || projected
                .fixed_transform_bindings
                .iter()
                .any(|binding| binding.input_dependencies() != 0)
        {
            projected.uniform_buffer_assets.fixed_transforms = buffers.add(
                shader_buffer_from_uniform_value(&projected.fixed_transforms),
            );
        }
        Some(projected)
    }

    pub fn replace_texture_asset_at_d3d9_stage(
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

    pub fn replace_texture_asset_for_effect_semantic(
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

pub(in crate::material::runtime) fn fixed_function_ambient_colour_from_argb(argb: u32) -> Vec4 {
    Vec4::new(
        ((argb >> 16) & 255) as f32,
        ((argb >> 8) & 255) as f32,
        (argb & 255) as f32,
        (argb >> 24) as f32,
    ) / 255.0
}
