//! Ordered renderer passes over an effect's existing simulation buffers.

use bevy::render::view::RetainedViewEntity;

use super::*;

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
    /// Draw particles back to front in each view.
    pub sort_by_distance: bool,
    /// Overrides for the renderer's color and depth targets.
    pub state: Option<EffectRenderPassState>,
}

/// Authored pipeline state for a particle renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EffectRenderPassState {
    /// Color and alpha blending, with None for replacement.
    pub blend: Option<BlendState>,
    /// Whether to write the fragment depth.
    pub depth_write_enabled: bool,
    /// Depth comparison in Bevy's reverse-Z convention.
    pub depth_compare: CompareFunction,
    /// Writable color channels.
    pub color_writes: ColorWrites,
}

#[derive(Component, Default)]
pub(crate) struct CompiledEffectRenderPasses(Vec<CompiledEffectRenderPass>);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CompiledEffectRenderPass {
    pub shader: Handle<Shader>,
    pub flags: LayoutFlags,
    pub alpha_mode: AlphaMode,
    pub texture_layout: TextureLayout,
    pub textures: Vec<Handle<Image>>,
    pub sort_by_distance: bool,
    pub state: Option<EffectRenderPassState>,
}

pub(crate) fn compile_effect_render_passes(
    assets: Res<Assets<EffectAsset>>,
    mut events: MessageReader<AssetEvent<EffectAsset>>,
    mut shaders: ResMut<Assets<Shader>>,
    mut shader_cache: ResMut<ShaderCache>,
    mut renderer_shaders: Local<
        HashMap<(AssetId<EffectAsset>, ParticleLayout), (Handle<Shader>, LayoutFlags)>,
    >,
    mut draws: Query<(
        Ref<EffectRenderPasses>,
        Ref<crate::ParticleEffect>,
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
    if !changed.is_empty() {
        renderer_shaders.retain(|(id, _), _| !changed.contains(id));
    }
    for (passes, effect, mut compiled) in &mut draws {
        if !passes.is_changed()
            && !effect.is_changed()
            && !changed.contains(&effect.handle.id())
            && !passes
                .0
                .iter()
                .any(|pass| changed.contains(&pass.effect.id()))
        {
            continue;
        }
        compiled.0.clear();
        let Some(simulator) = assets.get(&effect.handle) else {
            continue;
        };
        let layout = simulator.particle_layout();
        for pass in &passes.0 {
            let Some(renderer) = assets.get(&pass.effect) else {
                continue;
            };
            let key = (pass.effect.id(), layout.clone());
            let (shader, flags) = if let Some(cached) = renderer_shaders.get(&key) {
                cached.clone()
            } else {
                let source = match crate::EffectShaderSources::generate_for_layout(
                    renderer,
                    layout.clone(),
                    None,
                    0,
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
            compiled.0.push(CompiledEffectRenderPass {
                shader,
                flags,
                alpha_mode: renderer.alpha_mode,
                texture_layout,
                textures: pass.textures[..texture_count].to_vec(),
                sort_by_distance: pass.sort_by_distance,
                state: pass.state,
            });
        }
    }
}

impl CompiledEffectRenderPasses {
    pub(super) fn passes(&self) -> Vec<CompiledEffectRenderPass> {
        self.0.clone()
    }
}

#[derive(Resource, Default)]
pub(crate) struct AuthoredParticleDrawQueue {
    pub(super) views: HashMap<(RetainedViewEntity, MainEntity), QueuedParticlePasses>,
}

pub(super) struct QueuedParticlePasses {
    pub passes: Vec<QueuedParticlePass>,
    pub sort: Option<super::distance_sort::ViewParticleSort>,
}

pub(super) struct QueuedParticlePass {
    pub pipeline: CachedRenderPipelineId,
    pub properties: PropertyBindGroupKey,
    pub sorted: bool,
}

impl CompiledEffectRenderPass {
    pub fn property_key(&self, batch: &EffectBatch) -> PropertyBindGroupKey {
        PropertyBindGroupKey {
            texture_layout: self.texture_layout.clone(),
            textures: self.textures.iter().map(Handle::id).collect(),
            ..batch.property_key.clone()
        }
    }
}
