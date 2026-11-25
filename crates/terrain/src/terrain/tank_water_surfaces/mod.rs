use bevy::{
    camera::visibility::RenderLayers, mesh::VertexAttributeValues, prelude::*,
    render::storage::ShaderBuffer,
};
use openzt2_game_assets::{
    graphics_settings_types::{FourLevelGraphicsDetail, GraphicsSettings},
    material::{
        material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
    },
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::{
    model_render::authored_effect_technique_pass_submission_order::{
        AuthoredEffectTechniquePassSubmissionOrder, AuthoredEffectTransparentRenderStage,
    },
    simulation_time::native_visual_random_stream::NativeVisualRandomStream,
    world_spawn::{
        selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
        world_membership_types::WorldMember,
    },
};
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::{Tank, TankGeometry, TankWaterSurfaceMesh},
    terrain::{
        terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState,
        terrain_water_renderer_types::{
            AuthoredTerrainWaterRendererTargets, TerrainWaterScumScroll,
            TerrainWaterSurfacePassKind, TERRAIN_WATER_RENDER_LAYER,
        },
    },
};

use super::terrain_water_material_binding::{
    bind_authored_water_material_values, bind_authored_water_surface_textures,
};
use super::terrain_water_renderer_inputs::TerrainWaterLowDetailTextureState;

#[derive(Component)]
pub(super) struct TankWaterDraw {
    pub(super) tank: Entity,
    template: Handle<EffectPassMaterial>,
    order: usize,
}

#[derive(Component, PartialEq)]
pub(super) struct TankWaterMaterial {
    pub(super) terrain: Handle<TerrainAsset>,
    pub(super) id: openzt2_game_data::AssetId,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn project_authored_tank_water_draws(
    mut commands: Commands,
    graphics: Res<GraphicsSettings>,
    catalogue: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    materials: Res<Assets<MaterialAsset>>,
    targets: Single<Ref<AuthoredTerrainWaterRendererTargets>>,
    world_assets: Query<&SelectedWorldTerrainAssetHandle>,
    tanks: Query<(Ref<Tank>, &TankGeometry)>,
    surfaces: Query<(
        Entity,
        Ref<TankWaterSurfaceMesh>,
        Ref<Mesh3d>,
        &WorldMember,
        Option<&Children>,
    )>,
    draws: Query<(&TankWaterDraw, &TerrainWaterSurfacePassKind)>,
    mut events: MessageReader<AssetEvent<EffectPassMaterial>>,
    mut effects: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    (mut meshes, mut mesh_events): (ResMut<Assets<Mesh>>, MessageReader<AssetEvent<Mesh>>),
    (mut random, mut waves, mut existing, low_states, scum_states): (
        ResMut<NativeVisualRandomStream>,
        Query<&mut NativeTerrainWaterGeometricWaveShaderState>,
        Query<&mut TankWaterMaterial>,
        Query<(), With<TerrainWaterLowDetailTextureState>>,
        Query<(), With<TerrainWaterScumScroll>>,
    ),
) {
    let Some(definitions) = catalogue.get(&definition_assets) else {
        return;
    };
    let changed: bevy::platform::collections::HashSet<_> = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect();
    let changed_meshes: bevy::platform::collections::HashSet<_> = mesh_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect();
    let source_changes = graphics.is_changed()
        || catalogue.is_changed()
        || definition_assets.is_changed()
        || terrain_assets.is_changed()
        || materials.is_changed()
        || targets.is_changed();
    for (entity, surface, mesh, world, children) in &surfaces {
        let Some((tank, geometry, terrain_handle, terrain, water, presentation, handle)) = (|| {
            let (tank, geometry) = tanks.get(surface.0).ok()?;
            let definition = definitions.find_tank(tank.definition)?;
            let terrain_handle = &world_assets.get(world.root).ok()?.0;
            let terrain = terrain_assets.get(terrain_handle)?;
            let (presentation, low, medium, geometric) =
                terrain.tank_water_material_presentation(definition.water_material)?;
            let handle = match graphics.water_detail {
                FourLevelGraphicsDetail::Low => low,
                FourLevelGraphicsDetail::Medium => medium,
                FourLevelGraphicsDetail::High | FourLevelGraphicsDetail::Ultra => geometric,
            };
            Some((
                tank,
                geometry,
                terrain_handle,
                terrain,
                definition.water_material,
                presentation,
                handle,
            ))
        })(
        ) else {
            for child in children.into_iter().flat_map(|children| children.iter()) {
                if draws.contains(child) {
                    commands.entity(child).try_despawn();
                }
            }
            continue;
        };
        let appearance = TankWaterMaterial {
            terrain: terrain_handle.clone(),
            id: water,
        };
        if let Ok(mut current) = existing.get_mut(surface.0) {
            current.set_if_neq(appearance);
        } else {
            commands.entity(surface.0).try_insert(appearance);
        }
        let stitch = matches!(
            graphics.water_detail,
            FourLevelGraphicsDetail::Low | FourLevelGraphicsDetail::Medium
        );
        let scum = !stitch
            && presentation
                .surface_material
                .configuration
                .scum
                .as_ref()
                .and_then(|scum| scum.image.as_deref())
                .is_some_and(|image| terrain.texture_image(image).is_some());
        let material = if scum {
            terrain.water_material("Materials/waterscum.bfmat")
        } else {
            Some(handle)
        };
        let Some(material) = material.and_then(|handle| materials.get(handle)) else {
            continue;
        };
        let templates =
            material.evaluated_pass_material_assets_for_effect_quality(graphics.effects);
        let mesh_changed = mesh.is_changed() || changed_meshes.contains(&mesh.id());
        if !source_changes
            && !surface.is_changed()
            && !mesh_changed
            && !tank.is_changed()
            && children.is_some()
            && !templates
                .iter()
                .any(|handle| changed.contains(&handle.id()))
        {
            continue;
        }
        let water_presentation = &presentation.surface_material;
        if let Ok(mut state) = waves.get_mut(surface.0) {
            state.reconfigure(
                water_presentation.geometric_waves,
                water_presentation.ripple_waves,
                &mut || random.unit_f64(),
            );
        } else {
            commands.entity(surface.0).try_insert(
                NativeTerrainWaterGeometricWaveShaderState::from_authored_geometric_wave(
                    water_presentation.geometric_waves,
                    water_presentation.ripple_waves,
                    &mut || random.unit_f64(),
                ),
            );
        }
        if stitch && !low_states.contains(surface.0) {
            commands
                .entity(surface.0)
                .try_insert(TerrainWaterLowDetailTextureState::default());
        } else if !stitch && low_states.contains(surface.0) {
            commands
                .entity(surface.0)
                .try_remove::<TerrainWaterLowDetailTextureState>();
        }
        if scum && !scum_states.contains(surface.0) {
            commands
                .entity(surface.0)
                .try_insert(TerrainWaterScumScroll::default());
        } else if !scum && scum_states.contains(surface.0) {
            commands
                .entity(surface.0)
                .try_remove::<TerrainWaterScumScroll>();
        }
        let Some(templates) = templates
            .iter()
            .map(|handle| Some((handle, effects.get(handle)?.clone())))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let mut retained = children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|entity| {
                draws
                    .get(entity)
                    .ok()
                    .map(|(draw, kind)| (draw.order, (entity, draw, kind)))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let authored = if stitch || scum {
            templates
                .iter()
                .map(|(handle, pass)| {
                    let kind = if scum {
                        TerrainWaterSurfacePassKind::Scum
                    } else if pass.has_fixed_function_reflection_projection() {
                        TerrainWaterSurfacePassKind::MediumEnvironment
                    } else if graphics.water_detail == FourLevelGraphicsDetail::Medium {
                        TerrainWaterSurfacePassKind::MediumDetail
                    } else {
                        TerrainWaterSurfacePassKind::LowDetail
                    };
                    (handle, pass, kind)
                })
                .collect::<Vec<_>>()
        } else {
            let [(refract_handle, refraction), (reflect_handle, reflection), ..] =
                templates.as_slice()
            else {
                continue;
            };
            vec![
                (
                    reflect_handle,
                    reflection,
                    TerrainWaterSurfacePassKind::FallbackReflection,
                ),
                (
                    refract_handle,
                    refraction,
                    TerrainWaterSurfacePassKind::Refraction,
                ),
                (
                    reflect_handle,
                    reflection,
                    TerrainWaterSurfacePassKind::Reflection,
                ),
            ]
        };
        let mut draw_mesh = None;
        for (index, (&handle, template, kind)) in authored.into_iter().enumerate() {
            if let Some((previous, draw, previous_kind)) = retained.remove(&index) {
                if draw.template == *handle
                    && *previous_kind == kind
                    && !mesh_changed
                    && !source_changes
                    && !changed.contains(&handle.id())
                {
                    continue;
                }
                commands.entity(previous).try_despawn();
            }
            if draw_mesh.is_none() {
                let Some(source) = meshes.get(mesh.id()) else {
                    continue;
                };
                let Some(source) =
                    authored_tank_water_mesh(source, &presentation.texture_animations, stitch)
                else {
                    continue;
                };
                draw_mesh = Some(meshes.add(source));
            }
            let mut pass = template.clone();
            if !kind.uses_stitch_geometry() {
                bind_authored_water_surface_textures(&mut pass, &targets, terrain, presentation);
                bind_authored_water_material_values(&mut pass, presentation, geometry.water_height);
                if matches!(
                    kind,
                    TerrainWaterSurfacePassKind::FallbackReflection
                        | TerrainWaterSurfacePassKind::Scum
                ) {
                    pass.replace_texture_asset_for_effect_semantic(
                        "EnvReflect",
                        targets.fallback_reflection.clone(),
                    );
                }
            }
            pass.retain_authored_draw_order();
            pass.recreate_uniform_buffers_for_runtime_material(&mut buffers);
            let Some(order) = AuthoredEffectTechniquePassSubmissionOrder::new(entity, index) else {
                continue;
            };
            commands.spawn((
                TankWaterDraw {
                    tank: surface.0,
                    template: handle.clone(),
                    order: index,
                },
                kind,
                Mesh3d(draw_mesh.as_ref().expect("source mesh was built").clone()),
                MeshMaterial3d(effects.add(pass)),
                order.with_render_stage(AuthoredEffectTransparentRenderStage::WaterSurface),
                RenderLayers::layer(TERRAIN_WATER_RENDER_LAYER),
                ChildOf(entity),
                Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::Inherited,
            ));
        }
        for (entity, ..) in retained.into_values() {
            commands.entity(entity).try_despawn();
        }
    }
}

fn authored_tank_water_mesh(
    mesh: &Mesh,
    animations: &[openzt2_game_data::terrain::TerrainWaterTextureAnimation; 2],
    stitch: bool,
) -> Option<Mesh> {
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let positions: Vec<_> = positions.iter().map(|&[x, y, z]| [x, -z, y]).collect();
    let mut mesh = Mesh::new(
        mesh.primitive_topology(),
        bevy::asset::RenderAssetUsages::default(),
    );
    if stitch {
        let count = positions.len();
        let [uv0, uv1] = animations.each_ref().map(|animation| {
            let scale = Vec2::from_array(animation.grid_dimensions) * 3.0;
            positions
                .iter()
                .map(|&[x, y, _]| (Vec2::new(x, y) / scale).to_array())
                .collect::<Vec<_>>()
        });
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv0);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    Some(mesh)
}

pub(super) fn remove_tank_wave_state(removed: On<Remove<Tank>>, mut commands: Commands) {
    commands.entity(removed.entity).try_remove::<(
        TankWaterMaterial,
        NativeTerrainWaterGeometricWaveShaderState,
        TerrainWaterLowDetailTextureState,
        TerrainWaterScumScroll,
        openzt2_zoo_state::terrain::terrain_water_underwater_particle_types::TerrainWaterRegionLight,
    )>();
}
