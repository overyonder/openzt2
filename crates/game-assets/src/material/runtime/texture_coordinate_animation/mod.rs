//! Immutable UV curves and the animated material buffer update.

#[cfg(test)]
mod tests;

use std::{collections::HashSet, ops::RangeInclusive, sync::Arc};

use bevy::{
    asset::AssetEvent,
    curve::cubic_splines::CubicSegment,
    pbr::MeshMaterial3d,
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_asset::{prepare_assets, RenderAssets},
        render_resource::BufferId,
        renderer::RenderQueue,
        storage::{GpuShaderBuffer, ShaderBuffer},
        sync_world::RenderEntity,
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
    },
};

use super::effect_pass_gpu_data::EffectPassMaterial;

/// One native controller lowered to per-segment polynomial coefficients.
/// Curves are shared by material instances; only their clock origins differ.
#[derive(Clone, Debug)]
pub struct TextureCoordinateAnimation {
    pub uv_set: u16,
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub interval: RangeInclusive<f32>,
    pub tracks: [Box<[(f32, CubicSegment<f32>)]>; 4],
}

impl TextureCoordinateAnimation {
    fn transform(&self, elapsed: f32, started: f32) -> Mat4 {
        if self.flags & 8 == 0 {
            return Mat4::IDENTITY;
        }
        let elapsed = if self.flags & 1 != 0 {
            elapsed - started
        } else {
            elapsed
        };
        let time = self.sample_time(elapsed);
        let [offset_u, offset_v, scale_u, scale_v] = std::array::from_fn(|index| {
            sample_track(&self.tracks[index], time, if index < 2 { 0.0 } else { 1.0 })
        });
        Mat4::from_cols(
            Vec4::new(scale_u, 0.0, 0.0, 0.0),
            Vec4::new(0.0, scale_v, 0.0, 0.0),
            Vec4::Z,
            Vec4::new(
                (1.0 - scale_u).mul_add(0.5, -offset_u),
                (1.0 - scale_v).mul_add(0.5, offset_v),
                0.0,
                1.0,
            ),
        )
    }

    fn sample_time(&self, elapsed: f32) -> f32 {
        let start = *self.interval.start();
        let stop = *self.interval.end();
        let duration = stop - start;
        if duration <= 0.0 {
            return start;
        }
        let time = elapsed.mul_add(self.frequency, self.phase);
        match (self.flags >> 1) & 3 {
            0 => {
                let wrapped = start + time % duration;
                (if wrapped < 0.0 {
                    wrapped + duration
                } else {
                    wrapped
                })
                .clamp(start, stop)
            }
            1 => {
                let phase = time.rem_euclid(2.0 * duration);
                start
                    + if phase <= duration {
                        phase
                    } else {
                        2.0_f32.mul_add(duration, -phase)
                    }
            }
            _ => time.clamp(start, stop),
        }
    }
}

fn sample_track(keys: &[(f32, CubicSegment<f32>)], time: f32, default: f32) -> f32 {
    let Some(first) = keys.first() else {
        return default;
    };
    if time <= first.0 {
        return first.1.position(0.0);
    }
    let index = keys.partition_point(|key| key.0 <= time).saturating_sub(1);
    let (start, segment) = &keys[index];
    let fraction = keys
        .get(index + 1)
        .map_or(0.0, |next| (time - start) / (next.0 - start));
    segment.position(fraction)
}

/// Playback begins with the geometry and survives material changes.
#[derive(Component)]
pub struct GeometryTextureCoordinateAnimationClock {
    start_seconds: f32,
}

impl GeometryTextureCoordinateAnimationClock {
    pub const fn new(start_seconds: f32) -> Self {
        Self { start_seconds }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureCoordinateAnimationSet;

#[derive(Clone, PartialEq)]
struct TextureAnimationBufferTarget {
    geometry: Entity,
    transforms: Handle<ShaderBuffer>,
    uv_sets: [Option<u32>; 8],
}

#[derive(Component)]
struct TextureAnimationPass {
    target: TextureAnimationBufferTarget,
    controllers: Option<Arc<[TextureCoordinateAnimation]>>,
}

#[derive(Clone, Debug, Component, PartialEq, ExtractComponent)]
#[extract_app(RenderApp)]
#[extract_component_filter(Changed<GeometryTextureAnimationSample>)]
struct GeometryTextureAnimationSample(Vec<(u16, Mat4)>);

#[derive(Component)]
struct ExtractedTextureAnimationBufferTarget {
    target: TextureAnimationBufferTarget,
    prepared_buffer: Option<BufferId>,
}

pub(super) fn register_texture_coordinate_animation(application: &mut App) {
    application
        .add_plugins(ExtractComponentPlugin::<GeometryTextureAnimationSample>::default())
        .add_systems(
            PostUpdate,
            (
                project_texture_animation_passes,
                sample_geometry_texture_animations.after(project_texture_animation_passes),
            )
                .in_set(TextureCoordinateAnimationSet),
        );
    if let Some(render) = application.get_sub_app_mut(RenderApp) {
        render
            .add_systems(ExtractSchedule, extract_texture_animation_targets)
            .add_systems(
                Render,
                upload_geometry_texture_animations
                    .in_set(RenderSystems::PrepareResources)
                    .after(prepare_assets::<GpuShaderBuffer>),
            );
    }
}

fn project_texture_animation_passes(
    mut commands: Commands,
    materials: Res<Assets<EffectPassMaterial>>,
    mut material_events: MessageReader<AssetEvent<EffectPassMaterial>>,
    mut removed_materials: RemovedComponents<MeshMaterial3d<EffectPassMaterial>>,
    changed_passes: Query<Entity, Changed<MeshMaterial3d<EffectPassMaterial>>>,
    material_handles: Query<(Entity, &MeshMaterial3d<EffectPassMaterial>)>,
    mut animations: Query<&mut TextureAnimationPass>,
) {
    let changed_materials: HashSet<_> = material_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect();
    let mut selected: bevy::ecs::entity::EntityHashSet = changed_passes.iter().collect();
    if !changed_materials.is_empty() {
        selected.extend(material_handles.iter().filter_map(|(entity, material)| {
            changed_materials
                .contains(&material.0.id())
                .then_some(entity)
        }));
    }
    for entity in removed_materials.read() {
        if !material_handles.contains(entity) && animations.contains(entity) {
            commands
                .entity(entity)
                .remove::<(TextureAnimationPass, GeometryTextureAnimationSample)>();
        }
    }

    for (entity, handle) in material_handles.iter_many(selected).matched() {
        let current = animations.get_mut(entity).ok();
        let Some(material) = materials
            .get(&handle.0)
            .filter(|item| item.has_texture_coordinate_animation())
        else {
            if current.is_some() {
                commands
                    .entity(entity)
                    .remove::<(TextureAnimationPass, GeometryTextureAnimationSample)>();
            }
            continue;
        };
        let geometry = material
            .texture_coordinate_animation_geometry
            .unwrap_or(entity);
        let target = TextureAnimationBufferTarget {
            geometry,
            transforms: material.uniform_buffer_assets.fixed_transforms.clone(),
            uv_sets: std::array::from_fn(|stage| {
                (material.fixed_function.values[stage * 5 + 4].x.to_bits() == 2.0_f32.to_bits())
                    .then(|| material.fixed_function.values[stage * 5 + 3].y.to_bits())
            }),
        };
        let controllers =
            (geometry == entity).then(|| material.texture_coordinate_animations.clone());
        if let Some(mut current) = current {
            let same_controllers = match (&current.controllers, &controllers) {
                (Some(previous), Some(next)) => Arc::ptr_eq(previous, next),
                (None, None) => true,
                _ => false,
            };
            if current.target != target || !same_controllers {
                *current = TextureAnimationPass {
                    target,
                    controllers,
                };
            }
        } else {
            commands.entity(entity).insert(TextureAnimationPass {
                target,
                controllers,
            });
        }
    }
}

fn sample_geometry_texture_animations(
    mut commands: Commands,
    time: Res<Time>,
    mut geometries: Query<(
        Entity,
        &TextureAnimationPass,
        Option<&GeometryTextureCoordinateAnimationClock>,
        Option<&mut GeometryTextureAnimationSample>,
    )>,
) {
    let elapsed = time.elapsed_secs();
    for (entity, animation, clock, sample) in &mut geometries {
        let Some(controllers) = &animation.controllers else {
            if sample.is_some() {
                commands
                    .entity(entity)
                    .remove::<GeometryTextureAnimationSample>();
            }
            continue;
        };
        let started = clock.map_or(elapsed, |clock| clock.start_seconds);
        let mut values: Vec<(u16, Mat4)> = Vec::with_capacity(controllers.len());
        for animation in controllers.iter() {
            let transform = animation.transform(elapsed, started);
            if let Some((_, matrix)) = values.iter_mut().find(|(uv, _)| *uv == animation.uv_set) {
                *matrix = transform * *matrix;
            } else {
                values.push((animation.uv_set, transform));
            }
        }
        let next = GeometryTextureAnimationSample(values);
        if let Some(mut sample) = sample {
            sample.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

fn extract_texture_animation_targets(
    mut commands: Commands,
    passes: Extract<Query<(RenderEntity, Ref<TextureAnimationPass>)>>,
    render_entities: Extract<Query<RenderEntity>>,
    mut removed: Extract<RemovedComponents<TextureAnimationPass>>,
    mut shader_buffer_events: Extract<MessageReader<AssetEvent<ShaderBuffer>>>,
    mut extracted: Query<&mut ExtractedTextureAnimationBufferTarget>,
) {
    let uploaded: HashSet<_> = shader_buffer_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(id),
            _ => None,
        })
        .collect();
    for (render_entity, animation) in &passes {
        let mut previous = extracted.get_mut(render_entity).ok();
        if let Some(previous) = previous.as_mut() {
            if !animation.is_changed() && !uploaded.contains(&previous.target.transforms.id()) {
                continue;
            }
        }
        let mut target = animation.target.clone();
        let Ok(geometry) = render_entities.get(target.geometry) else {
            if previous.is_some() {
                commands
                    .entity(render_entity)
                    .remove::<ExtractedTextureAnimationBufferTarget>();
            }
            continue;
        };
        target.geometry = geometry;
        if let Some(mut previous) = previous {
            *previous = ExtractedTextureAnimationBufferTarget {
                target,
                prepared_buffer: None,
            };
        } else {
            commands
                .entity(render_entity)
                .insert(ExtractedTextureAnimationBufferTarget {
                    target,
                    prepared_buffer: None,
                });
        }
    }
    for entity in removed.read() {
        if let Ok(render_entity) = render_entities.get(entity) {
            if !passes.contains(entity) && extracted.contains(render_entity) {
                commands
                    .entity(render_entity)
                    .remove::<ExtractedTextureAnimationBufferTarget>();
            }
        }
    }
}

fn upload_geometry_texture_animations(
    queue: Res<RenderQueue>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
    samples: Query<Ref<GeometryTextureAnimationSample>>,
    mut targets: Query<&mut ExtractedTextureAnimationBufferTarget>,
) {
    for mut draw in &mut targets {
        let target = &draw.target;
        let (Ok(sample), Some(buffer)) = (
            samples.get(target.geometry),
            buffers.get(&target.transforms),
        ) else {
            continue;
        };
        if !sample.is_changed() && draw.prepared_buffer == Some(buffer.buffer.id()) {
            continue;
        }
        for (stage, uv) in target.uv_sets.iter().enumerate() {
            let Some(matrix) = uv.and_then(|uv| {
                sample
                    .0
                    .iter()
                    .find_map(|(set, matrix)| (uv == f32::from(*set).to_bits()).then_some(matrix))
            }) else {
                continue;
            };
            let bytes = matrix.to_cols_array().map(f32::to_le_bytes);
            queue.write_buffer(
                &buffer.buffer,
                (stage * bytes.as_flattened().len()) as u64,
                bytes.as_flattened(),
            );
        }
        draw.prepared_buffer = Some(buffer.buffer.id());
    }
}
