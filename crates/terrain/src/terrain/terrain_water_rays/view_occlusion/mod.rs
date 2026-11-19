use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use bevy::{
    core_pipeline::{
        core_3d::{main_opaque_pass_3d, main_transparent_pass_3d},
        schedule::Core3d,
        Core3dSystems,
    },
    mesh::VertexBufferLayout,
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            CachedRenderPipelineId, PipelineCache, RenderPipelineDescriptor,
            SpecializedRenderPipeline, SpecializedRenderPipelines, VertexState,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        view::ViewDepthStencilTexture,
        Render, RenderApp, RenderSystems,
    },
    shader::Shader,
};

/// Camera-owned draw input and asynchronous result, shared only across extraction.
#[derive(Component, Clone, ExtractComponent)]
#[extract_app(RenderApp)]
pub(in crate::terrain) struct WaterRayViewOcclusion {
    pub clip_corners: [Vec4; 4],
    pub full_sample_count: f32,
    pub projected_fraction: f32,
    result: Arc<Mutex<Option<f32>>>,
}

impl Default for WaterRayViewOcclusion {
    fn default() -> Self {
        Self {
            clip_corners: [Vec4::ZERO; 4],
            full_sample_count: 0.0,
            projected_fraction: 0.0,
            result: Arc::default(),
        }
    }
}

impl WaterRayViewOcclusion {
    pub(in crate::terrain) fn visible_fraction(&self) -> Option<f32> {
        *self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub(in crate::terrain) struct WaterRayOcclusionPlugin;

impl Plugin for WaterRayOcclusionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<WaterRayViewOcclusion>::default());
        if app.get_sub_app(RenderApp).is_none() {
            return;
        }
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(
                WATER_RAY_OCCLUSION_SHADER.id(),
                Shader::from_wgsl(
                    include_str!("occlusion.wgsl"),
                    "embedded://openzt2/water-ray-occlusion.wgsl",
                ),
            )
            .expect("water ray occlusion shader uses a UUID handle");
        let render = app.sub_app_mut(RenderApp);
        render.init_resource::<SpecializedRenderPipelines<WaterRayOcclusionPipeline>>();
        render.add_systems(
            Render,
            prepare_water_ray_view_queries.in_set(RenderSystems::PrepareResources),
        );
        render.add_systems(
            Core3d,
            query_water_ray_occlusion
                .in_set(Core3dSystems::MainPass)
                .after(main_opaque_pass_3d)
                .before(main_transparent_pass_3d),
        );
        render.add_systems(
            Render,
            (map_water_ray_occlusion, remove_inactive_view_queries).in_set(RenderSystems::Cleanup),
        );
    }
}

const WATER_RAY_OCCLUSION_SHADER: Handle<Shader> =
    bevy::asset::uuid_handle!("216ec9aa-518c-4b5f-8b0d-c228c2f91449");

struct WaterRayOcclusionPipeline;

impl SpecializedRenderPipeline for WaterRayOcclusionPipeline {
    type Key = (wgpu::TextureFormat, u32);

    fn specialize(&self, (format, samples): Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("water ray occlusion".into()),
            vertex: VertexState {
                shader: WATER_RAY_OCCLUSION_SHADER,
                entry_point: Some("vertex".into()),
                buffers: vec![VertexBufferLayout::from_vertex_formats(
                    wgpu::VertexStepMode::Vertex,
                    [wgpu::VertexFormat::Float32x4],
                )],
                ..default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: default(),
                bias: default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..default()
            },
            ..default()
        }
    }
}

#[derive(Component, PartialEq, Eq)]
struct WaterRayViewPipeline(CachedRenderPipelineId);

fn prepare_water_ray_view_queries(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<WaterRayOcclusionPipeline>>,
    mut views: Query<
        (
            Entity,
            &ViewDepthStencilTexture,
            Option<&mut WaterRayViewPipeline>,
            Has<WaterRayGpuQuery>,
        ),
        With<WaterRayViewOcclusion>,
    >,
) {
    for (entity, depth, pipeline, prepared) in &mut views {
        let next = WaterRayViewPipeline(pipelines.specialize(
            &pipeline_cache,
            &WaterRayOcclusionPipeline,
            (depth.texture().format(), depth.texture().sample_count()),
        ));
        if let Some(mut current) = pipeline {
            current.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
        if !prepared {
            commands.entity(entity).insert(WaterRayGpuQuery::default());
        }
    }
}

struct ViewQueryBuffers {
    vertices: wgpu::Buffer,
    query: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    issued: AtomicBool,
    pending: Arc<AtomicBool>,
    result: Arc<Mutex<Option<f32>>>,
    denominator: Mutex<f32>,
}

#[derive(Component, Default)]
struct WaterRayGpuQuery(Mutex<Option<ViewQueryBuffers>>);

fn query_water_ray_occlusion(
    view: ViewQuery<(
        &WaterRayViewOcclusion,
        &ViewDepthStencilTexture,
        &WaterRayGpuQuery,
        &WaterRayViewPipeline,
    )>,
    mut context: RenderContext,
    pipeline_cache: Res<PipelineCache>,
    queue: Res<RenderQueue>,
) {
    let (request, depth, gpu, pipeline) = view.into_inner();
    let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline.0) else {
        return;
    };
    let mut state = gpu
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let samples = depth.texture().sample_count();
    if state
        .as_ref()
        .is_some_and(|state| state.pending.load(Ordering::Acquire))
    {
        return;
    }
    if request.full_sample_count <= 0.0 {
        *request
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(0.0);
        return;
    }
    if state
        .as_ref()
        .is_none_or(|state| !Arc::ptr_eq(&state.result, &request.result))
    {
        *state = Some(create_view_query(context.render_device(), request));
    }
    let Some(state) = state.as_ref() else {
        return;
    };
    let vertices = [0, 1, 2, 2, 1, 3]
        .map(|index| request.clip_corners[index].to_array().map(f32::to_ne_bytes));
    queue.write_buffer(&state.vertices, 0, vertices.as_flattened().as_flattened());
    {
        let mut pass = context
            .command_encoder()
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("water ray occlusion"),
                color_attachments: &[],
                depth_stencil_attachment: Some(depth.get_attachment(wgpu::StoreOp::Store)),
                occlusion_query_set: Some(&state.query),
                timestamp_writes: None,
                multiview_mask: None,
            });
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, state.vertices.slice(..));
        pass.begin_occlusion_query(0);
        pass.draw(0..6, 0..1);
        pass.end_occlusion_query();
    }
    context
        .command_encoder()
        .resolve_query_set(&state.query, 0..1, &state.resolve, 0);
    context
        .command_encoder()
        .copy_buffer_to_buffer(&state.resolve, 0, &state.readback, 0, 8);
    *state
        .denominator
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) =
        request.full_sample_count * samples as f32;
    state.pending.store(true, Ordering::Release);
    state.issued.store(true, Ordering::Release);
}

fn create_view_query(device: &RenderDevice, request: &WaterRayViewOcclusion) -> ViewQueryBuffers {
    let device = device.wgpu_device();
    ViewQueryBuffers {
        vertices: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water ray occlusion vertices"),
            size: 6 * 16,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        query: device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("water ray visibility"),
            ty: wgpu::QueryType::Occlusion,
            count: 1,
        }),
        resolve: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water ray query resolve"),
            size: 256,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }),
        readback: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water ray query readback"),
            size: 8,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        }),
        issued: AtomicBool::new(false),
        pending: Arc::default(),
        result: request.result.clone(),
        denominator: Mutex::new(0.0),
    }
}

fn map_water_ray_occlusion(queries: Query<&WaterRayGpuQuery>) {
    for query in &queries {
        let state = query
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = state.as_ref() else {
            continue;
        };
        if !state.issued.swap(false, Ordering::AcqRel) {
            continue;
        }
        let buffer = state.readback.clone();
        let result = state.result.clone();
        let pending = state.pending.clone();
        let denominator = *state
            .denominator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |status| {
                if status.is_ok() {
                    if let Ok(bytes) = buffer.slice(..).get_mapped_range() {
                        let count = u64::from_ne_bytes(
                            bytes[..].try_into().expect("eight byte query buffer"),
                        );
                        *result
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some((count as f32 / denominator).clamp(0.0, 1.0));
                    }
                    buffer.unmap();
                }
                pending.store(false, Ordering::Release);
            });
    }
}

fn remove_inactive_view_queries(
    mut commands: Commands,
    queries: Query<Entity, (With<WaterRayGpuQuery>, Without<WaterRayViewOcclusion>)>,
) {
    for entity in &queries {
        commands
            .entity(entity)
            .remove::<(WaterRayGpuQuery, WaterRayViewPipeline)>();
    }
}
