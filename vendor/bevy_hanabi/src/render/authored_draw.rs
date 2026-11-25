//! Ordered renderer passes over an effect's existing simulation buffers.

use super::*;
use wgpu::SamplerBorderColor;

/// Ordered draw passes for a single particle simulation.
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(CompiledEffectRenderPasses)]
pub struct EffectRenderPasses(
    /// Passes in authored renderer and material order.
    pub Vec<EffectRenderPass>,
);

/// A renderer shader and material for an existing particle population.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectRenderPass {
    /// Effect containing the renderer modifiers.
    pub effect: Handle<EffectAsset>,
    /// Textures in the renderer's binding order.
    pub textures: Vec<Handle<Image>>,
    /// Sampler overrides in the same order as the texture bindings.
    pub samplers: Vec<Option<EffectSampler>>,
    /// Draw particles back to front in each view.
    pub sort_by_distance: bool,
    /// Overrides for the renderer's color and depth-stencil targets.
    pub state: Option<EffectRenderPassState>,
    /// Reference value for the stencil test and replace operation.
    pub stencil_reference: Option<u32>,
    /// Optional fragment-alpha comparison before blending.
    pub alpha_test: Option<EffectAlphaTest>,
}

/// An alpha threshold encoded as an eight-bit normalized value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EffectAlphaTest {
    /// Comparison of the shader's fragment alpha against the reference.
    pub compare: CompareFunction,
    /// Alpha at the comparison boundary.
    pub reference: u8,
}

impl EffectAlphaTest {
    pub(crate) fn fragment_code(self) -> String {
        let op = match self.compare {
            CompareFunction::Never => return "if true { discard; }\n".to_owned(),
            CompareFunction::Always => return String::new(),
            CompareFunction::Less => ">=",
            CompareFunction::Equal => "!=",
            CompareFunction::LessEqual => ">",
            CompareFunction::Greater => "<=",
            CompareFunction::NotEqual => "==",
            CompareFunction::GreaterEqual => "<",
        };
        format!(
            "if color.a {op} ({}.0 / 255.0) {{ discard; }}\n",
            self.reference
        )
    }
}

/// A render sampler descriptor with a stable cache key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectSampler {
    address: [AddressMode; 3],
    mag_filter: FilterMode,
    min_filter: FilterMode,
    mipmap_filter: MipmapFilterMode,
    lod_clamp: [u32; 2],
    compare: Option<CompareFunction>,
    anisotropy_clamp: u16,
    border_color: Option<SamplerBorderColor>,
}

impl From<SamplerDescriptor<'_>> for EffectSampler {
    fn from(source: SamplerDescriptor<'_>) -> Self {
        Self {
            address: [
                source.address_mode_u,
                source.address_mode_v,
                source.address_mode_w,
            ],
            mag_filter: source.mag_filter,
            min_filter: source.min_filter,
            mipmap_filter: source.mipmap_filter,
            lod_clamp: [
                source.lod_min_clamp.to_bits(),
                source.lod_max_clamp.to_bits(),
            ],
            compare: source.compare,
            anisotropy_clamp: source.anisotropy_clamp,
            border_color: source.border_color,
        }
    }
}

impl EffectSampler {
    pub(super) fn descriptor(&self) -> SamplerDescriptor<'static> {
        SamplerDescriptor {
            label: Some("particle effect sampler"),
            address_mode_u: self.address[0],
            address_mode_v: self.address[1],
            address_mode_w: self.address[2],
            mag_filter: self.mag_filter,
            min_filter: self.min_filter,
            mipmap_filter: self.mipmap_filter,
            lod_min_clamp: f32::from_bits(self.lod_clamp[0]),
            lod_max_clamp: f32::from_bits(self.lod_clamp[1]),
            compare: self.compare,
            anisotropy_clamp: self.anisotropy_clamp,
            border_color: self.border_color,
        }
    }
}

/// Authored pipeline state for a particle renderer.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectRenderPassState {
    /// Color and alpha blending, with None for replacement.
    pub blend: Option<BlendState>,
    /// Whether to write the fragment depth.
    pub depth_write_enabled: bool,
    /// Depth comparison in Bevy's reverse-Z convention.
    pub depth_compare: CompareFunction,
    /// Stencil test and operations for stencil-capable views.
    pub stencil: StencilState,
    /// Writable color channels.
    pub color_writes: ColorWrites,
}

#[derive(Component, Default, PartialEq)]
pub(crate) struct CompiledEffectRenderPasses(Vec<CompiledEffectRenderPass>);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CompiledEffectRenderPass {
    pub shader: Handle<Shader>,
    pub flags: LayoutFlags,
    pub alpha_mode: AlphaMode,
    pub texture_layout: TextureLayout,
    pub textures: Vec<Handle<Image>>,
    pub samplers: Vec<Option<EffectSampler>>,
    pub sort_by_distance: bool,
    pub state: Option<EffectRenderPassState>,
    pub stencil_reference: Option<u32>,
}

pub(crate) fn compile_effect_render_passes(
    assets: Res<Assets<EffectAsset>>,
    mut events: MessageReader<AssetEvent<EffectAsset>>,
    mut shaders: ResMut<Assets<Shader>>,
    mut shader_cache: ResMut<ShaderCache>,
    mut renderer_shaders: Local<
        HashMap<
            (
                AssetId<EffectAsset>,
                ParticleLayout,
                Option<EffectAlphaTest>,
            ),
            (Handle<Shader>, LayoutFlags),
        >,
    >,
    changed_draws: Query<
        Entity,
        (
            With<EffectRenderPasses>,
            With<crate::ParticleEffect>,
            Or<(Changed<EffectRenderPasses>, Changed<crate::ParticleEffect>)>,
        ),
    >,
    sources: Query<(Entity, &EffectRenderPasses, &crate::ParticleEffect)>,
    mut draws: Query<(
        &EffectRenderPasses,
        &crate::ParticleEffect,
        &mut CompiledEffectRenderPasses,
    )>,
) {
    let changed: bevy::platform::collections::HashSet<_> = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect();
    let mut selected: bevy::ecs::entity::EntityHashSet = changed_draws.iter().collect();
    if !changed.is_empty() {
        renderer_shaders.retain(|(id, _, _), _| !changed.contains(id));
        selected.extend(sources.iter().filter_map(|(entity, passes, effect)| {
            (changed.contains(&effect.handle.id())
                || passes
                    .0
                    .iter()
                    .any(|pass| changed.contains(&pass.effect.id())))
            .then_some(entity)
        }));
    }
    let mut selected_draws = draws.iter_many_mut(selected).matched();
    while let Some((passes, effect, mut compiled)) = selected_draws.fetch_next() {
        let Some(simulator) = assets.get(&effect.handle) else {
            compiled.set_if_neq(CompiledEffectRenderPasses::default());
            continue;
        };
        let layout = simulator.particle_layout();
        let mut draw_passes = Vec::with_capacity(passes.0.len());
        for pass in &passes.0 {
            let Some(renderer) = assets.get(&pass.effect) else {
                continue;
            };
            let key = (pass.effect.id(), layout.clone(), pass.alpha_test);
            let (shader, flags) = if let Some(cached) = renderer_shaders.get(&key) {
                cached.clone()
            } else {
                let source = match crate::EffectShaderSources::generate_for_layout(
                    renderer,
                    layout.clone(),
                    None,
                    0,
                    pass.alpha_test,
                ) {
                    Ok(source) => source,
                    Err(error) => {
                        error!(
                            "Failed to prepare particle renderer '{}': {error}",
                            renderer.name
                        );
                        continue;
                    }
                };
                let shader = shader_cache.get_or_insert(
                    &renderer.name,
                    "render",
                    &source.render_shader_source,
                    &mut shaders,
                );
                renderer_shaders.insert(key, (shader.clone(), source.layout_flags));
                (shader, source.layout_flags)
            };
            let texture_layout = renderer.texture_layout();
            let texture_count = texture_layout.layout.len();
            if texture_count > pass.textures.len() {
                continue;
            }
            draw_passes.push(CompiledEffectRenderPass {
                shader,
                flags,
                alpha_mode: renderer.alpha_mode,
                texture_layout,
                textures: pass.textures[..texture_count].to_vec(),
                samplers: pass.samplers.iter().take(texture_count).cloned().collect(),
                sort_by_distance: pass.sort_by_distance,
                state: pass.state.clone(),
                stencil_reference: pass.stencil_reference,
            });
        }
        if draw_passes.len() != passes.0.len() {
            draw_passes.clear();
        }
        compiled.set_if_neq(CompiledEffectRenderPasses(draw_passes));
    }
}

impl CompiledEffectRenderPasses {
    pub(super) fn passes(&self) -> Vec<CompiledEffectRenderPass> {
        self.0.clone()
    }
}

#[derive(Component, Default)]
pub(crate) struct ParticleViewDraws {
    pub(super) authored: HashMap<MainEntity, QueuedParticlePasses>,
    #[cfg(feature = "3d")]
    pub(super) binned_pipeline_key: Option<ParticleBinnedViewKey>,
}

#[cfg(feature = "3d")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ParticleBinnedViewKey {
    pub target_format: TextureFormat,
    pub depth_format: TextureFormat,
    pub msaa_samples: u32,
}

pub(crate) fn initialize_particle_draw_views(
    mut commands: Commands,
    views: Query<Entity, (With<ExtractedView>, Without<ParticleViewDraws>)>,
) {
    for entity in &views {
        commands.entity(entity).insert(ParticleViewDraws::default());
    }
}

pub(super) struct QueuedParticlePasses {
    pub passes: Vec<QueuedParticlePass>,
    pub sort: Option<super::distance_sort::ViewParticleSort>,
}

pub(super) struct QueuedParticlePass {
    pub pipeline: CachedRenderPipelineId,
    pub properties: PropertyBindGroupKey,
    pub sorted: bool,
    pub needs_view_fog: bool,
    pub stencil_reference: Option<u32>,
}

impl CompiledEffectRenderPass {
    pub fn property_key(&self, batch: &EffectBatch) -> PropertyBindGroupKey {
        PropertyBindGroupKey {
            texture_layout: self.texture_layout.clone(),
            textures: self.textures.iter().map(Handle::id).collect(),
            samplers: self.samplers.clone(),
            ..batch.property_key.clone()
        }
    }
}
