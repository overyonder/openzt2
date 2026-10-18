use crate::model_render::prefab_render_layer_camera_matching::render_layers_for_prefab_entity_and_ancestor_chain;
use bevy::asset::AssetEvent;
use bevy::camera::visibility::RenderLayers;
use bevy::gltf::GltfMaterialName;
use bevy::light::NotShadowCaster;
use bevy::light::NotShadowReceiver;
use bevy::mesh::morph::MeshMorphWeights;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::pbr::DistanceFog;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::material::runtime::resolved_effect_transform_bindings::EffectRenderViewTransforms;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::authored_model_material_pass_projection::EffectPassMaterialRenderView;
use openzt2_world_state::model_render::authored_model_material_pass_projection::PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabClothingTextures;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabMaterialOverrides;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPrimitiveMaterial;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentLight;
use openzt2_zoo_state::environment::environment_state_types::AuthoredWindShaderPresentationState;

pub(super) fn project_authored_model_material_passes(
    mut commands: Commands,
    roots: Query<
        (
            Entity,
            &PrefabMaterialOverrides,
            Option<&PrefabClothingTextures>,
        ),
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
            Entity,
            &Camera,
            &GlobalTransform,
            &Projection,
            Option<&RenderLayers>,
            Has<EffectPassMaterialRenderView>,
        ),
        With<Camera3d>,
    >,
    hierarchy: Query<(
        Option<&ChildOf>,
        Option<&RenderLayers>,
        Option<&crate::model_render::world_entity_model_suppression::SuppressedModelRenderLayers>,
    )>,
) {
    for (root, overrides, clothing) in &roots {
        let source_parent = hierarchy
            .get(root)
            .ok()
            .and_then(|(parent, _, _)| parent.map(ChildOf::parent))
            .unwrap_or(root);
        let source_index = children
            .get(source_parent)
            .ok()
            .and_then(|children| children.iter().position(|child| child == root))
            .unwrap_or(0);
        let render_layers = render_layers_for_prefab_entity_and_ancestor_chain(root, &hierarchy);
        let Some((view_entity, _, camera_transform, camera_projection, _, _)) = cameras
            .iter()
            .find(|(_, camera, _, _, camera_layers, additional_view)| {
                camera.is_active
                    && !additional_view
                    && camera_layers.unwrap_or_default().intersects(&render_layers)
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
            target.insert(PrefabPrimitiveMaterial(handle.clone()));
            let projected_passes = material
                .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                .iter()
                .filter_map(|handle| {
                    let template = writable_effect_materials.get(handle)?;
                    let clothed =
                        material
                            .netimmerse_texture_binding
                            .as_ref()
                            .and_then(|binding| {
                                let texture = clothing?.0.iter().find(|(id, _)| {
                                    name.0
                                        .eq_ignore_ascii_case(&id.to_lowercase_hexadecimal_string())
                                })?;
                                Some(template.clone_with_netimmerse_decal(
                                    binding.decal_stage,
                                    binding.decal_uv_set,
                                    texture.1.clone(),
                                    &mut effect_uniform_buffers,
                                ))
                            });
                    let template = clothed.as_ref().unwrap_or(template);
                    // Only model/view-dependent inputs require instance-local
                    // buffers; world wind/light-only registers remain shared.
                    if !template.requires_draw_specific_shader_registers() {
                        return Some(clothed.map_or_else(
                            || handle.clone(),
                            |material| writable_effect_materials.add(material),
                        ));
                    }
                    let mut projected = template.clone();
                    projected.bind_per_draw_transform_semantics(
                        transform.to_matrix(),
                        &view_transforms,
                        true,
                        true,
                    );
                    projected.bind_fixed_function_transform_expressions(
                        transform.to_matrix(),
                        view_entity,
                        &view_transforms,
                        true,
                        true,
                        None,
                        None,
                        None,
                    );
                    projected.allocate_dynamic_uniform_buffers_for_model_view(
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
                .is_some_and(EffectPassMaterial::has_dynamic_effect_parameter_bindings)
            {
                target.insert(DynamicEffectPassMaterial);
            } else {
                target.remove::<DynamicEffectPassMaterial>();
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
                    .is_some_and(EffectPassMaterial::has_dynamic_effect_parameter_bindings)
                {
                    additional_pass.insert(DynamicEffectPassMaterial);
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

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_per_draw_effect_transform_semantics(
    mut hot_loop_measurements: Local<MaterialHotLoopMeasurements>,
    mut prepared_views: Local<Vec<(Entity, EffectRenderViewTransforms)>>,
    cameras: Query<
        (
            Entity,
            Ref<Camera>,
            Ref<GlobalTransform>,
            Ref<Projection>,
            Option<&RenderLayers>,
            Option<Ref<DistanceFog>>,
            Has<ZooCamera>,
            Has<EffectPassMaterialRenderView>,
        ),
        With<Camera3d>,
    >,
    hierarchy: Query<(
        Option<&ChildOf>,
        Option<&RenderLayers>,
        Option<&crate::model_render::world_entity_model_suppression::SuppressedModelRenderLayers>,
    )>,
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
    renderables: Query<
        (
            Entity,
            Ref<GlobalTransform>,
            Ref<MeshMaterial3d<EffectPassMaterial>>,
            Ref<PerDrawEffectPassMaterial>,
            Ref<ViewVisibility>,
            Option<&EffectPassMaterialRenderViewProjection>,
        ),
        With<DynamicEffectPassMaterial>,
    >,
) {
    let measurement_started =
        bevy::log::tracing::enabled!(target: "openzt2_material_hot_loops", bevy::log::Level::DEBUG)
            .then(std::time::Instant::now);
    let mut scanned = 0_u64;
    let mut updated = 0_u64;
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
    prepared_views.clear();
    prepared_views.extend(
        cameras
            .iter()
            .filter(|(_, camera, ..)| camera.is_active)
            .map(|(entity, _, transform, projection, ..)| {
                (
                    entity,
                    EffectRenderViewTransforms::from_camera(&transform, &projection),
                )
            }),
    );
    let world_fog = cameras
        .iter()
        .find(|(_, _, _, _, _, _, zoo, _)| *zoo)
        .and_then(|(_, _, _, _, _, fog, _, _)| fog);
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
    for (entity, transform, material_handle, projected_marker, visibility, view_projection) in
        &renderables
    {
        scanned += 1;
        if !visibility.get() {
            continue;
        }
        let (transform, accepted_camera) =
            if let Some(view_projection) = view_projection {
                let (Ok(source), Ok(camera)) = (
                    renderables.get(view_projection.source),
                    cameras.get(view_projection.view),
                ) else {
                    continue;
                };
                (source.1, camera)
            } else {
                let layers = render_layers_for_prefab_entity_and_ancestor_chain(entity, &hierarchy);
                let Some(camera) = cameras.iter().find(
                    |(_, camera, _, _, camera_layers, _, _, additional_view)| {
                        camera.is_active
                            && !additional_view
                            && camera_layers.unwrap_or_default().intersects(&layers)
                    },
                ) else {
                    continue;
                };
                (transform, camera)
            };
        let (view_entity, camera, view_transform, projection, _, view_fog, _, _) = accepted_camera;
        if !camera.is_active {
            continue;
        }
        let Some((_, prepared_view)) = prepared_views
            .iter()
            .find(|(entity, _)| *entity == view_entity)
        else {
            continue;
        };
        // Additional world cameras share the world's authored fog settings, but
        // derive directional/depth inputs from their own accepted view. Preview
        // cameras keep their own environment instead of borrowing the zoo view.
        let distance_fog = if view_projection.is_some() {
            view_fog.or(world_fog)
        } else {
            view_fog
        };
        let fog_changed = distance_fog.as_ref().is_some_and(|fog| fog.is_changed());
        let force = material_handle.is_changed()
            || visibility.is_changed()
            || projected_marker.is_changed();
        let model_changed = force || transform.is_changed();
        let view_changed =
            force || camera.is_changed() || view_transform.is_changed() || projection.is_changed();
        // Only CPU register values change here. Buffer handles and pipeline
        // state stay fixed; ShaderBuffer assets publish the GPU updates.
        // Emitting Material::Modified would unnecessarily rebuild bind groups.
        let Some(material) = effect_materials
            .get_mut_untracked(&material_handle.0)
            .filter(|material| material.has_dynamic_effect_parameter_bindings())
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
            .and_then(|fog| prepared_view.fog_shader_registers(fog, &projection));
        material.bind_changed_environment_registers(
            (force || wind_changed).then_some(wind_registers.as_slice()),
            (force || light_changed).then_some(light_rig.as_slice()),
            fog_parameters.as_ref().map(|values| values.as_slice()),
        );
        material.bind_fixed_function_transform_expressions(
            transform.to_matrix(),
            view_entity,
            prepared_view,
            model_changed,
            view_changed,
            (force || wind_changed).then_some(wind_registers.as_slice()),
            (force || light_changed).then_some(light_rig.as_slice()),
            fog_parameters.as_ref().map(|values| values.as_slice()),
        );
        material.write_changed_fixed_function_transforms_to_persistent_buffer(
            &mut effect_uniform_buffers,
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
    mut invalidation: crate::model_render::material_render_view_invalidation::MaterialRenderViewInvalidation,
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
        bevy::log::tracing::enabled!(target: "openzt2_material_hot_loops", bevy::log::Level::DEBUG)
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
            projected_material.bind_fixed_function_transform_expressions(
                source_transform.to_matrix(),
                *view,
                prepared_view,
                true,
                true,
                None,
                None,
                None,
            );
            projected_material
                .allocate_dynamic_uniform_buffers_for_model_view(&mut effect_uniform_buffers);
            let mut projection = commands.spawn((
                mesh.clone(),
                MeshMaterial3d(effect_materials.add(projected_material)),
                Transform::default(),
                Visibility::Inherited,
                ChildOf(source),
                RenderLayers::layer(*render_layer),
                PerDrawEffectPassMaterial,
                DynamicEffectPassMaterial,
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
        debug!(target: "openzt2_material_hot_loops", system_name,
            calls = measurements.calls, mean_ms = measurements.total_milliseconds / 120.0,
            worst_ms = measurements.worst_milliseconds, visited_per_call = measurements.visited_items / 120,
            changed_per_call = measurements.changed_items / 120, "material hot loop attribution");
        *measurements = MaterialHotLoopMeasurements::default();
    }
}

pub(super) fn invalidate_authored_model_material_pass_projection(
    mut events: MessageReader<AssetEvent<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut commands: Commands,
    roots: Query<
        (
            Entity,
            &PrefabMaterialOverrides,
            Option<Ref<PrefabClothingTextures>>,
        ),
        With<AuthoredModelMaterialPassesProjected>,
    >,
    children: Query<&Children>,
    additional_passes: Query<(), With<AdditionalAuthoredModelMaterialPass>>,
    changed_clothing: Query<(), Changed<PrefabClothingTextures>>,
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
    if changed_materials.is_empty() && !all_materials_changed && changed_clothing.is_empty() {
        return;
    }
    for (root, overrides, clothing) in &roots {
        let root_material_changed = clothing.is_some_and(|clothing| clothing.is_changed())
            || all_materials_changed
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

#[derive(Default)]
pub(crate) struct MaterialHotLoopMeasurements {
    calls: u32,
    total_milliseconds: f64,
    worst_milliseconds: f64,
    visited_items: u64,
    changed_items: u64,
}

#[derive(Component)]
pub(crate) struct AuthoredModelMaterialPassesProjected;

#[derive(Component)]
pub(crate) struct AdditionalAuthoredModelMaterialPass;

#[derive(Component)]
pub(crate) struct PerDrawEffectPassMaterial;

#[derive(Component)]
pub(crate) struct DynamicEffectPassMaterial;

#[derive(Component)]
pub(crate) struct EffectPassMaterialRenderViewProjection {
    source: Entity,
    view: Entity,
    source_material: Handle<EffectPassMaterial>,
}

#[derive(Component)]
pub(crate) struct EffectPassMaterialSourceRenderLayers(pub(crate) RenderLayers);
