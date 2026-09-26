use crate::assets::material::runtime::resolved_effect_transform_bindings::EffectRenderViewTransforms;
use bevy::platform::collections::HashSet;
use bevy::{
    asset::AssetEvent,
    camera::visibility::RenderLayers,
    gltf::GltfMaterialName,
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{morph::MeshMorphWeights, skinning::SkinnedMesh},
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
    render::storage::ShaderBuffer,
};

use crate::{
    assets::material::{
        material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
    },
    plugins::camera::camera_runtime_state_types::ZooCamera,
    plugins::environment::environment_presentation_types::EnvironmentLight,
    plugins::environment::environment_state_types::AuthoredWindShaderPresentationState,
    plugins::settings::graphics_settings_types::GraphicsSettings,
    plugins::world_spawn::prefab_presentation_types::PrefabMaterialOverrides,
};

use super::prefab_render_layer_camera_matching::render_layers_for_prefab_entity_and_ancestor_chain;
use super::ModelExpanded;
use crate::plugins::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;

#[derive(Default)]
pub(super) struct MaterialHotLoopMeasurements {
    calls: u32,
    total_milliseconds: f64,
    worst_milliseconds: f64,
    visited_items: u64,
    changed_items: u64,
}

#[derive(Component)]
pub(super) struct AuthoredModelMaterialPassesProjected;

#[derive(Component)]
pub(super) struct AdditionalAuthoredModelMaterialPass;

#[derive(Component)]
pub(super) struct PerDrawEffectPassMaterial;

#[derive(Component)]
pub(super) struct ProgrammableEffectPassMaterial;

pub(crate) const PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER: usize = 5;
pub(crate) const WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER: usize = 6;
pub(crate) const WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER: usize = 7;

#[derive(Component, Clone, Copy)]
pub(crate) struct EffectPassMaterialRenderView {
    pub(crate) render_layer: usize,
}

#[derive(Component)]
pub(super) struct EffectPassMaterialRenderViewProjection {
    source: Entity,
    view: Entity,
    source_material: Handle<EffectPassMaterial>,
}

#[derive(Component)]
pub(super) struct EffectPassMaterialSourceRenderLayers(pub(super) RenderLayers);

pub(super) fn project_authored_model_material_passes(
    mut commands: Commands,
    roots: Query<
        (Entity, &PrefabMaterialOverrides),
        (
            With<ModelExpanded>,
            Without<AuthoredModelMaterialPassesProjected>,
        ),
    >,
    children: Query<&Children>,
    primitives: Query<(
        Entity,
        &GltfMaterialName,
        &Mesh3d,
        Option<&RenderLayers>,
        Has<NotShadowCaster>,
        Has<NotShadowReceiver>,
        Option<&SkinnedMesh>,
        Option<&MeshMorphWeights>,
        &GlobalTransform,
    )>,
    materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut writable_effect_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    cameras: Query<
        (
            &Camera,
            &GlobalTransform,
            &Projection,
            Option<&RenderLayers>,
        ),
        With<Camera3d>,
    >,
    hierarchy: Query<(Option<&ChildOf>, Option<&RenderLayers>)>,
) {
    for (root, overrides) in &roots {
        let source_parent = hierarchy
            .get(root)
            .ok()
            .and_then(|(parent, _)| parent.map(ChildOf::parent))
            .unwrap_or(root);
        let source_index = children
            .get(source_parent)
            .ok()
            .and_then(|children| children.iter().position(|child| child == root))
            .unwrap_or(0);
        let render_layers = render_layers_for_prefab_entity_and_ancestor_chain(root, &hierarchy);
        let Some((_, camera_transform, camera_projection, _)) =
            cameras.iter().find(|(camera, _, _, camera_layers)| {
                camera.is_active && camera_layers.unwrap_or_default().intersects(&render_layers)
            })
        else {
            continue;
        };
        let view_transforms =
            EffectRenderViewTransforms::from_camera(camera_transform, camera_projection);
        if std::iter::once(root)
            .chain(children.iter_descendants_depth_first::<Children>(root))
            .all(|entity| !primitives.contains(entity))
        {
            continue;
        }
        if overrides.0.iter().any(|material_override| {
            material_override
                .handle
                .as_ref()
                .is_none_or(|handle| materials.get(handle).is_none())
        }) {
            continue;
        }
        let mut matched_primitive_count = 0_usize;
        let all_matched_effect_pass_materials_are_loaded = std::iter::once(root)
            .chain(children.iter_descendants_depth_first::<Children>(root))
            .filter_map(|entity| primitives.get(entity).ok())
            .filter_map(|(_, name, _, _, _, _, _, _, _)| {
                overrides
                    .0
                    .iter()
                    .find(|material_override| {
                        name.0.eq_ignore_ascii_case(
                            &material_override.material.to_lowercase_hexadecimal_string(),
                        )
                    })
                    .and_then(|material_override| material_override.handle.as_ref())
                    .map(|handle| {
                        matched_primitive_count += 1;
                        handle
                    })
            })
            .all(|handle| {
                materials.get(handle).is_some_and(|material| {
                    material
                        .evaluated_pass_material_assets_for_effect_quality(
                            graphics_settings.effects,
                        )
                        .iter()
                        .all(|pass| writable_effect_materials.get(pass).is_some())
                })
            });
        if !all_matched_effect_pass_materials_are_loaded
            || (!overrides.0.is_empty() && matched_primitive_count == 0)
        {
            continue;
        }
        for entity in
            std::iter::once(root).chain(children.iter_descendants_depth_first::<Children>(root))
        {
            let Ok((
                entity,
                name,
                mesh,
                layers,
                no_shadow_cast,
                no_shadow_receive,
                skin,
                morph,
                transform,
            )) = primitives.get(entity)
            else {
                continue;
            };
            let Some(handle) = overrides
                .0
                .iter()
                .find(|material_override| {
                    name.0.eq_ignore_ascii_case(
                        &material_override.material.to_lowercase_hexadecimal_string(),
                    )
                })
                .and_then(|material_override| material_override.handle.as_ref())
            else {
                continue;
            };
            let Some(material) = materials.get(handle) else {
                continue;
            };
            let mut target = commands.entity(entity);
            let projected_passes = material
                .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                .iter()
                .filter_map(|handle| {
                    let template = writable_effect_materials.get(handle)?;
                    // Fixed-function transforms use Bevy's mesh/view bindings.
                    // Programmable wind/light-only bindings are also shared;
                    // their presence does not make their values instance-local.
                    if !template.requires_draw_specific_shader_registers() {
                        return Some(handle.clone());
                    }
                    let mut projected = template.clone();
                    projected.bind_per_draw_transform_semantics(
                        transform.to_matrix(),
                        &view_transforms,
                        true,
                        true,
                    );
                    projected.allocate_programmable_float_register_buffers_for_model_view(
                        &mut effect_uniform_buffers,
                    );
                    Some(writable_effect_materials.add(projected))
                })
                .collect::<Vec<_>>();
            let Some(first_pass) = projected_passes.first().cloned() else {
                target.insert(Visibility::Hidden);
                continue;
            };
            if writable_effect_materials
                .get(&first_pass)
                .is_some_and(EffectPassMaterial::has_programmable_effect_parameter_bindings)
            {
                target.insert(ProgrammableEffectPassMaterial);
            } else {
                target.remove::<ProgrammableEffectPassMaterial>();
            }
            target
                .insert(Visibility::Inherited)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert((
                    MeshMaterial3d::<EffectPassMaterial>(first_pass),
                    PerDrawEffectPassMaterial,
                    AuthoredEffectTechniquePassSubmissionOrder::new(entity, 0)
                        .expect("the first authored effect pass index fits u16")
                        .with_source_hierarchy_order(source_parent, source_index),
                ));
            for (pass_index, pass) in projected_passes.iter().enumerate().skip(1) {
                let mut additional_pass = commands.spawn((
                    mesh.clone(),
                    MeshMaterial3d::<EffectPassMaterial>(pass.clone()),
                    Transform::default(),
                    Visibility::Inherited,
                    ChildOf(entity),
                    AdditionalAuthoredModelMaterialPass,
                    PerDrawEffectPassMaterial,
                    AuthoredEffectTechniquePassSubmissionOrder::new(entity, pass_index)
                        .expect("the authored effect pass index fits u16")
                        .with_source_hierarchy_order(source_parent, source_index),
                ));
                if writable_effect_materials
                    .get(pass)
                    .is_some_and(EffectPassMaterial::has_programmable_effect_parameter_bindings)
                {
                    additional_pass.insert(ProgrammableEffectPassMaterial);
                }
                if let Some(layers) = layers {
                    additional_pass.insert(layers.clone());
                }
                if no_shadow_cast {
                    additional_pass.insert(NotShadowCaster);
                }
                if no_shadow_receive {
                    additional_pass.insert(NotShadowReceiver);
                }
                if let Some(skin) = skin {
                    additional_pass.insert(skin.clone());
                }
                if let Some(morph) = morph {
                    additional_pass.insert(morph.clone());
                }
            }
        }
        commands
            .entity(root)
            .insert(AuthoredModelMaterialPassesProjected);
    }
}

// Each draw needs its camera, lights and render-view transforms together.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_per_draw_effect_transform_semantics(
    mut hot_loop_measurements: Local<MaterialHotLoopMeasurements>,
    mut prepared_views: Local<Vec<(Entity, EffectRenderViewTransforms)>>,
    camera: Single<
        (
            Ref<GlobalTransform>,
            Ref<Projection>,
            Option<Ref<DistanceFog>>,
        ),
        With<ZooCamera>,
    >,
    ambient_light: Res<GlobalAmbientLight>,
    directional_lights: Query<(
        Ref<DirectionalLight>,
        Ref<GlobalTransform>,
        Ref<ViewVisibility>,
        Ref<EnvironmentLight>,
    )>,
    wind: Single<Ref<AuthoredWindShaderPresentationState>>,
    mut effect_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    render_views: Query<
        (Entity, Ref<GlobalTransform>, Ref<Projection>),
        With<EffectPassMaterialRenderView>,
    >,
    renderables: Query<
        (
            Ref<GlobalTransform>,
            Ref<MeshMaterial3d<EffectPassMaterial>>,
            Ref<PerDrawEffectPassMaterial>,
            Ref<ViewVisibility>,
            Option<&EffectPassMaterialRenderViewProjection>,
        ),
        With<ProgrammableEffectPassMaterial>,
    >,
) {
    let measurement_started =
        bevy::log::tracing::enabled!(target: "openzt2_material_hot_loops", bevy::log::Level::INFO)
            .then(std::time::Instant::now);
    let mut scanned = 0_u64;
    let mut updated = 0_u64;
    let (camera_transform, camera_projection, distance_fog) = camera.into_inner();
    let fog_changed = distance_fog.as_ref().is_some_and(|fog| fog.is_changed());
    let wind_changed = wind.is_changed();
    let light_changed = ambient_light.is_changed()
        || directional_lights
            .iter()
            .any(|(light, transform, visibility, authored_light)| {
                light.is_changed()
                    || transform.is_changed()
                    || visibility.is_changed()
                    || authored_light.is_changed()
            });
    let primary_view =
        EffectRenderViewTransforms::from_camera(&camera_transform, &camera_projection);
    prepared_views.clear();
    prepared_views.extend(render_views.iter().map(|(entity, transform, projection)| {
        (
            entity,
            EffectRenderViewTransforms::from_camera(&transform, &projection),
        )
    }));
    let mut light_rig = [Vec4::ZERO; 7];
    for (light, transform, visibility, authored_light) in &directional_lights {
        let Some(light_index) = authored_light.kind_rank.checked_sub(1).map(usize::from) else {
            continue;
        };
        if !visibility.get() || light_index >= 3 {
            continue;
        }
        let mut color = Vec4::from_array(light.color.to_linear().to_f32_array());
        color *= light.illuminance / light_consts::lux::DIRECT_SUNLIGHT;
        color.w = 1.0;
        light_rig[light_index] = color;
        light_rig[light_index + 3] = Vec3::from(transform.back()).extend(1.0);
    }
    light_rig[6] =
        Vec4::from_array(ambient_light.color.to_linear().to_f32_array()) * ambient_light.brightness;
    light_rig[6].w = 1.0;
    let wind_registers = wind.shader_registers();
    for (transform, material_handle, projected_marker, visibility, view_projection) in &renderables
    {
        scanned += 1;
        if !visibility.get() {
            continue;
        }
        let (transform, view_transform, projection, prepared_view) =
            if let Some(view_projection) = view_projection {
                let (Ok(source_transform), Ok((_, view_transform, projection))) = (
                    renderables
                        .get(view_projection.source)
                        .map(|source| source.0),
                    render_views.get(view_projection.view),
                ) else {
                    continue;
                };
                let Some((_, prepared)) = prepared_views
                    .iter()
                    .find(|(entity, _)| *entity == view_projection.view)
                else {
                    continue;
                };
                (source_transform, view_transform, projection, prepared)
            } else {
                (
                    transform,
                    camera_transform,
                    camera_projection,
                    &primary_view,
                )
            };
        let force = material_handle.is_changed()
            || visibility.is_changed()
            || projected_marker.is_changed();
        let model_changed = force || transform.is_changed();
        let view_changed = force || view_transform.is_changed() || projection.is_changed();
        // Only CPU register values change here. Buffer handles and pipeline
        // state stay fixed; ShaderBuffer assets publish the GPU updates.
        // Emitting Material::Modified would unnecessarily rebuild bind groups.
        let Some(material) = effect_materials
            .get_mut_untracked(&material_handle.0)
            .filter(|material| material.has_programmable_effect_parameter_bindings())
        else {
            continue;
        };
        if !force
            && !material.programmable_inputs_changed(
                model_changed,
                view_changed,
                wind_changed,
                light_changed,
                fog_changed,
            )
        {
            continue;
        }
        if model_changed || view_changed {
            material.bind_per_draw_transform_semantics(
                transform.to_matrix(),
                prepared_view,
                model_changed,
                view_changed,
            );
        }
        let fog_parameters = distance_fog
            .as_ref()
            .filter(|_| force || fog_changed || view_changed)
            .and_then(|fog| {
                effect_fog_shader_registers_for_render_view(fog, &view_transform, &projection)
            });
        material.bind_changed_environment_registers(
            (force || wind_changed).then_some(wind_registers.as_slice()),
            (force || light_changed).then_some(light_rig.as_slice()),
            fog_parameters.as_ref().map(|values| values.as_slice()),
        );
        material
            .write_programmable_float_registers_to_persistent_buffers(&mut effect_uniform_buffers);
        updated += 1;
    }
    report_material_hot_loop_measurements(
        "update_per_draw_effect_transform_semantics",
        measurement_started,
        scanned,
        updated,
        &mut hot_loop_measurements,
    );
}

#[allow(clippy::type_complexity)]
pub(super) fn project_effect_pass_material_instances_for_additional_render_views(
    mut hot_loop_measurements: Local<MaterialHotLoopMeasurements>,
    mut invalidation: super::material_render_view_invalidation::MaterialRenderViewInvalidation,
    mut candidates: Local<Vec<Entity>>,
    mut changed_assets: Local<HashSet<bevy::asset::AssetId<EffectPassMaterial>>>,
    mut prepared_views: Local<Vec<(Entity, usize, EffectRenderViewTransforms)>>,
    mut commands: Commands,
    render_views: Query<(
        Entity,
        &GlobalTransform,
        &Projection,
        &EffectPassMaterialRenderView,
    )>,
    sources: Query<
        (
            Entity,
            &GlobalTransform,
            &Mesh3d,
            &MeshMaterial3d<EffectPassMaterial>,
            Option<&RenderLayers>,
            Option<&EffectPassMaterialSourceRenderLayers>,
            Has<NotShadowCaster>,
            Has<NotShadowReceiver>,
            Option<&SkinnedMesh>,
            Option<&MeshMorphWeights>,
            Option<&AuthoredEffectTechniquePassSubmissionOrder>,
        ),
        (
            With<PerDrawEffectPassMaterial>,
            Without<EffectPassMaterialRenderViewProjection>,
        ),
    >,
    projections: Query<(Entity, &EffectPassMaterialRenderViewProjection)>,
    source_children: Query<&Children>,
    changed_geometry: Query<
        (),
        Or<(
            Changed<Mesh3d>,
            Changed<SkinnedMesh>,
            Changed<MeshMorphWeights>,
            Changed<AuthoredEffectTechniquePassSubmissionOrder>,
        )>,
    >,
    mut effect_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let measurement_started =
        bevy::log::tracing::enabled!(target: "openzt2_material_hot_loops", bevy::log::Level::INFO)
            .then(std::time::Instant::now);
    let mut scanned = 0_u64;
    let mut created = 0_u64;
    let (sweep_orphans, views_changed) =
        invalidation.collect_affected_sources(&mut candidates, &mut changed_assets);
    if sweep_orphans {
        for (entity, projection) in &projections {
            if !sources.contains(projection.source) || !render_views.contains(projection.view) {
                commands.entity(entity).despawn();
            }
        }
    }
    prepared_views.clear();
    if !candidates.is_empty() {
        prepared_views.extend(render_views.iter().map(
            |(entity, transform, projection, context)| {
                (
                    entity,
                    context.render_layer,
                    EffectRenderViewTransforms::from_camera(transform, projection),
                )
            },
        ));
    }
    for source_entity in candidates.iter().copied() {
        let Ok((
            source,
            source_transform,
            mesh,
            source_material,
            render_layers,
            saved_render_layers,
            no_shadow_cast,
            no_shadow_receive,
            skin,
            morph,
            pass_submission_order,
        )) = sources.get(source_entity)
        else {
            continue;
        };
        scanned += 1;
        let source_render_layers = saved_render_layers.map_or_else(
            || render_layers.cloned().unwrap_or_default(),
            |saved| saved.0.clone(),
        );
        if !source_render_layers.iter().any(|layer| layer == 0) {
            continue;
        }
        let Some(needs_view_uniforms) = effect_materials
            .get(&source_material.0)
            .map(EffectPassMaterial::requires_view_specific_shader_registers)
        else {
            continue;
        };
        if !needs_view_uniforms {
            // Every world camera includes layer zero. Draws with no camera
            // uniforms use their existing model/global registers in every view,
            // without another mesh entity or bind group. Restore this path when
            // a quality change or asset reload removes camera-dependent inputs.
            if saved_render_layers.is_some() {
                commands
                    .entity(source)
                    .insert(source_render_layers)
                    .remove::<EffectPassMaterialSourceRenderLayers>();
            }
            if let Ok(children) = source_children.get(source) {
                for child in children.iter() {
                    if projections
                        .get(child)
                        .is_ok_and(|(_, projection)| projection.source == source)
                    {
                        commands.entity(child).despawn();
                    }
                }
            }
            continue;
        }
        if changed_assets.contains(&source_material.0.id()) {
            commands.entity(source).insert(PerDrawEffectPassMaterial);
        }
        if saved_render_layers.is_none() {
            commands.entity(source).insert((
                EffectPassMaterialSourceRenderLayers(source_render_layers.clone()),
                source_render_layers
                    .clone()
                    .without(0)
                    .with(PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER),
            ));
        }
        for (view, render_layer, prepared_view) in prepared_views.iter() {
            // These projections are ChildOf(source). Query that existing Bevy
            // relation instead of scanning every projected mesh for every draw.
            let matching_projection = source_children
                .get(source)
                .into_iter()
                .flat_map(|children| children.iter())
                .filter_map(|child| projections.get(child).ok())
                .find(|(_, projection)| projection.source == source && projection.view == *view);
            if matching_projection
                .is_some_and(|(_, projection)| projection.source_material == source_material.0)
                && !changed_geometry.contains(source)
                && !changed_assets.contains(&source_material.0.id())
                && !views_changed
            {
                continue;
            }
            if let Some((entity, _)) = matching_projection {
                commands.entity(entity).despawn();
            }
            let Some(mut projected_material) = effect_materials.get(&source_material.0).cloned()
            else {
                continue;
            };
            projected_material.bind_per_draw_transform_semantics(
                source_transform.to_matrix(),
                prepared_view,
                true,
                true,
            );
            projected_material.allocate_programmable_float_register_buffers_for_model_view(
                &mut effect_uniform_buffers,
            );
            let mut projection = commands.spawn((
                mesh.clone(),
                MeshMaterial3d(effect_materials.add(projected_material)),
                Transform::default(),
                Visibility::Inherited,
                ChildOf(source),
                RenderLayers::layer(*render_layer),
                PerDrawEffectPassMaterial,
                ProgrammableEffectPassMaterial,
                EffectPassMaterialRenderViewProjection {
                    source,
                    view: *view,
                    source_material: source_material.0.clone(),
                },
            ));
            created += 1;
            if let Some(pass_submission_order) = pass_submission_order {
                projection.insert(*pass_submission_order);
            }
            if no_shadow_cast {
                projection.insert(NotShadowCaster);
            }
            if no_shadow_receive {
                projection.insert(NotShadowReceiver);
            }
            if let Some(skin) = skin {
                projection.insert(skin.clone());
            }
            if let Some(morph) = morph {
                projection.insert(morph.clone());
            }
        }
    }
    report_material_hot_loop_measurements(
        "project_effect_pass_material_instances_for_additional_render_views",
        measurement_started,
        scanned,
        created,
        &mut hot_loop_measurements,
    );
}

/// Opt-in aggregate timings; excludes deferred command application and GPU work.
fn report_material_hot_loop_measurements(
    system_name: &str,
    started: Option<std::time::Instant>,
    visited: u64,
    changed: u64,
    measurements: &mut MaterialHotLoopMeasurements,
) {
    let Some(started) = started else { return };
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    measurements.calls += 1;
    measurements.total_milliseconds += elapsed_ms;
    measurements.worst_milliseconds = measurements.worst_milliseconds.max(elapsed_ms);
    measurements.visited_items += visited;
    measurements.changed_items += changed;
    if measurements.calls == 120 {
        info!(target: "openzt2_material_hot_loops", system_name,
            calls = measurements.calls, mean_ms = measurements.total_milliseconds / 120.0,
            worst_ms = measurements.worst_milliseconds, visited_per_call = measurements.visited_items / 120,
            changed_per_call = measurements.changed_items / 120, "material hot loop attribution");
        *measurements = MaterialHotLoopMeasurements::default();
    }
}

fn effect_fog_shader_registers_for_render_view(
    fog: &DistanceFog,
    view_transform: &GlobalTransform,
    view_projection: &Projection,
) -> Option<[Vec4; 2]> {
    let FogFalloff::Linear { start, end } = fog.falloff else {
        return None;
    };
    let view_direction = Vec3::from(view_transform.forward());
    let far_plane = match view_projection {
        Projection::Perspective(projection) => projection.far,
        Projection::Orthographic(projection) => projection.far,
        Projection::Custom(_) => end,
    };
    Some([
        Vec4::new(start, (end - start).recip(), end, far_plane),
        view_direction.extend(view_transform.translation().dot(view_direction)),
    ])
}

pub(super) fn invalidate_authored_model_material_pass_projection(
    mut events: MessageReader<AssetEvent<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut commands: Commands,
    roots: Query<(Entity, &PrefabMaterialOverrides), With<AuthoredModelMaterialPassesProjected>>,
    children: Query<&Children>,
    additional_passes: Query<(), With<AdditionalAuthoredModelMaterialPass>>,
) {
    let changed_materials = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            AssetEvent::Unused { .. } => None,
        })
        .collect::<HashSet<_>>();
    let all_materials_changed = graphics_settings.is_changed();
    if changed_materials.is_empty() && !all_materials_changed {
        return;
    }
    for (root, overrides) in &roots {
        let root_material_changed = all_materials_changed
            || overrides.0.iter().any(|material_override| {
                material_override
                    .handle
                    .as_ref()
                    .is_some_and(|handle| changed_materials.contains(&handle.id()))
            });
        if !root_material_changed {
            continue;
        }
        commands
            .entity(root)
            .remove::<AuthoredModelMaterialPassesProjected>();
        for descendant in children.iter_descendants_depth_first(root) {
            if additional_passes.contains(descendant) {
                commands.entity(descendant).despawn();
            }
        }
    }
}
