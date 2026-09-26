//! Uniform-buffer and sampler bind-group preparation for one effect pass.

use bevy::{
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    render::{
        render_asset::RenderAssets,
        render_resource::{
            AsBindGroup, AsBindGroupError, BindGroupLayout, BindGroupLayoutEntry, BindingType,
            BufferBindingType, BufferInitDescriptor, BufferUsages, OwnedBindingResource,
            SamplerBindingType, ShaderStages, TextureSampleType, TextureViewDimension,
            UnpreparedBindGroup,
        },
        renderer::RenderDevice,
        storage::GpuShaderBuffer,
    },
};

use super::effect_pass_gpu_data::{
    EffectPassMaterial, EffectPassPipelineSpecializationKey, EffectPassTextureAssets,
    EFFECT_SHADER_BINDING_LAYOUT,
};

const FIRST_TEXTURE_BINDING: u32 = EFFECT_SHADER_BINDING_LAYOUT.first_texture_binding;
const FIRST_CUBE_TEXTURE_BINDING: u32 = EFFECT_SHADER_BINDING_LAYOUT.first_cube_texture_binding;
// Read by the fixed-function shader to choose each stage's texture type.
const CUBE_TEXTURE_STAGE_MASK_BINDING: u32 = 46;

impl AsBindGroup for EffectPassMaterial {
    type Data = EffectPassPipelineSpecializationKey;
    type Param = (
        <EffectPassTextureAssets as AsBindGroup>::Param,
        SRes<RenderAssets<GpuShaderBuffer>>,
    );

    fn label() -> &'static str {
        "effect pass material"
    }

    fn bind_group_data(&self) -> Self::Data {
        self.pipeline_specialization_key.clone()
    }

    fn unprepared_bind_group(
        &self,
        layout: &BindGroupLayout,
        render_device: &RenderDevice,
        param: &mut SystemParamItem<'_, '_, Self::Param>,
        force_no_bindless: bool,
    ) -> Result<UnpreparedBindGroup, AsBindGroupError> {
        let mut result = self.texture_assets.unprepared_bind_group(
            layout,
            render_device,
            &mut param.0,
            force_no_bindless,
        )?;
        // D3D9 decides a stage's texture type from the bound texture. Cube
        // textures therefore move to the parallel cube range, and the unused
        // slot of each pair receives the matching fallback view.
        let (images, fallback_image, _) = &param.0;
        let mut cube_texture_stage_mask = 0_u32;
        for (stage, texture_asset) in (0_u32..).zip(self.texture_assets.ordered_texture_assets()) {
            let bound_cube_texture_view = texture_asset
                .and_then(|handle| images.get(handle))
                .filter(|image| {
                    image
                        .texture_view_descriptor
                        .as_ref()
                        .and_then(|descriptor| descriptor.dimension)
                        == Some(TextureViewDimension::Cube)
                })
                .map(|image| image.texture_view.clone());
            let cube_texture_view = bound_cube_texture_view.map_or_else(
                || fallback_image.cube.texture_view.clone(),
                |view| {
                    cube_texture_stage_mask |= 1 << stage;
                    if let Some((_, resource)) = result
                        .bindings
                        .0
                        .iter_mut()
                        .find(|(candidate, _)| *candidate == FIRST_TEXTURE_BINDING + stage)
                    {
                        *resource = OwnedBindingResource::TextureView(
                            TextureViewDimension::D2,
                            fallback_image.d2.texture_view.clone(),
                        );
                    }
                    view
                },
            );
            result.bindings.0.push((
                FIRST_CUBE_TEXTURE_BINDING + stage,
                OwnedBindingResource::TextureView(TextureViewDimension::Cube, cube_texture_view),
            ));
        }
        let mut cube_texture_stage_mask_bytes = [0_u8; 16];
        cube_texture_stage_mask_bytes[..4].copy_from_slice(&cube_texture_stage_mask.to_le_bytes());
        result.bindings.0.push((
            CUBE_TEXTURE_STAGE_MASK_BINDING,
            OwnedBindingResource::Buffer(render_device.create_buffer_with_data(
                &BufferInitDescriptor {
                    label: Some("effect pass cube texture stages"),
                    contents: &cube_texture_stage_mask_bytes,
                    usage: BufferUsages::UNIFORM,
                },
            )),
        ));
        for (index, state) in self.evaluated_sampler_states.iter().enumerate() {
            let binding =
                10 + u32::try_from(index).map_err(|_| AsBindGroupError::RetryNextUpdate)?;
            if let Some((_, resource)) = result
                .bindings
                .0
                .iter_mut()
                .find(|(candidate, _)| *candidate == binding)
            {
                *resource = OwnedBindingResource::Sampler(
                    SamplerBindingType::Filtering,
                    render_device.create_sampler(&state.descriptor()),
                );
            }
        }
        let uniforms = [
            (1, &self.uniform_buffer_assets.fixed_function),
            (18, &self.uniform_buffer_assets.vertex_floats),
            (19, &self.uniform_buffer_assets.vertex_integers),
            (20, &self.uniform_buffer_assets.vertex_booleans),
            (27, &self.uniform_buffer_assets.pixel_floats),
            (28, &self.uniform_buffer_assets.pixel_integers),
            (29, &self.uniform_buffer_assets.pixel_booleans),
            (36, &self.uniform_buffer_assets.fixed_transforms),
        ];
        for (binding, handle) in uniforms {
            let Some(buffer) = param.1.get(handle) else {
                return Err(AsBindGroupError::RetryNextUpdate);
            };
            result
                .bindings
                .0
                .push((binding, OwnedBindingResource::Buffer(buffer.buffer.clone())));
        }
        Ok(result)
    }

    fn bind_group_layout_entries(
        render_device: &RenderDevice,
        force_no_bindless: bool,
    ) -> Vec<BindGroupLayoutEntry> {
        let mut entries =
            EffectPassTextureAssets::bind_group_layout_entries(render_device, force_no_bindless);
        entries.push(BindGroupLayoutEntry {
            binding: 1,
            visibility: ShaderStages::VERTEX_FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        entries.extend(
            [18, 19, 20, 27, 28, 29, 36].map(|binding| BindGroupLayoutEntry {
                binding,
                visibility: if matches!(binding, 18 | 19 | 20) {
                    ShaderStages::VERTEX
                } else {
                    ShaderStages::FRAGMENT
                },
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }),
        );
        entries.extend((0..8).map(|stage| BindGroupLayoutEntry {
            binding: FIRST_CUBE_TEXTURE_BINDING + stage,
            visibility: ShaderStages::VERTEX_FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::Cube,
                multisampled: false,
            },
            count: None,
        }));
        entries.push(BindGroupLayoutEntry {
            binding: CUBE_TEXTURE_STAGE_MASK_BINDING,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        entries.sort_unstable_by_key(|entry| entry.binding);
        entries
    }
}
