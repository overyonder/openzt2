use std::borrow::Cow;

#[cfg(feature = "2d")]
use bevy::core_pipeline::core_2d::Transparent2d;
#[cfg(feature = "3d")]
use bevy::core_pipeline::core_3d::{AlphaMask3d, Opaque3d, Transparent3d};
use bevy::{
    asset::uuid_handle,
    camera::visibility::VisibilitySystems,
    diagnostic::FrameCountPlugin,
    prelude::*,
    render::{
        extract_component::ExtractComponentPlugin,
        render_asset::prepare_assets,
        render_phase::DrawFunctions,
        render_resource::{SpecializedComputePipelines, SpecializedRenderPipelines},
        renderer::{RenderAdapterInfo, RenderDevice},
        texture::GpuImage,
        uniform::UniformComponentPlugin,
        view::prepare_view_uniforms,
        Render, RenderApp, RenderSystems,
    },
    time::{time_system, TimeSystems},
};

use crate::{asset::EffectAssetLoader, render::extract_main_view};
use crate::{
    asset::{DefaultMesh, EffectAsset},
    compile_effects,
    properties::EffectProperties,
    register_modifiers,
    render::{
        allocate_effects, allocate_events, allocate_metadata, allocate_parent_child_infos,
        allocate_properties, batch_effects, clear_previous_frame_resizes, extract_effect_events,
        extract_effects, extract_sim_params, fixup_parents, on_remove_cached_draw_indirect_args,
        on_remove_cached_effect, on_remove_cached_effect_events, on_remove_cached_metadata,
        on_remove_cached_properties, prepare_batch_inputs, prepare_bind_groups,
        prepare_effect_metadata, prepare_gpu_resources, prepare_indirect_pipeline,
        prepare_init_update_pipelines, prepare_particle_view_fog_bind_group,
        prepare_property_buffers, propagate_ready_state, publish_render_ready_state, queue_effects,
        queue_init_fill_dispatch_ops, queue_init_indirect_workgroup_update,
        queue_sort_fill_dispatch_ops, start_stop_gpu_debug_capture, update_mesh_locations, Batcher,
        DebugSettings, DispatchIndirectPipeline, DrawEffects, EffectAssetEvents, EffectBindGroups,
        EffectCache, EffectsMeta, EventCache, GpuBatchInfo, GpuBufferOperations, GpuSpawnerParams,
        HanabiRenderPlugin, InitFillDispatchQueue, ParticleDrawIndirectBuffer,
        ParticleEffectMetadataBuffer, ParticlesInitPipeline, ParticlesRenderPipeline,
        ParticlesUpdatePipeline, PrefixSumPipeline, PropertyBindGroups, PropertyCache,
        RenderDebugSettings, ShaderCache, SimParams, SortBindGroups, SortFillDispatchQueue,
        StorageType as _, UtilsPipeline,
    },
    render_feedback::{particle_render_feedback_channel, receive_particle_render_feedback},
    spawn::{self, Random},
    tick_spawners,
    time::effect_simulation_time_system,
    update_effect_collision_slots, update_properties_from_asset, EffectSimulation,
    EffectVisibilityClass, ParticleEffect, ParticleViewFog, SpawnerSettings,
};

/// Source code for the `vfx_sort` compute shader.
pub(crate) const VFX_SORT_WESL: Cow<'static, str> =
    Cow::Borrowed(include_str!("render/vfx_sort.wesl"));

/// Source code for the `vfx_sort_merge` compute shader.
pub(crate) const VFX_SORT_MERGE_WESL: Cow<'static, str> =
    Cow::Borrowed(include_str!("render/vfx_sort_merge.wesl"));

/// Labels for the Hanabi systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum EffectSystems {
    /// Tick all effect instances to generate particle spawn counts.
    ///
    /// This system runs during the [`PostUpdate`] schedule. Any system which
    /// modifies an effect spawner should run before this set to ensure the
    /// spawner takes into account the newly set values during its ticking.
    TickSpawners,

    /// Compile the effect instances, updating the [`CompiledParticleEffect`]
    /// components.
    ///
    /// This system runs during the [`PostUpdate`] schedule. This is largely an
    /// internal task which can be ignored by most users.
    ///
    /// [`CompiledParticleEffect`]: crate::CompiledParticleEffect
    CompileEffects,

    /// Update the properties of the effect instance based on the declared
    /// properties in the [`EffectAsset`], updating the associated
    /// [`EffectProperties`] component.
    ///
    /// This system runs during the [`PostUpdate`] schedule, after the assets
    /// have been updated. Any system which modifies an [`EffectAsset`]'s
    /// declared properties should run before this set in order for changes to
    /// be taken into account in the same frame.
    UpdatePropertiesFromAsset,

    /// Prepare effect assets for the extracted effects.
    ///
    /// Part of Bevy's own [`RenderSystems::PrepareAssets`].
    PrepareEffectAssets,

    /// Queue the GPU commands for the extracted effects.
    ///
    /// Part of Bevy's own [`RenderSystems::Queue`].
    QueueEffects,

    /// Prepare GPU data for the queued effects.
    ///
    /// Part of Bevy's own [`RenderSystems::PrepareResources`].
    PrepareEffectGpuResources,

    /// Prepare the GPU bind groups once all buffers have been (re-)allocated
    /// and won't change this frame.
    ///
    /// Part of Bevy's own [`RenderSystems::PrepareBindGroups`].
    PrepareBindGroups,
}

const HANABI_COMMON_TEMPLATE_HANDLE: Handle<Shader> =
    uuid_handle!("626E7AD3-4E54-487E-B796-9A90E34CC1EC");

/// Plugin to add systems related to Hanabi.
#[derive(Debug, Clone, Copy)]
pub struct HanabiPlugin;

impl HanabiPlugin {
    /// Create the `vfx_common.wesl` shader with proper alignment.
    ///
    /// This creates a new [`Shader`] from the `vfx_common.wesl` template file,
    /// by applying the given alignment for storage buffers. This produces a
    /// shader ready for the specific GPU device associated with that
    /// alignment.
    pub(crate) fn make_common_shader(min_storage_buffer_offset_alignment: u32) -> Shader {
        let spawner_padding_code =
            GpuSpawnerParams::padding_code(min_storage_buffer_offset_alignment);
        let batch_info_padding_code =
            GpuBatchInfo::padding_code(min_storage_buffer_offset_alignment);
        let common_code = include_str!("render/vfx_common.wesl")
            .replace("{{SPAWNER_PADDING}}", &spawner_padding_code)
            .replace("{{BATCH_INFO_PADDING}}", &batch_info_padding_code);
        Shader::from_wesl(common_code, "embedded://bevy_hanabi/render/vfx_common.wesl")
    }

    /// Create the `vfx_indirect.wesl` shader with proper alignment.
    ///
    /// This creates a new [`Shader`] from the `vfx_indirect.wesl` template
    /// file, by applying the given alignment for storage buffers. This
    /// produces a shader ready for the specific GPU device associated with
    /// that alignment.
    pub(crate) fn make_indirect_shader(
        min_storage_buffer_offset_alignment: u32,
        has_events: bool,
    ) -> Shader {
        let indirect_code = include_str!("render/vfx_indirect.wesl");
        Shader::from_wesl(
            indirect_code,
            embedded_path(&format!(
                "vfx_indirect_{}_{}",
                min_storage_buffer_offset_alignment,
                if has_events { "events" } else { "noevent" },
            )),
        )
    }

    /// Create the `vfx_prefix_sum.wesl` shader.
    ///
    /// This creates a new [`Shader`] from the `vfx_prefix_sum.wesl` template
    /// file.
    pub(crate) fn make_prefix_sum_shader() -> Shader {
        let prefix_sum_code = include_str!("render/vfx_prefix_sum.wesl");
        Shader::from_wesl(prefix_sum_code, embedded_path("vfx_prefix_sum"))
    }
}

/// Path of one of Hanabi's built-in shaders.
///
/// [`Shader::from_wesl`] derives a shader's module path from the path it is
/// given. Under `embedded://` that is `bevy_hanabi::render::<name>`, the same
/// for every build; a path from `file!()` would be wherever the source was
/// compiled, which for a git or registry dependency is an absolute path on the
/// build machine.
fn embedded_path(name: &str) -> String {
    format!("embedded://bevy_hanabi/render/{name}.wesl")
}

impl Plugin for HanabiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameCountPlugin>() {
            app.add_plugins(FrameCountPlugin);
        }
        // Register modifiers
        {
            let type_registry = app.world().resource::<AppTypeRegistry>();
            register_modifiers(type_registry);
        }

        // Register asset
        app.init_asset::<EffectAsset>()
            .insert_resource(Random(spawn::new_rng()))
            .add_plugins(HanabiRenderPlugin)
            .add_plugins(ExtractComponentPlugin::<EffectVisibilityClass>::default())
            .add_plugins((
                ExtractComponentPlugin::<ParticleViewFog>::default(),
                UniformComponentPlugin::<ParticleViewFog>::default(),
            ))
            .init_resource::<DefaultMesh>()
            .init_resource::<ShaderCache>()
            .init_resource::<DebugSettings>()
            .init_resource::<Time<EffectSimulation>>()
            .init_resource::<crate::EffectParticleBudget>()
            .configure_sets(
                PostUpdate,
                (
                    EffectSystems::TickSpawners
                        // This checks the visibility to skip work, so needs to run after
                        // ComputedVisibility was updated.
                        .after(VisibilitySystems::VisibilityPropagate),
                    EffectSystems::CompileEffects,
                ),
            )
            .configure_sets(
                PreUpdate,
                EffectSystems::UpdatePropertiesFromAsset.after(bevy::asset::AssetTrackingSystems),
            )
            .add_systems(
                First,
                effect_simulation_time_system
                    .after(time_system)
                    .in_set(TimeSystems),
            )
            .add_systems(
                PostUpdate,
                (
                    tick_spawners.in_set(EffectSystems::TickSpawners),
                    (
                        compile_effects,
                        update_effect_collision_slots,
                        crate::render::authored_draw::compile_effect_render_passes,
                    )
                        .in_set(EffectSystems::CompileEffects),
                    update_properties_from_asset.in_set(EffectSystems::UpdatePropertiesFromAsset),
                ),
            );

        app.init_asset_loader::<EffectAssetLoader>();

        // Register types with reflection
        app.register_type::<EffectAsset>()
            .register_type::<ParticleEffect>()
            .register_type::<EffectProperties>()
            .register_type::<SpawnerSettings>()
            .register_type::<Time<EffectSimulation>>();
    }

    fn finish(&self, app: &mut App) {
        let render_device = app
            .sub_app(RenderApp)
            .world()
            .resource::<RenderDevice>()
            .clone();

        let adapter_name = app
            .world()
            .get_resource::<RenderAdapterInfo>()
            .map(|ai| &ai.name[..])
            .unwrap_or("<unknown>");

        // Check device limits
        let limits = render_device.limits();
        if limits.max_bind_groups < 4 {
            error!("Hanabi requires a GPU device supporting at least 4 bind groups (Limits::max_bind_groups).\n  Current adapter: {}\n  Supported bind groups: {}", adapter_name, limits.max_bind_groups);
            return;
        } else {
            info!("Initializing Hanabi for GPU adapter {}", adapter_name);
        }

        // Insert the properly aligned `vfx_common.wesl` shader into
        // Assets<Shader>, so that the automated Bevy shader processing
        // finds it as an import. This is used for init/update/render
        // shaders (but not the indirect one).
        {
            let common_shader = HanabiPlugin::make_common_shader(
                render_device.limits().min_storage_buffer_offset_alignment,
            );
            let mut assets = app.world_mut().resource_mut::<Assets<Shader>>();
            assets
                .insert(&HANABI_COMMON_TEMPLATE_HANDLE, common_shader)
                .unwrap();
        }

        // Insert the two variants of the properly aligned `vfx_indirect.wesl`
        // shaders into Assets<Shader>.
        let (
            indirect_shader_noevent,
            indirect_shader_events,
            prefix_sum_shader,
            sort_fill_shader,
            sort_shader,
            sort_merge_shader,
            sort_copy_shader,
        ) = {
            let align = render_device.limits().min_storage_buffer_offset_alignment;
            let indirect_shader_noevent = HanabiPlugin::make_indirect_shader(align, false);
            let indirect_shader_events = HanabiPlugin::make_indirect_shader(align, true);
            let prefix_sum_shader = HanabiPlugin::make_prefix_sum_shader();
            let sort_fill_shader = Shader::from_wesl(
                include_str!("render/vfx_sort_fill.wesl"),
                embedded_path("vfx_sort_fill"),
            );
            let sort_shader = Shader::from_wesl(VFX_SORT_WESL, embedded_path("vfx_sort"));
            let sort_merge_shader =
                Shader::from_wesl(VFX_SORT_MERGE_WESL, embedded_path("vfx_sort_merge"));
            let sort_copy_shader = Shader::from_wesl(
                include_str!("render/vfx_sort_copy.wesl"),
                embedded_path("vfx_sort_copy"),
            );

            let mut assets = app.world_mut().resource_mut::<Assets<Shader>>();
            let indirect_shader_noevent = assets.add(indirect_shader_noevent);
            let indirect_shader_events = assets.add(indirect_shader_events);
            let prefix_sum_shader = assets.add(prefix_sum_shader);
            let sort_fill_shader = assets.add(sort_fill_shader);
            let sort_shader = assets.add(sort_shader);
            let sort_merge_shader = assets.add(sort_merge_shader);
            let sort_copy_shader = assets.add(sort_copy_shader);
            assets
                .insert(
                    &crate::render::distance_sort::DISTANCE_SORT_SHADER,
                    Shader::from_wgsl(
                        include_str!("render/vfx_distance_sort.wgsl"),
                        "hanabi/distance_sort.wgsl",
                    ),
                )
                .unwrap();

            (
                indirect_shader_noevent,
                indirect_shader_events,
                prefix_sum_shader,
                sort_fill_shader,
                sort_shader,
                sort_merge_shader,
                sort_copy_shader,
            )
        };

        let effects_meta = EffectsMeta::new(
            render_device.clone(),
            indirect_shader_noevent,
            indirect_shader_events,
            prefix_sum_shader,
        );

        let effect_cache = EffectCache::new(render_device.clone());
        let property_cache = PropertyCache::new(&render_device);
        let event_cache = EventCache::new(render_device);
        let (feedback_sender, feedback_receiver) = particle_render_feedback_channel();
        app.insert_resource(feedback_receiver)
            .add_systems(PreUpdate, receive_particle_render_feedback);

        let render_app = app.sub_app_mut(RenderApp);
        let sort_bind_groups = SortBindGroups::new(
            render_app.world_mut(),
            sort_fill_shader,
            sort_shader,
            sort_merge_shader,
            sort_copy_shader,
        );

        // Register the custom render pipeline
        render_app
            .insert_resource(effects_meta)
            .insert_resource(feedback_sender)
            .init_resource::<ParticleDrawIndirectBuffer>()
            .init_resource::<ParticleEffectMetadataBuffer>()
            .insert_resource(effect_cache)
            .insert_resource(property_cache)
            .insert_resource(event_cache)
            .init_resource::<RenderDebugSettings>()
            .init_resource::<EffectBindGroups>()
            .init_resource::<PropertyBindGroups>()
            .init_resource::<InitFillDispatchQueue>()
            .init_resource::<SortFillDispatchQueue>()
            .insert_resource(sort_bind_groups)
            .init_resource::<UtilsPipeline>()
            .init_resource::<GpuBufferOperations>()
            .init_resource::<PrefixSumPipeline>()
            .init_resource::<DispatchIndirectPipeline>()
            .init_resource::<SpecializedComputePipelines<DispatchIndirectPipeline>>()
            .init_resource::<ParticlesInitPipeline>()
            .init_resource::<SpecializedComputePipelines<ParticlesInitPipeline>>()
            .init_resource::<ParticlesInitPipeline>()
            .init_resource::<SpecializedComputePipelines<ParticlesInitPipeline>>()
            .init_resource::<ParticlesUpdatePipeline>()
            .init_resource::<SpecializedComputePipelines<ParticlesUpdatePipeline>>()
            .init_resource::<ParticlesRenderPipeline>()
            .init_resource::<crate::render::distance_sort::ParticleDistanceSortPipeline>()
            .init_resource::<SpecializedRenderPipelines<ParticlesRenderPipeline>>()
            .init_resource::<EffectAssetEvents>()
            .init_resource::<SimParams>()
            .init_resource::<Batcher>()
            .configure_sets(
                Render,
                (
                    EffectSystems::PrepareEffectAssets.in_set(RenderSystems::PrepareAssets),
                    EffectSystems::QueueEffects.in_set(RenderSystems::Queue),
                    EffectSystems::PrepareEffectGpuResources
                        .in_set(RenderSystems::PrepareResources),
                    EffectSystems::PrepareBindGroups.in_set(RenderSystems::PrepareBindGroups),
                ),
            )
            .edit_schedule(ExtractSchedule, |schedule| {
                schedule.add_systems((
                    start_stop_gpu_debug_capture,
                    extract_effects,
                    extract_sim_params,
                    extract_effect_events,
                    extract_main_view.after(bevy::render::camera::extract_cameras),
                ));
            })
            .add_systems(
                Render,
                (
                    (
                        clear_previous_frame_resizes,
                        allocate_effects,
                        allocate_events,
                        allocate_properties,
                        update_mesh_locations
                            .after(clear_previous_frame_resizes)
                            .after(bevy::render::mesh::allocator::allocate_and_free_meshes)
                            .after(prepare_assets::<bevy::render::mesh::RenderMesh>),
                        allocate_metadata.after(clear_previous_frame_resizes),
                        allocate_parent_child_infos
                            .after(allocate_effects)
                            .after(allocate_events),
                        fixup_parents.after(allocate_parent_child_infos),
                        prepare_init_update_pipelines
                            .after(allocate_properties)
                            .after(fixup_parents),
                        prepare_indirect_pipeline.after(allocate_parent_child_infos),
                        propagate_ready_state.after(prepare_init_update_pipelines),
                        publish_render_ready_state
                            .after(propagate_ready_state)
                            .after(update_mesh_locations)
                            .after(prepare_assets::<GpuImage>),
                        prepare_batch_inputs
                            .after(clear_previous_frame_resizes)
                            .after(allocate_metadata)
                            .after(update_mesh_locations)
                            .after(propagate_ready_state),
                        batch_effects.after(prepare_batch_inputs),
                    )
                        .in_set(EffectSystems::PrepareEffectAssets),
                    // Once batched, queue the effects/batches which are ready to be
                    // updated/rendered this frame.
                    crate::render::authored_draw::initialize_particle_draw_views
                        .in_set(EffectSystems::QueueEffects)
                        .before(queue_effects),
                    queue_effects
                        .in_set(EffectSystems::QueueEffects)
                        .after(batch_effects),
                    // Queue the dispatch ops to fill the indirect dispatch args of the init pass
                    // of child effects.
                    queue_init_indirect_workgroup_update
                        .in_set(EffectSystems::QueueEffects)
                        .after(batch_effects)
                        .after(fixup_parents),
                    prepare_gpu_resources
                        .in_set(EffectSystems::PrepareEffectGpuResources)
                        // This creates the bind group for the view
                        .after(prepare_view_uniforms)
                        // Upload and optionally resize the draw indirect args buffer
                        .after(update_mesh_locations)
                        // Bind groups depend on buffers being re-/allocated
                        .before(prepare_bind_groups),
                    prepare_property_buffers
                        .in_set(EffectSystems::PrepareEffectGpuResources)
                        .before(prepare_bind_groups),
                    prepare_effect_metadata
                        .in_set(EffectSystems::PrepareEffectGpuResources)
                        //
                        .after(allocate_effects)
                        // Need the draw indirect args to be allocated
                        .after(update_mesh_locations)
                        // Need the local/global/base child index
                        .after(fixup_parents)
                        // Need the indirect dispatch args index for GPU event based init pass
                        .after(allocate_events)
                        // Need the properties block offset in GPU slab
                        .after(allocate_properties)
                        // This may invalidate some bind groups when resizing the metadata buffer
                        .before(prepare_bind_groups),
                    // Queue the dispatch ops to fill the indirect dispatch args of the ribbon
                    // particle sort pass. Deferred from batch_effects() so it runs after the
                    // effect metadata buffer has been (re-)allocated to this frame's size.
                    queue_sort_fill_dispatch_ops
                        .in_set(EffectSystems::PrepareEffectGpuResources)
                        // The prefix sum buffer is allocated and uploaded while batching.
                        .after(batch_effects)
                        // Need the metadata buffer (re-)allocated so the captured handle and
                        // dynamic offsets are correct and in-bounds
                        .after(prepare_effect_metadata)
                        // Keep this aligned with other queue submissions that capture GPU buffers.
                        .after(prepare_gpu_resources)
                        // Must submit into the shared GpuBufferOperations before
                        // queue_init_fill_dispatch_ops uploads its args buffer (end_frame)
                        .before(queue_init_fill_dispatch_ops)
                        .before(prepare_bind_groups),
                    queue_init_fill_dispatch_ops
                        .in_set(EffectSystems::PrepareEffectGpuResources)
                        .after(prepare_gpu_resources)
                        .before(prepare_bind_groups),
                    // Prepare the bind groups
                    prepare_bind_groups
                        .in_set(EffectSystems::PrepareBindGroups)
                        .after(queue_effects)
                        .after(prepare_assets::<GpuImage>),
                    prepare_particle_view_fog_bind_group
                        .in_set(EffectSystems::PrepareBindGroups)
                        .after(prepare_gpu_resources),
                ),
            );

        // Register observers to deallocate GPU resources
        {
            let world = render_app.world_mut();
            world.add_observer(on_remove_cached_effect);
            world.add_observer(on_remove_cached_metadata);
            world.add_observer(on_remove_cached_draw_indirect_args);
            world.add_observer(on_remove_cached_effect_events);
            world.add_observer(crate::render::on_remove_cached_parent_info);
            world.add_observer(on_remove_cached_properties);
        }

        // Register the draw function for drawing the particles. This will be
        // called during the main 2D/3D pass, at the Transparent2d/3d
        // phase, after the opaque objects have been rendered (or,
        // rather, commands for those have been recorded).
        #[cfg(feature = "2d")]
        {
            let draw_particles = DrawEffects::new(render_app.world_mut());
            render_app
                .world()
                .get_resource::<DrawFunctions<Transparent2d>>()
                .unwrap()
                .write()
                .add(draw_particles);
        }
        #[cfg(feature = "3d")]
        {
            let draw_particles = DrawEffects::new(render_app.world_mut());
            render_app
                .world()
                .get_resource::<DrawFunctions<Transparent3d>>()
                .unwrap()
                .write()
                .add(draw_particles);

            let draw_particles = DrawEffects::new(render_app.world_mut());
            render_app
                .world()
                .get_resource::<DrawFunctions<AlphaMask3d>>()
                .unwrap()
                .write()
                .add(draw_particles);

            let draw_particles = DrawEffects::new(render_app.world_mut());
            render_app
                .world()
                .get_resource::<DrawFunctions<Opaque3d>>()
                .unwrap()
                .write()
                .add(draw_particles);
        }
    }
}
