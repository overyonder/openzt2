use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy::render::{render_resource::VertexFormat, storage::ShaderBuffer};
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
use openzt2_game_data::AssetId;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::{
    model_render::{
        authored_effect_technique_pass_submission_order::{
            AuthoredEffectTechniquePassSubmissionOrder, AuthoredEffectTransparentRenderStage,
        },
        authored_model_material_pass_projection::PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER,
    },
    world_spawn::{
        selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
        world_membership_types::WorldMember,
    },
};
use openzt2_zoo_state::{
    aquatic::{
        aquatic_simulation_types::TankWaterSurfaceMesh,
        tank_wall_ground_sampling::TankWallGroundSampling,
    },
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::{HabitatChanged, HabitatIndex},
    },
    terrain::{
        terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER,
        terrain_chunk_types::TerrainChunk,
        terrain_edit_types::TerrainChanged,
        terrain_water_geometric_wave_shader_state::TerrainWaterWaveRegisters,
        terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionQuery},
        terrain_water_underwater_particle_types::TerrainWaterRegionLight,
    },
    topology::{
        habitat_boundary_queries::HabitatBoundaryQueries,
        topology_graph_types::{
            FenceEdge, FenceEnd, FenceSegment, FenceStart, TopologyGrid, TopologyNode,
        },
    },
};

use super::aquatic_simulation_types::{Tank, TankGeometry, TankWallTopHeight, WaterQuality};
use super::tank_geometry_projection::TankExcavationBoundary;
use super::water_filter_operation::WaterQualityFraction;

#[derive(Component)]
#[relationship(relationship_target = TankSurfaceMeshes)]
pub(super) struct TankSurfaceMeshOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = TankSurfaceMeshOf, linked_spawn)]
pub(super) struct TankSurfaceMeshes(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = TankWallPanels)]
pub(super) struct TankWallPanelOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = TankWallPanelOf, linked_spawn)]
pub(super) struct TankWallPanels(Vec<Entity>);

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) enum AuthoredTankSurface {
    Floor(AssetId),
    Panel {
        material: AssetId,
        wet_sides: [Option<TankPanelWater>; 2],
    },
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct TankPanelWater {
    owner: Entity,
    height: f32,
    side_colour: Option<[u8; 4]>,
    lit_side_colour: Option<[u8; 4]>,
}

#[derive(Component)]
pub(super) struct TankSurfaceEffectPass(usize, Handle<EffectPassMaterial>);

#[derive(Component, Default, PartialEq)]
pub(super) struct TankWallPanelGeometry {
    ends: [Vec2; 2],
    bottom: [f32; 2],
    top: f32,
}

pub(super) fn project_tank_surface_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    active: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    grids: Query<Ref<TopologyGrid>>,
    tanks: Query<(
        Entity,
        Ref<Tank>,
        Ref<TankGeometry>,
        Ref<TankExcavationBoundary>,
        &WorldMember,
        Option<&TankSurfaceMeshes>,
    )>,
    mut surfaces: Query<(
        Entity,
        &mut Mesh3d,
        &mut Transform,
        &mut Visibility,
        Option<&mut AuthoredTankSurface>,
    )>,
) {
    let Some(definitions) = active.get(&definition_assets) else {
        return;
    };
    let catalogue_changed = active.is_changed() || definition_assets.is_changed();
    for (tank, definition, geometry, outline, member, meshes_for_tank) in &tanks {
        let Ok(grid) = grids.get(member.root) else {
            continue;
        };
        let outline_changed = outline.is_changed() || grid.is_changed();
        if !outline_changed
            && !geometry.is_changed()
            && !definition.is_changed()
            && !catalogue_changed
            && meshes_for_tank.is_some()
        {
            continue;
        }
        let Some(definition) = definitions.find_tank(definition.definition) else {
            if let Some(previous) = meshes_for_tank {
                for surface in previous.iter() {
                    commands.entity(surface).try_despawn();
                }
            }
            continue;
        };
        let floor = (definition.floor_material != AssetId::default())
            .then_some(AuthoredTankSurface::Floor(definition.floor_material));
        let mut replacement = if outline_changed {
            build_water_surface_mesh(*grid, &outline.walls).map(|mesh| meshes.add(mesh))
        } else {
            None
        };
        let mut missing_water = true;
        let mut missing_floor = floor.is_some();
        if let Some(children) = meshes_for_tank {
            for entity in children.iter() {
                let Ok((_, mesh, mut transform, mut visible, material)) = surfaces.get_mut(entity)
                else {
                    continue;
                };
                let (height, visibility) = if let Some(mut material) = material {
                    let Some(floor) = floor else {
                        commands.entity(entity).try_despawn();
                        continue;
                    };
                    material.set_if_neq(floor);
                    missing_floor = false;
                    (geometry.floor_height, Visibility::Inherited)
                } else {
                    missing_water = false;
                    (geometry.water_height, tank_water_visibility(&geometry))
                };
                if outline_changed {
                    let Some(replacement) = replacement.as_ref() else {
                        commands.entity(entity).try_despawn();
                        continue;
                    };
                    mesh.map_unchanged(|mesh| &mut mesh.0)
                        .set_if_neq(replacement.clone());
                }
                transform
                    .map_unchanged(|transform| &mut transform.translation.y)
                    .set_if_neq(height);
                visible.set_if_neq(visibility);
            }
        }
        if missing_water || missing_floor {
            let mesh = if let Some(mesh) = replacement.take() {
                mesh
            } else if !outline_changed {
                let Some(mesh) = build_water_surface_mesh(*grid, &outline.walls) else {
                    continue;
                };
                meshes.add(mesh)
            } else {
                continue;
            };
            if missing_water {
                commands.spawn((
                    TankSurfaceMeshOf(tank),
                    TankWaterSurfaceMesh(tank),
                    *member,
                    Mesh3d(mesh.clone()),
                    Transform::from_xyz(0.0, geometry.water_height, 0.0),
                    tank_water_visibility(&geometry),
                ));
            }
            if let Some(floor) = floor.filter(|_| missing_floor) {
                commands.spawn((
                    TankSurfaceMeshOf(tank),
                    floor,
                    *member,
                    Mesh3d(mesh),
                    Transform::from_xyz(0.0, geometry.floor_height, 0.0),
                    Visibility::Inherited,
                ));
            }
        }
    }
}

fn tank_water_visibility(geometry: &TankGeometry) -> Visibility {
    if geometry.depth() > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn project_tank_wall_panels(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    graphics: Res<GraphicsSettings>,
    active: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    grids: Query<Ref<TopologyGrid>>,
    walls: Query<(
        Entity,
        FenceSegment,
        &WorldMember,
        Ref<TankWallTopHeight>,
        Option<&TankWallPanels>,
    )>,
    changed_walls: Query<(), Or<(Changed<FenceEdge>, Changed<FenceStart>, Changed<FenceEnd>)>>,
    nodes: Query<Ref<TopologyNode>>,
    boundaries: HabitatBoundaryQueries,
    tanks: Query<(Ref<Tank>, Ref<TankGeometry>, Ref<TankExcavationBoundary>)>,
    ground: TankWallGroundSampling,
    water_indexes: Query<(Ref<TerrainWaterMembershipIndex>, &WorldMember)>,
    water_regions: TerrainWaterRegionQuery,
    mut panel_geometry: Query<(
        &Mesh3d,
        &mut TankWallPanelGeometry,
        &mut AuthoredTankSurface,
    )>,
    (mut habitat_changes, mut terrain_changes, chunks, world_terrains, mut demoted, terrains): (
        MessageReader<HabitatChanged>,
        MessageReader<TerrainChanged>,
        Query<(&TerrainChunk, &WorldMember)>,
        Query<Ref<SelectedWorldTerrainAssetHandle>>,
        RemovedComponents<Tank>,
        Res<Assets<TerrainAsset>>,
    ),
) {
    let Some(definitions) = active.get(&definition_assets) else {
        return;
    };
    let changed_habitats: bevy::ecs::entity::EntityHashSet = habitat_changes
        .read()
        .map(|change| change.habitat_entity)
        .chain(demoted.read())
        .collect();
    let ground_changes: Vec<_> = terrain_changes
        .read()
        .filter(|change| change.ground_height_changed)
        .filter_map(|change| {
            let (chunk, member) = chunks.get(change.chunk).ok()?;
            let minimum = chunk.origin + change.min.as_vec2() * chunk.spacing_m;
            let maximum = chunk.origin + change.max.as_vec2() * chunk.spacing_m;
            Some((
                member.root,
                minimum - Vec2::splat(chunk.spacing_m),
                maximum + Vec2::splat(chunk.spacing_m),
            ))
        })
        .collect();
    let definitions_changed = active.is_changed() || definition_assets.is_changed();
    let low_detail = matches!(
        graphics.water_detail,
        FourLevelGraphicsDetail::Low | FourLevelGraphicsDetail::Medium
    );
    for (wall, edge, member, top, panels) in &walls {
        let panel = panels.and_then(|panels| panels.iter().next());
        let (Ok(grid), Ok(a), Ok(b)) = (
            grids.get(member.root),
            nodes.get(edge.a()),
            nodes.get(edge.b()),
        ) else {
            continue;
        };
        let Some(definition) = definitions.find_tank(edge.definition) else {
            if let Some(panel) = panel {
                commands.entity(panel).try_despawn();
            }
            continue;
        };
        let members: Vec<_> = boundaries.habitats(wall).collect();
        let index = water_indexes
            .iter()
            .find_map(|(index, owner)| (owner.root == member.root).then_some(index));
        let world_terrain = world_terrains.get(member.root).ok();
        let ends = [
            grid.cell_translation(a.cell).xz(),
            grid.cell_translation(b.cell).xz(),
        ];
        if panel.is_some()
            && !definitions_changed
            && !graphics.is_changed()
            && !grid.is_changed()
            && !top.is_changed()
            && !a.is_changed()
            && !b.is_changed()
            && !changed_walls.contains(wall)
            && !terrains.is_changed()
            && !world_terrain
                .as_ref()
                .is_some_and(DetectChanges::is_changed)
            && !index.as_ref().is_some_and(DetectChanges::is_changed)
            && !ground_changes.iter().any(|(root, minimum, maximum)| {
                *root == member.root
                    && ends
                        .iter()
                        .any(|end| end.cmpge(*minimum).all() && end.cmple(*maximum).all())
            })
            && !members.iter().any(|habitat| {
                changed_habitats.contains(habitat)
                    || tanks.get(*habitat).is_ok_and(|(tank, geometry, outline)| {
                        tank.is_changed() || geometry.is_changed() || outline.is_changed()
                    })
            })
        {
            continue;
        }
        let [ground_a, ground_b] =
            [a.cell, b.cell].map(|cell| ground.ground_height_at_topology_cell(member.root, cell));
        let (Some(ground_a), Some(ground_b)) = (ground_a, ground_b) else {
            continue;
        };
        if ends[0] == ends[1] {
            continue;
        }
        let mut bottom = [ground_a, ground_b];
        let edge_vector = ends[1] - ends[0];
        let centre = (ends[0] + ends[1]) * 0.5;
        // Source space has Y opposite Bevy Z. The CCW probe is the
        // left-hand side of the segment in source space.
        let normal = Vec2::new(edge_vector.y, -edge_vector.x) * 0.5;
        let mut wet = [None, None];
        for (side, distance) in [1.0, -1.0].into_iter().enumerate() {
            let point = centre + normal * distance;
            let tank = members.iter().find_map(|habitat| {
                let (tank, geometry, outline) = tanks.get(*habitat).ok()?;
                inside_walls(point, &outline.walls).then_some((*habitat, tank, geometry))
            });
            if let Some((owner, tank, geometry)) = tank {
                bottom = bottom.map(|height| height.min(geometry.floor_height));
                if geometry.depth() > 0.0 {
                    let configuration = (|| {
                        let id = definitions.find_tank(tank.definition)?.water_material;
                        let terrain = terrains.get(&world_terrain.as_ref()?.0)?;
                        let (presentation, ..) = terrain.tank_water_material_presentation(id)?;
                        presentation.surface_material.configuration.side.as_ref()
                    })();
                    wet[side] = Some(panel_water(owner, geometry.water_height, configuration));
                }
            } else if let Some(index) = index.as_ref() {
                wet[side] = (|| {
                    let terrain = terrains.get(index.terrain())?;
                    let owner = index.owner_at_world_xz(point, terrain)??;
                    let source = water_regions.get(owner)?;
                    let height = source.surface_height(terrain)?;
                    let configuration = source
                        .biome(terrain)?
                        .water_presentation
                        .as_ref()?
                        .surface_material
                        .configuration
                        .side
                        .as_ref();
                    Some(panel_water(owner, height, configuration))
                })();
            }
        }
        let mut geometry = TankWallPanelGeometry {
            ends,
            bottom,
            top: top.0,
        };
        let (variant, wet_sides) = match wet {
            [None, None] => (0, [None, None]),
            [Some(water), None] => (1, [Some(water), None]),
            [None, Some(water)] => {
                geometry.ends.reverse();
                geometry.bottom.reverse();
                (1, [Some(water), None])
            }
            [Some(a), Some(b)] => (2, [Some(a), Some(b)]),
        };
        let material = if low_detail {
            definition.wall_materials_low[variant]
        } else {
            definition.wall_materials[variant]
        };
        let source = AuthoredTankSurface::Panel {
            material,
            wet_sides,
        };
        if let Some((mesh, mut current, mut authored)) =
            panel.and_then(|panel| panel_geometry.get_mut(panel).ok())
        {
            if *current != geometry {
                let _ = meshes.insert(mesh.id(), build_tank_wall_panel_mesh(&geometry));
                current.set_if_neq(geometry);
            }
            authored.set_if_neq(source);
        } else {
            commands.spawn((
                TankWallPanelOf(wall),
                source,
                Mesh3d(meshes.add(build_tank_wall_panel_mesh(&geometry))),
                geometry,
                *member,
                Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::Inherited,
            ));
        }
    }
}

fn panel_water(
    owner: Entity,
    height: f32,
    config: Option<&openzt2_game_data::terrain::TerrainWaterSideConfiguration>,
) -> TankPanelWater {
    TankPanelWater {
        owner,
        height,
        side_colour: config.and_then(|side| side.colour_rgba255),
        lit_side_colour: config.and_then(|side| side.lit_colour_rgba255),
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn project_authored_tank_material_passes(
    mut commands: Commands,
    graphics: Res<GraphicsSettings>,
    catalogue: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    materials: Res<Assets<MaterialAsset>>,
    mut effects: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    sources: Query<(
        Entity,
        Ref<Mesh3d>,
        Ref<AuthoredTankSurface>,
        Option<&Children>,
    )>,
    mut draws: Query<
        (
            &TankSurfaceEffectPass,
            &MeshMaterial3d<EffectPassMaterial>,
            &mut Mesh3d,
        ),
        Without<AuthoredTankSurface>,
    >,
    mut loaded: MessageReader<AssetEvent<EffectPassMaterial>>,
) {
    let effects_changed: bevy::platform::collections::HashSet<_> = loaded
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect();
    let catalogue_changed = catalogue.is_changed() || definitions.is_changed();
    let Some(definitions) = catalogue.get(&definitions) else {
        return;
    };
    for (owner, mesh, source, children) in &sources {
        let (id, stage, layer) = match *source {
            AuthoredTankSurface::Floor(id) => (
                id,
                AuthoredEffectTransparentRenderStage::TerrainEffects,
                TERRAIN_RENDER_LAYER,
            ),
            AuthoredTankSurface::Panel { material, .. } => (
                material,
                AuthoredEffectTransparentRenderStage::Scene,
                PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER,
            ),
        };
        let handles = definitions
            .material(id)
            .and_then(|handle| materials.get(&handle))
            .map(|material| {
                material.evaluated_pass_material_assets_for_effect_quality(graphics.effects)
            });
        let changed = source.is_changed()
            || mesh.is_changed()
            || graphics.is_changed()
            || catalogue_changed
            || materials.is_changed()
            || handles.as_ref().is_some_and(|handles| {
                handles
                    .iter()
                    .any(|handle| effects_changed.contains(&handle.id()))
            });
        if !changed && children.is_some() {
            continue;
        }
        let mut retained = children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|entity| draws.get(entity).ok().map(|(pass, ..)| (pass.0, entity)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let Some(handles) = handles else {
            for entity in retained.into_values() {
                commands.entity(entity).try_despawn();
            }
            continue;
        };
        for (index, template) in handles.iter().enumerate() {
            let Some(mut material) = effects.get(template).cloned() else {
                continue;
            };
            if let Some(entity) = retained.remove(&index) {
                if let Ok((pass, handle, draw_mesh)) = draws.get_mut(entity) {
                    if pass.1 == *template
                        && !effects_changed.contains(&template.id())
                        && effects.contains(handle.id())
                    {
                        draw_mesh
                            .map_unchanged(|mesh| &mut mesh.0)
                            .set_if_neq(mesh.0.clone());
                        continue;
                    }
                }
                commands.entity(entity).try_despawn();
            }
            material.retain_authored_draw_order();
            let mut material = material
                .clone_with_fixed_function_world_lighting_context(None, false, layer, &mut buffers)
                .unwrap_or(material);
            material.recreate_uniform_buffers_for_runtime_material(&mut buffers);
            let Some(order) = AuthoredEffectTechniquePassSubmissionOrder::new(owner, index) else {
                continue;
            };
            commands.spawn((
                TankSurfaceEffectPass(index, template.clone()),
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(effects.add(material)),
                order.with_render_stage(stage),
                RenderLayers::from_layers(&[0, layer]),
                ChildOf(owner),
                Transform::IDENTITY,
                Visibility::Inherited,
            ));
        }
        for entity in retained.into_values() {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(super) fn update_tank_wall_water_bindings(
    panels: Query<&AuthoredTankSurface>,
    draws: Query<(&ChildOf, &MeshMaterial3d<EffectPassMaterial>), With<TankSurfaceEffectPass>>,
    water_registers: Query<(&TerrainWaterWaveRegisters, Option<&TerrainWaterRegionLight>)>,
    water_quality: Query<(&WaterQuality, Option<&WaterQualityFraction>)>,
    cameras: Query<(&Camera, &Transform, &WorldMember), With<ZooCamera>>,
    habitats: Query<(&HabitatIndex, &TopologyGrid)>,
    tanks: Query<&TankGeometry, With<Tank>>,
    water_indexes: Query<(&TerrainWaterMembershipIndex, &WorldMember)>,
    water_regions: TerrainWaterRegionQuery,
    terrains: Res<Assets<TerrainAsset>>,
    mut effects: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let camera_water = (|| {
        let (_, transform, member) = cameras.iter().find(|(camera, ..)| camera.is_active)?;
        let eye = transform.translation;
        if let Ok((habitats, grid)) = habitats.get(member.root) {
            if let Some(tank) =
                locate_habitat_at_world_position(habitats, eye.xz(), *grid).filter(|&tank| {
                    tanks.get(tank).is_ok_and(|geometry| {
                        eye.y > geometry.floor_height && eye.y <= geometry.water_height
                    })
                })
            {
                return Some(tank);
            }
        }
        let (index, _) = water_indexes
            .iter()
            .find(|(_, owner)| owner.root == member.root)?;
        let terrain = terrains.get(index.terrain())?;
        let owner = index.owner_at_world_xz(eye.xz(), terrain)??;
        (eye.y <= water_regions.get(owner)?.surface_height(terrain)?).then_some(owner)
    })();
    for (parent, handle) in &draws {
        let Ok(AuthoredTankSurface::Panel { wet_sides, .. }) = panels.get(parent.parent()) else {
            continue;
        };
        let Some(material) = effects.get_mut_untracked(handle) else {
            continue;
        };
        let mut culling_changed = false;
        for (side, water) in wet_sides.iter().enumerate() {
            let Some(water) = water else { continue };
            let containment = if side == 0 {
                "CameraContainedCCW"
            } else {
                "CameraContainedCW"
            };
            culling_changed |= material
                .bind_d3d9_boolean_effect_semantic(containment, camera_water == Some(water.owner));
            let (registers, light) = water_registers.get(water.owner).ok().unzip();
            let wave = registers
                .copied()
                .unwrap_or_default()
                .particle_wave_surface(water.height);
            let semantics = if side == 0 {
                [
                    "DirtinessCCW",
                    "DepthOffsetCCW",
                    "DepthScaleCCW",
                    "FrequencyCCW",
                    "PhaseCCW",
                    "AmplitudeCCW",
                    "PosXCCW",
                    "PosYCCW",
                    "KCCW",
                    "RampOffsetCCW",
                    "RampScaleCCW",
                    "WaterColorCCW",
                ]
            } else {
                [
                    "DirtinessCW",
                    "DepthOffsetCW",
                    "DepthScaleCW",
                    "FrequencyCW",
                    "PhaseCW",
                    "AmplitudeCW",
                    "PosXCW",
                    "PosYCW",
                    "KCW",
                    "RampOffsetCW",
                    "RampScaleCW",
                    "WaterColorCW",
                ]
            };
            let values = [
                Vec4::splat(
                    water_quality
                        .get(water.owner)
                        .map_or(0.0, |(quality, fraction)| {
                            let value =
                                f32::from(quality.0) + fraction.map_or(0.0, |fraction| fraction.0);
                            (1000.0 - value.min(1000.0)) * 0.001
                        }),
                ),
                wave.depth_offset,
                wave.depth_scale,
                wave.frequency,
                wave.phase,
                wave.amplitude,
                wave.pos_x,
                wave.pos_y,
                wave.k,
                wave.ramp_offset,
                wave.ramp_scale,
            ];
            for (semantic, value) in semantics.into_iter().zip(values) {
                material.bind_float_vector_effect_semantic(semantic, value);
            }
            if let Some(base) = water.side_colour {
                let rgba = |colour: [u8; 4]| {
                    Vec4::from_array(colour.map(|value| f32::from(value) / 255.0))
                };
                let ambient = light
                    .flatten()
                    .map_or(Vec4::ONE, TerrainWaterRegionLight::raw_base_rgba);
                let base = rgba(base);
                let lit = water.lit_side_colour.map_or(base, rgba);
                let colour = base + (lit - base) * ambient;
                material.bind_float_vector_effect_semantic(semantics[11], colour);
            }
        }
        material.write_programmable_float_registers_to_persistent_buffers(&mut buffers);
        if culling_changed {
            let _ = effects.get_mut(handle.id());
        }
    }
}

pub(super) fn retire_tank_wall_panel(
    removed: On<Remove<(FenceEdge, FenceStart, FenceEnd, TankWallTopHeight)>>,
    mut commands: Commands,
    panels: Query<&TankWallPanels>,
) {
    if let Ok(panels) = panels.get(removed.entity) {
        for panel in panels.iter() {
            commands.entity(panel).try_despawn();
        }
    }
}

pub(super) fn retire_tank_surfaces_on_demotion(
    removed: On<Remove<Tank>>,
    mut commands: Commands,
    roots: Query<&TankSurfaceMeshes>,
) {
    if let Ok(roots) = roots.get(removed.entity) {
        for root in roots.iter() {
            commands.entity(root).try_despawn();
        }
    }
}

fn triangles_mesh(triangles: Vec<[Vec3; 3]>) -> Option<Mesh> {
    if triangles.is_empty() {
        return None;
    }
    let normals = triangles
        .iter()
        .flat_map(|[a, b, c]| [(*b - *a).cross(*c - *a).normalize_or_zero().to_array(); 3])
        .collect::<Vec<_>>();
    let texcoords = triangles
        .iter()
        .flatten()
        .map(|point| [point.x, -point.z])
        .collect::<Vec<_>>();
    let positions = triangles
        .into_iter()
        .flat_map(|triangle| triangle.map(|corner| corner.to_array()))
        .collect::<Vec<_>>();
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, texcoords.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, texcoords),
    )
}

fn build_tank_wall_panel_mesh(geometry: &TankWallPanelGeometry) -> Mesh {
    let [a, b] = geometry.ends;
    let [a_bottom, b_bottom] = geometry.bottom;
    let top = geometry.top.max(a_bottom).max(b_bottom);
    let [a0, b0, a1, b1] = [
        Vec3::new(a.x, -a.y, a_bottom),
        Vec3::new(b.x, -b.y, b_bottom),
        Vec3::new(a.x, -a.y, top),
        Vec3::new(b.x, -b.y, top),
    ];
    let axis = (b - a).normalize_or_zero();
    let u = [a, b].map(|point| point.dot(axis) * 0.1);
    let uv0 = vec![
        [u[0], 0.0, a_bottom],
        [u[1], 1.0, b_bottom],
        [u[1], 0.0, b_bottom],
        [u[0], 0.0, a_bottom],
        [u[0], 1.0, a_bottom],
        [u[1], 1.0, b_bottom],
    ];
    let uv1 = uv0.iter().map(|&[u, v, _]| [u, v]).collect::<Vec<_>>();
    let texcoord_with_floor = bevy::mesh::MeshVertexAttribute {
        format: VertexFormat::Float32x3,
        ..Mesh::ATTRIBUTE_UV_0
    };
    triangles_mesh(vec![[a0, b1, b0], [a0, a1, b1]])
        .expect("tank panel has two triangles")
        .with_inserted_attribute(texcoord_with_floor, uv0)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
}
/// Even-odd test against the unordered wall edges.
pub(super) fn inside_walls(point: Vec2, walls: &[[Vec2; 2]]) -> bool {
    walls
        .iter()
        .filter(|[a, b]| {
            (a.y > point.y) != (b.y > point.y)
                && point.x < a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x)
        })
        .count()
        % 2
        == 1
}

fn build_water_surface_mesh(grid: TopologyGrid, walls: &[[Vec2; 2]]) -> Option<Mesh> {
    if walls.is_empty() {
        return None;
    }
    let (minimum, maximum) = walls.iter().flatten().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(minimum, maximum), point| (minimum.min(*point), maximum.max(*point)),
    );
    let first = ((minimum - grid.origin) / grid.spacing_m)
        .round()
        .as_ivec2();
    let last = ((maximum - grid.origin) / grid.spacing_m)
        .round()
        .as_ivec2();
    // Walls run along cell sides or diagonals, so each half of a diagonal
    // cut holds two whole pinwheel triangles, whose centroids are never on a wall.
    let triangles = (first.y..last.y)
        .flat_map(|y| (first.x..last.x).map(move |x| IVec2::new(x, y)))
        .flat_map(|cell| {
            let minimum = grid.cell_translation(cell.extend(0)).xz();
            let corners = [Vec2::ZERO, Vec2::Y, Vec2::ONE, Vec2::X]
                .map(|corner| minimum + corner * grid.spacing_m);
            let centre = minimum + Vec2::splat(grid.spacing_m * 0.5);
            (0..4).filter_map(move |side| {
                let triangle = [corners[side], corners[(side + 1) % 4], centre];
                let centroid = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
                inside_walls(centroid, walls)
                    .then(|| triangle.map(|corner| Vec3::new(corner.x, 0.0, corner.y)))
            })
        })
        .collect();
    triangles_mesh(triangles)
}
