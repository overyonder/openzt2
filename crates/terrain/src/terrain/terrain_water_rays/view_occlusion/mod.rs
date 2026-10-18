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
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::ViewDepthTexture,
        Render, RenderApp, RenderSystems,
    },
};

/// Camera-owned draw input and asynchronous result, shared only across extraction.
#[derive(Component, Clone, ExtractComponent)]
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
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
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

struct ViewQueryBuffers {
    query: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    samples: u32,
    issued: AtomicBool,
    pending: Arc<AtomicBool>,
    result: Arc<Mutex<Option<f32>>>,
    denominator: Mutex<f32>,
}

#[derive(Component, Default)]
struct WaterRayGpuQuery(Mutex<Option<ViewQueryBuffers>>);

fn query_water_ray_occlusion(
    view: ViewQuery<(
        Entity,
        &WaterRayViewOcclusion,
        &ViewDepthTexture,
        Option<&WaterRayGpuQuery>,
    )>,
    mut commands: Commands,
    mut context: RenderContext,
) {
    let (entity, request, depth, gpu) = view.into_inner();
    let Some(gpu) = gpu else {
        commands.entity(entity).insert(WaterRayGpuQuery::default());
        return;
    };
    let mut state = gpu
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let samples = depth.texture.sample_count();
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
    if state.as_ref().is_none_or(|state| {
        state.samples != samples || !Arc::ptr_eq(&state.result, &request.result)
    }) {
        *state = Some(create_view_query(
            context.render_device(),
            depth.texture.format(),
            samples,
            request,
        ));
    }
    let Some(state) = state.as_ref() else {
        return;
    };
    let vertices: Vec<u8> = [0, 1, 2, 2, 1, 3]
        .into_iter()
        .flat_map(|index| request.clip_corners[index].to_array())
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertices = context.render_device().wgpu_device().create_buffer_init(
        &wgpu::util::BufferInitDescriptor {
            label: Some("water ray occlusion vertices"),
            contents: &vertices,
            usage: wgpu::BufferUsages::VERTEX,
        },
    );
    {
        let mut pass = context
            .command_encoder()
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("water ray occlusion"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth.view(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: Some(&state.query),
                timestamp_writes: None,
                multiview_mask: None,
            });
        pass.set_pipeline(&state.pipeline);
        pass.set_vertex_buffer(0, vertices.slice(..));
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

use wgpu::util::DeviceExt;

fn create_view_query(
    device: &RenderDevice,
    format: wgpu::TextureFormat,
    samples: u32,
    request: &WaterRayViewOcclusion,
) -> ViewQueryBuffers {
    let device = device.wgpu_device();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("water ray occlusion"), source: wgpu::ShaderSource::Wgsl("@vertex fn vertex(@location(0) position: vec4<f32>) -> @builtin(position) vec4<f32> { return position; }".into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("water ray occlusion"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                }],
            }],
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
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
        fragment: None,
        multiview_mask: None,
        cache: None,
    });
    ViewQueryBuffers {
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
        pipeline,
        samples,
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
                    let bytes = buffer.slice(..).get_mapped_range();
                    let count =
                        u64::from_ne_bytes(bytes[..8].try_into().expect("eight byte query buffer"));
                    *result
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                        Some((count as f32 / denominator).clamp(0.0, 1.0));
                    drop(bytes);
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
        commands.entity(entity).remove::<WaterRayGpuQuery>();
    }
}
