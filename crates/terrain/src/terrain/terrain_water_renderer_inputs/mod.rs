use std::collections::BTreeMap;
use std::time::Duration;

use super::tank_water_surfaces::{TankWaterDraw, TankWaterMaterial};
use super::terrain_water_renderer_targets::WaterCaptureAge;
use crate::terrain::terrain_water_material_binding::bind_authored_water_frame_base_tints;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::CameraProjection;
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::ecs::entity_disabling::Disabled;
use bevy::math::{FloatOrd, Vec3A};
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{Tank, TankGeometry};
use openzt2_zoo_state::environment::environment_state_types::EnvironmentMapSamples;
use openzt2_zoo_state::habitat::{
    habitat_membership_and_containment::locate_habitat_at_world_position,
    habitat_types::{HabitatCellOf, HabitatIndex},
};
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::{
    NativeTerrainWaterGeometricWaveShaderState, TerrainWaterWaveRegisters,
};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionQuery;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterSurfaceEffectPass;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterScumScroll;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfacePassKind;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfaceRegion;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterWaveStateKey;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::{
    AuthoredTerrainWaterRendererTargets, TerrainWaterCaptureExtent, TerrainWaterCaptureRole,
    TerrainWaterSurfaceCoverage, TerrainWaterSurfaceLocalFromWorld,
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

#[derive(Component)]
pub(super) struct TerrainWaterLowDetailTextureState {
    intervals: [Option<Timer>; 2],
    reverse: [bool; 2],
    transforms: [Mat4; 2],
}

#[derive(Component, Default)]
pub(super) struct WaterCaptureBoundsIndex {
    contributors: EntityHashMap<[FloatOrd; 6]>,
    coordinates: [BTreeMap<FloatOrd, usize>; 6],
}

impl WaterCaptureBoundsIndex {
    fn replace_bounds(&mut self, entity: Entity, bounds: Option<[FloatOrd; 6]>) {
        if self.contributors.get(&entity).copied() == bounds {
            return;
        }
        if let Some(previous) = self.contributors.remove(&entity) {
            for (axis, coordinate) in self.coordinates.iter_mut().zip(previous) {
                if let std::collections::btree_map::Entry::Occupied(mut entry) =
                    axis.entry(coordinate)
                {
                    *entry.get_mut() -= 1;
                    if *entry.get() == 0 {
                        entry.remove();
                    }
                }
            }
        }
        if let Some(bounds) = bounds {
            self.contributors.insert(entity, bounds);
            for (axis, coordinate) in self.coordinates.iter_mut().zip(bounds) {
                *axis.entry(coordinate).or_default() += 1;
            }
        }
    }

    fn combined_bounds(&self) -> Option<(Vec3, Vec3)> {
        let [min_x, min_y, min_z, max_x, max_y, max_z] = &self.coordinates;
        Some((
            Vec3::new(
                min_x.first_key_value()?.0 .0,
                min_y.first_key_value()?.0 .0,
                min_z.first_key_value()?.0 .0,
            ),
            Vec3::new(
                max_x.last_key_value()?.0 .0,
                max_y.last_key_value()?.0 .0,
                max_z.last_key_value()?.0 .0,
            ),
        ))
    }
}

#[derive(Component, Default, PartialEq)]
pub(super) struct WaterCaptureSceneBounds(Option<(Vec3, Vec3)>);

#[allow(clippy::type_complexity)]
pub(super) fn update_water_capture_scene_bounds(
    mut cameras: Query<
        (&mut WaterCaptureBoundsIndex, &mut WaterCaptureSceneBounds),
        With<ZooCamera>,
    >,
    changed: Query<
        Entity,
        (
            With<Mesh3d>,
            Or<(
                Changed<Mesh3d>,
                Changed<Aabb>,
                Changed<GlobalTransform>,
                Changed<RenderLayers>,
            )>,
        ),
    >,
    renderables: Query<(Entity, &Aabb, &GlobalTransform, Option<&RenderLayers>), With<Mesh3d>>,
    disabled: Query<Entity, (With<Mesh3d>, Added<Disabled>)>,
    (mut meshes, mut bounds, mut transforms, mut layers, mut enabled): (
        RemovedComponents<Mesh3d>,
        RemovedComponents<Aabb>,
        RemovedComponents<GlobalTransform>,
        RemovedComponents<RenderLayers>,
        RemovedComponents<Disabled>,
    ),
) {
    let changed: EntityHashSet = changed
        .iter()
        .chain(disabled.iter())
        .chain(meshes.read())
        .chain(bounds.read())
        .chain(transforms.read())
        .chain(layers.read())
        .chain(enabled.read())
        .collect();
    let Ok((mut index, mut scene)) = cameras.single_mut() else {
        return;
    };
    if changed.is_empty() && !index.is_added() {
        return;
    }
    let capture_layers = RenderLayers::from_layers(&[
        0,
        TERRAIN_RENDER_LAYER,
        WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
    ]);
    let projected = |bounds: &Aabb, transform: &GlobalTransform, layers: Option<&RenderLayers>| {
        if layers.is_some_and(|layers| !layers.intersects(&capture_layers)) {
            return None;
        }
        let centre = transform.transform_point(Vec3::from(bounds.center));
        let half = Vec3::from(bounds.half_extents);
        let matrix = transform.affine().matrix3;
        let extent = Vec3::from(matrix.x_axis.abs()) * half.x
            + Vec3::from(matrix.y_axis.abs()) * half.y
            + Vec3::from(matrix.z_axis.abs()) * half.z;
        let min = centre - extent;
        let max = centre + extent;
        Some([min.x, min.y, min.z, max.x, max.y, max.z].map(FloatOrd))
    };
    if index.is_added() {
        for (entity, bounds, transform, layers) in &renderables {
            index.replace_bounds(entity, projected(bounds, transform, layers));
        }
    } else {
        for entity in changed {
            let bounds = renderables
                .get(entity)
                .ok()
                .and_then(|(_, bounds, transform, layers)| projected(bounds, transform, layers));
            index.replace_bounds(entity, bounds);
        }
    }
    scene.set_if_neq(WaterCaptureSceneBounds(index.combined_bounds()));
}

#[derive(Component, Default, PartialEq)]
pub(super) struct WaterCaptureDemand {
    primary_plane: Option<f32>,
    high_detail: bool,
    fallback: bool,
}

#[allow(clippy::type_complexity)]
pub(super) fn sample_terrain_water_capture_demand(
    mut cameras: Query<
        (
            &TerrainWaterCameraPose,
            &TerrainWaterCaptureSelection,
            Option<&TerrainWaterCaptureRegion>,
            &mut WaterCaptureDemand,
            &WorldMember,
        ),
        With<ZooCamera>,
    >,
    surfaces: Query<(
        &MeshMaterial3d<EffectPassMaterial>,
        &TerrainWaterWaveStateKey,
        &ChildOf,
        &TerrainWaterSurfacePassKind,
        &TerrainWaterSurfaceCoverage,
    )>,
    groups: Query<&TerrainWaterSurfaceRegion>,
    regions: Query<(&TerrainWaterRegionFade, &TerrainWaterRegionLight)>,
    sources: TerrainWaterRegionQuery,
    terrains: Res<Assets<TerrainAsset>>,
    materials: Res<Assets<EffectPassMaterial>>,
    waves: Query<(), With<NativeTerrainWaterGeometricWaveShaderState>>,
    (tank_draws, tanks, habitats, cell_owners): (
        Query<(&TankWaterDraw, &TerrainWaterSurfacePassKind)>,
        Query<(&TankGeometry, &WorldMember), With<Tank>>,
        Query<(&HabitatIndex, &TopologyGrid)>,
        Query<&HabitatCellOf>,
    ),
) {
    let Ok((pose, selection, target, mut demand, world)) = cameras.single_mut() else {
        return;
    };
    let primary_plane = target.filter(|_| selection.resolved).and_then(|target| {
        let source = sources.get(target.0)?;
        source.surface_height(terrains.get(source.terrain())?)
    });
    let mut next = WaterCaptureDemand {
        primary_plane,
        ..default()
    };
    if let Some(pose) = pose.0 {
        if let Ok((habitats, grid)) = habitats.get(world.root) {
            let origin = pose.translation();
            let direction = *pose.forward();
            let mut distance =
                primary_plane.map_or(f32::INFINITY, |height| (height - origin.y) / direction.y);
            for (draw, kind) in &tank_draws {
                if !matches!(
                    kind,
                    TerrainWaterSurfacePassKind::Refraction | TerrainWaterSurfacePassKind::Scum
                ) {
                    continue;
                }
                let Ok((tank, owner)) = tanks.get(draw.tank) else {
                    continue;
                };
                if owner.root != world.root || tank.depth() <= 0.0 {
                    continue;
                }
                next.high_detail = true;
                next.fallback = true;
                let t = (tank.water_height - origin.y) / direction.y;
                if t.is_finite()
                    && t >= 0.0
                    && (t < distance || distance < 0.0)
                    && locate_habitat_at_world_position(
                        habitats,
                        &cell_owners,
                        (origin + direction * t).xz(),
                        *grid,
                    ) == Some(draw.tank)
                {
                    next.primary_plane = Some(tank.water_height);
                    distance = t;
                }
            }
        }
        for (material, key, parent, kind, coverage) in &surfaces {
            if kind.uses_stitch_geometry()
                || !coverage.0
                || !materials.contains(material.id())
                || !waves.contains(key.0)
            {
                continue;
            }
            let Some(fade) = groups
                .get(parent.parent())
                .ok()
                .and_then(|group| regions.get(group.0).ok())
                .map(|(fade, _)| fade)
            else {
                continue;
            };
            let has_material_definition = sources
                .get(key.0)
                .and_then(|source| source.biome(terrains.get(source.terrain())?))
                .is_some_and(|biome| biome.water_presentation.is_some());
            if has_material_definition {
                next.high_detail = true;
                next.fallback |= *kind == TerrainWaterSurfacePassKind::Scum
                    || (*kind == TerrainWaterSurfacePassKind::FallbackReflection
                        && fade.fraction() < 1.0);
                if next.fallback {
                    break;
                }
            }
        }
    }
    demand.set_if_neq(next);
}

type WaterCaptureFrame = (Transform, TerrainWaterCaptureProjection, Vec2);

#[derive(Component, Default)]
pub(super) struct PreparedWaterCapture {
    pose: Option<Transform>,
    reflection: Option<WaterCaptureFrame>,
    fallback: Option<WaterCaptureFrame>,
    refraction: Option<WaterCaptureFrame>,
}

pub(super) fn prepare_changed_water_capture_projections(
    mut cameras: Query<
        (
            Ref<Projection>,
            Ref<TerrainWaterCameraPose>,
            Ref<WaterCaptureDemand>,
            Ref<WaterCaptureSceneBounds>,
            &mut PreparedWaterCapture,
        ),
        With<ZooCamera>,
    >,
) {
    let Ok((projection, pose, demand, bounds, mut prepared)) = cameras.single_mut() else {
        return;
    };
    if !projection.is_changed()
        && !pose.is_changed()
        && !demand.is_changed()
        && !bounds.is_changed()
    {
        return;
    }
    let Some(pose) = pose
        .0
        .filter(|_| demand.high_detail)
        .map(|pose| pose.compute_transform())
    else {
        if prepared.pose.is_some() {
            *prepared = PreparedWaterCapture::default();
        }
        return;
    };
    let (minimum, maximum) = bounds
        .0
        .unwrap_or((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)));
    let plane = demand.primary_plane.unwrap_or(0.0);
    let fallback =
        construct_water_capture_projection(pose, &projection, 0.0, minimum, maximum, true);
    let reflection = if plane == 0.0 {
        fallback.clone()
    } else {
        construct_water_capture_projection(pose, &projection, plane, minimum, maximum, true)
            .or_else(|| fallback.clone())
    };
    let refraction =
        construct_water_capture_projection(pose, &projection, plane, minimum, maximum, false);
    *prepared = PreparedWaterCapture {
        pose: Some(pose),
        reflection,
        fallback,
        refraction,
    };
}

impl Default for TerrainWaterLowDetailTextureState {
    fn default() -> Self {
        Self {
            intervals: [None, None],
            reverse: [false; 2],
            transforms: [Mat4::IDENTITY; 2],
        }
    }
}
use openzt2_zoo_state::terrain::terrain_water_underwater_particle_types::TerrainWaterRegionLight;
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterCameraPose;
use openzt2_zoo_state::terrain::terrain_water_view_state::{
    TerrainWaterCaptureRegion, TerrainWaterCaptureSelection, TerrainWaterRegionFade,
    TerrainWaterRegionVisibility, TerrainWaterTrainingFade,
};

pub(super) fn initialize_authored_terrain_water_region_material_state(
    mut commands: Commands,
    mut random: ResMut<NativeVisualRandomStream>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    mut initialized: Query<&mut NativeTerrainWaterGeometricWaveShaderState>,
) {
    for (entity, source) in regions.iter() {
        let Some(presentation) = terrain_assets
            .get(source.terrain())
            .and_then(|asset| source.biome(asset))
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        if let Ok(mut state) = initialized.get_mut(entity) {
            state.reconfigure(
                presentation.surface_material.geometric_waves,
                presentation.surface_material.ripple_waves,
                &mut || random.unit_f64(),
            );
            continue;
        }
        commands.entity(entity).insert(
            NativeTerrainWaterGeometricWaveShaderState::from_authored_geometric_wave(
                presentation.surface_material.geometric_waves,
                presentation.surface_material.ripple_waves,
                &mut || random.unit_f64(),
            ),
        );
    }
}

pub(super) fn advance_authored_terrain_water_surface_effect_state(
    time: Res<Time>,
    mut random: ResMut<NativeVisualRandomStream>,
    mut capture_ages: Query<&mut WaterCaptureAge>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut waves: Query<(Entity, &mut NativeTerrainWaterGeometricWaveShaderState)>,
    regions: TerrainWaterRegionQuery,
    mut scrolls: Query<(Entity, &mut TerrainWaterScumScroll)>,
    mut low_textures: Query<(Entity, &mut TerrainWaterLowDetailTextureState)>,
    tanks: Query<&TankWaterMaterial, With<Tank>>,
) {
    let Ok(mut capture_age) = capture_ages.single_mut() else {
        return;
    };
    capture_age.0.tick(time.delta());
    for (entity, mut wave_state) in &mut waves {
        if regions.get(entity).is_none() && !tanks.contains(entity) {
            continue;
        }
        wave_state.advance_wave_channels(time.delta_secs(), &mut || random.unit_f64());
    }
    let water_for = |entity| {
        if let Ok(appearance) = tanks.get(entity) {
            let (water, ..) = terrain_assets
                .get(&appearance.terrain)?
                .tank_water_material_presentation(appearance.id)?;
            return Some(water);
        }
        let source = regions.get(entity)?;
        source
            .biome(terrain_assets.get(source.terrain())?)?
            .water_presentation
            .as_ref()
    };
    for (entity, mut state) in &mut low_textures {
        let Some(water) = water_for(entity) else {
            continue;
        };
        for (index, animation) in water.texture_animations.iter().enumerate() {
            let duration =
                Duration::try_from_secs_f32(animation.scroll_interval).unwrap_or_default();
            let interval = &mut state.intervals[index];
            let delta = if interval.is_some() {
                time.delta()
            } else {
                Duration::ZERO
            };
            let interval = interval.get_or_insert_with(|| Timer::new(duration, TimerMode::Once));
            if interval.duration() != duration {
                interval.set_duration(duration);
            }
            interval.tick(delta);
            if !animation.scroll_texture {
                continue;
            }
            let fraction = interval.fraction();
            if interval.is_finished() {
                interval.reset();
                if animation.ping_pong {
                    state.reverse[index] = !state.reverse[index];
                }
            }
            let start_min = Vec2::from_array(animation.start_minimum_uv);
            let start_max = Vec2::from_array(animation.start_maximum_uv);
            let end_min = Vec2::from_array(animation.end_minimum_uv);
            let end_max = Vec2::from_array(animation.end_maximum_uv);
            let (minimum, maximum) = if state.reverse[index] {
                (
                    end_min - (end_min - start_min) * fraction,
                    end_max - (end_max - start_max) * fraction,
                )
            } else {
                (
                    start_min + (end_min - start_min) * fraction,
                    start_max + (end_max - start_max) * fraction,
                )
            };
            let scale = maximum - minimum;
            state.transforms[index] = Mat4::from_cols(
                Vec4::X * scale.x,
                Vec4::Y * scale.y,
                Vec4::Z,
                minimum.extend(0.0).extend(1.0),
            );
        }
    }
    for (entity, mut scroll) in &mut scrolls {
        let Some(scum) = water_for(entity)
            .and_then(|presentation| presentation.surface_material.configuration.scum.as_ref())
        else {
            continue;
        };
        let (Some(bottom_size), Some(top_size), Some(bottom_rate), Some(top_rate)) = (
            scum.bottom_size,
            scum.top_size,
            scum.bottom_rate,
            scum.top_rate,
        ) else {
            continue;
        };
        let rates = bottom_rate.into_iter().chain(top_rate);
        let sizes = bottom_size.into_iter().chain(top_size);
        let mut channels = scroll.offsets.to_array();
        for ((offset, rate), size) in channels.iter_mut().zip(rates).zip(sizes) {
            *offset += if size == 0.0 { rate } else { rate / size } * time.delta_secs();
            if *offset > 1.0 {
                *offset -= 1.0;
            }
            if *offset < 0.0 {
                *offset += 1.0;
            }
        }
        scroll.offsets = Vec4::from_array(channels);
    }
}

pub(super) fn project_authored_terrain_water_surface_coverage(
    mut commands: Commands,
    zoo_camera: Query<
        (&Projection, Option<&TerrainWaterCameraPose>),
        (With<ZooCamera>, With<Camera>),
    >,
    waves: Query<Ref<TerrainWaterWaveRegisters>>,
    groups: Query<&TerrainWaterSurfaceRegion>,
    region_sources: TerrainWaterRegionQuery,
    mut regions: Query<(Entity, &mut TerrainWaterRegionVisibility)>,
    mut surfaces: Query<(
        Entity,
        &AuthoredTerrainWaterSurfaceEffectPass,
        &TerrainWaterWaveStateKey,
        &ChildOf,
        &mut TerrainWaterSurfaceCoverage,
        &TerrainWaterSurfacePassKind,
        &TerrainWaterSurfaceLocalFromWorld,
        &mut Aabb,
        Has<NoFrustumCulling>,
    )>,
) {
    let frustum = zoo_camera.single().ok().and_then(|(projection, pose)| {
        pose.and_then(|pose| pose.0)
            .map(|transform| projection.compute_frustum(&transform))
    });
    let mut visible_regions = bevy::ecs::entity::EntityHashSet::default();
    for (
        entity,
        surface,
        key,
        parent,
        mut coverage,
        kind,
        coordinates,
        mut local_bounds,
        uncullable,
    ) in &mut surfaces
    {
        let world_bounds = if kind.uses_stitch_geometry() {
            Some(surface.world_bounds(Vec2::ZERO))
        } else {
            waves
                .get(key.0)
                .ok()
                .and_then(|wave| surface.wave_expanded_world_bounds(&wave))
        };
        if let Some(bounds) = world_bounds {
            let matrix = coordinates.0;
            let centre = matrix.transform_point3(Vec3::from(bounds.center));
            let half = Vec3::from(bounds.half_extents);
            let extent = matrix.x_axis.truncate().abs() * half.x
                + matrix.y_axis.truncate().abs() * half.y
                + matrix.z_axis.truncate().abs() * half.z;
            let next = Aabb::from_min_max(centre - extent, centre + extent);
            if local_bounds.center != next.center || local_bounds.half_extents != next.half_extents
            {
                *local_bounds = next;
            }
            if uncullable {
                commands.entity(entity).remove::<NoFrustumCulling>();
            }
        } else if !uncullable {
            commands.entity(entity).insert(NoFrustumCulling);
        }
        let active_owner = groups
            .get(parent.parent())
            .ok()
            .filter(|group| region_sources.get(group.0).is_some());
        let visible = active_owner.is_some()
            && (kind.uses_stitch_geometry() || waves.contains(key.0))
            && frustum.as_ref().is_some_and(|frustum| {
                world_bounds.is_none_or(|bounds| frustum.intersects_obb_identity(&bounds))
            });
        coverage.set_if_neq(TerrainWaterSurfaceCoverage(visible));
        if visible {
            if let Some(group) = active_owner {
                visible_regions.insert(group.0);
            }
        }
    }
    for (owner, mut visibility) in &mut regions {
        let visible = visible_regions.contains(&owner);
        if visibility.0 != visible {
            visibility.0 = visible;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_authored_terrain_water_renderer_inputs(
    environment_maps: Query<(&WorldMember, Ref<EnvironmentMapSamples>)>,
    waves: Query<Ref<TerrainWaterWaveRegisters>>,
    phase: Res<State<openzt2_game_assets::application_lifecycle::GamePhase>>,
    targets: Query<(
        &AuthoredTerrainWaterRendererTargets,
        &Children,
        &WaterCaptureAge,
    )>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    zoo_camera: Query<
        (
            Option<&TerrainWaterCameraPose>,
            &WaterCaptureDemand,
            &PreparedWaterCapture,
            Option<&TerrainWaterTrainingFade>,
            Ref<WorldMember>,
            Ref<Camera>,
        ),
        With<ZooCamera>,
    >,
    mut camera_transforms: Query<
        (
            &TerrainWaterCaptureRole,
            &mut Camera,
            &mut Projection,
            &mut Transform,
            &mut TerrainWaterCaptureExtent,
        ),
        Without<ZooCamera>,
    >,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    region_sources: TerrainWaterRegionQuery,
    regions: Query<(
        Option<&TerrainWaterScumScroll>,
        &TerrainWaterRegionFade,
        Option<&TerrainWaterRegionLight>,
        Option<&TerrainWaterLowDetailTextureState>,
    )>,
    surface_groups: Query<&TerrainWaterSurfaceRegion>,
    mut water_surfaces: Query<
        (
            Ref<MeshMaterial3d<EffectPassMaterial>>,
            Ref<AuthoredTerrainWaterSurfaceEffectPass>,
            Ref<TerrainWaterWaveStateKey>,
            &ChildOf,
            &mut Visibility,
            &TerrainWaterSurfacePassKind,
            &TerrainWaterSurfaceCoverage,
        ),
        With<AuthoredTerrainWaterSurfaceEffectPass>,
    >,
    (mut tank_draws, tanks): (
        Query<
            (
                &TankWaterDraw,
                &TerrainWaterSurfacePassKind,
                Ref<MeshMaterial3d<EffectPassMaterial>>,
                &mut Visibility,
            ),
            Without<AuthoredTerrainWaterSurfaceEffectPass>,
        >,
        Query<
            (
                Ref<TankGeometry>,
                &WorldMember,
                &TankWaterMaterial,
                Option<&TerrainWaterScumScroll>,
                Option<&TerrainWaterRegionLight>,
                Option<&TerrainWaterLowDetailTextureState>,
            ),
            With<Tank>,
        >,
    ),
) {
    let Ok((targets, captures, age)) = targets.single() else {
        return;
    };
    let active_camera = zoo_camera
        .iter()
        .find(|(_, _, _, _, _, camera)| camera.is_active);
    let selected_camera = active_camera
        .as_ref()
        .map(|(pose, demand, prepared, fade, world, _)| {
            (*pose, *demand, *prepared, *fade, &**world)
        });
    let training_opacity_mix = selected_camera
        .and_then(|(_, _, _, fade, _)| fade)
        .map_or(0.0, |fade| fade.0);
    let camera = selected_camera.and_then(|(pose, ..)| pose?.0);
    let environment_map = active_camera
        .as_ref()
        .and_then(|(_, _, _, _, owner, camera)| {
            let (_, maps) = environment_maps
                .iter()
                .find(|(member, _)| member.root == owner.root)?;
            let changed = maps.is_changed() || owner.is_changed() || camera.is_changed();
            Some((maps.into_inner().water.texture()?, changed))
        });
    let mut visible_count = 0;
    for (handle, surface, key, parent, mut visibility, kind, coverage) in &mut water_surfaces {
        let visible = (|| {
            if !coverage.0 {
                return None;
            }
            let (_, fade, light, textures) = regions
                .get(surface_groups.get(parent.parent()).ok()?.0)
                .ok()?;
            if *kind == TerrainWaterSurfacePassKind::MediumEnvironment {
                let (texture, changed) = environment_map?;
                if changed || handle.is_changed() || *visibility == Visibility::Hidden {
                    effect_pass_materials
                        .get_mut(handle.id())?
                        .replace_texture_asset_at_d3d9_stage(0, texture.clone());
                }
                return Some(true);
            }
            if kind.uses_stitch_geometry() {
                let textures = textures?;
                let material = effect_pass_materials.get_mut_untracked(handle.id())?;
                material.set_fixed_function_ambient_colour(
                    light.map_or(Vec4::ONE, TerrainWaterRegionLight::raw_base_rgba),
                );
                material.set_fixed_function_texture_transform(0, textures.transforms[0]);
                if *kind == TerrainWaterSurfacePassKind::MediumDetail {
                    material.set_fixed_function_texture_transform(1, textures.transforms[1]);
                }
                material.write_changed_fixed_function_registers_to_persistent_buffer(
                    &mut effect_uniform_buffers,
                );
                material.write_changed_fixed_function_transforms_to_persistent_buffer(
                    &mut effect_uniform_buffers,
                );
                return Some(true);
            }
            let pose = camera.as_ref()?;
            waves.get(key.0).ok()?;
            let source = region_sources.get(key.0)?;
            let terrain = terrain_assets.get(source.terrain())?;
            let presentation = source.biome(terrain)?.water_presentation.as_ref()?;
            let fraction = fade.fraction();
            let transitioning = fraction > 0.0 && fraction < 1.0;
            let strength = presentation.surface_material.reflection.strength_percent;
            let above = pose.translation().y > surface.surface_plane_height;
            Some(match kind {
                TerrainWaterSurfacePassKind::Scum => true,
                TerrainWaterSurfacePassKind::FallbackReflection => {
                    transitioning || (fraction <= 0.0 && strength > 5.0)
                }
                TerrainWaterSurfacePassKind::Refraction => fraction > 0.0 && above,
                TerrainWaterSurfacePassKind::Reflection => {
                    transitioning || (fraction >= 1.0 && strength > 5.0)
                }
                _ => false,
            })
        })()
        .unwrap_or(false);
        visible_count += usize::from(visible);
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (draw, kind, handle, mut visibility) in &mut tank_draws {
        let visible = (|| {
            let (tank, world, appearance, _, light, textures) = tanks.get(draw.tank).ok()?;
            let (_, demand, _, _, camera_world) = selected_camera?;
            if tank.depth() <= 0.0 || world.root != camera_world.root {
                return None;
            }
            if *kind == TerrainWaterSurfacePassKind::MediumEnvironment {
                let (texture, changed) = environment_map?;
                if changed || handle.is_changed() || *visibility == Visibility::Hidden {
                    effect_pass_materials
                        .get_mut(handle.id())?
                        .replace_texture_asset_at_d3d9_stage(0, texture.clone());
                }
                return Some(true);
            }
            if kind.uses_stitch_geometry() {
                let material = effect_pass_materials.get_mut_untracked(handle.id())?;
                let textures = textures?;
                material.set_fixed_function_ambient_colour(
                    light.map_or(Vec4::ONE, TerrainWaterRegionLight::raw_base_rgba),
                );
                material.set_fixed_function_texture_transform(0, textures.transforms[0]);
                if *kind == TerrainWaterSurfacePassKind::MediumDetail {
                    material.set_fixed_function_texture_transform(1, textures.transforms[1]);
                }
                material.write_changed_fixed_function_registers_to_persistent_buffer(
                    &mut effect_uniform_buffers,
                );
                material.write_changed_fixed_function_transforms_to_persistent_buffer(
                    &mut effect_uniform_buffers,
                );
                return Some(true);
            }
            let above = camera?.translation().y > tank.water_height;
            let primary = demand.primary_plane == Some(tank.water_height);
            let (water, ..) = terrain_assets
                .get(&appearance.terrain)?
                .tank_water_material_presentation(appearance.id)?;
            let reflection = water.surface_material.reflection.strength_percent > 5.0;
            Some(match kind {
                TerrainWaterSurfacePassKind::Scum => true,
                TerrainWaterSurfacePassKind::Reflection => primary && reflection,
                TerrainWaterSurfacePassKind::FallbackReflection => !primary && reflection,
                TerrainWaterSurfacePassKind::Refraction => primary && above,
                _ => false,
            })
        })()
        .unwrap_or(false);
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let Some((demand, prepared, zoo_transform)) = selected_camera
        .and_then(|(_, demand, prepared, ..)| Some((demand, prepared, prepared.pose?)))
    else {
        let mut frames = camera_transforms.iter_many_mut(captures.iter());
        while let Some(item) = frames.fetch_next() {
            let Ok((_, mut camera, ..)) = item else {
                continue;
            };
            if camera.is_active {
                camera.is_active = false;
            }
        }
        return;
    };
    let reflection_capture = &prepared.reflection;
    let fallback_capture = &prepared.fallback;
    let refraction_capture = &prepared.refraction;
    let water_plane = demand.primary_plane;
    let refraction_enabled =
        water_plane.is_some_and(|height| zoo_transform.translation.y >= height);
    for handle in &targets.water_bump_combination_materials {
        let Some(material) = effect_pass_materials.get_mut_untracked(handle) else {
            continue;
        };
        let first_axis_offset = (age.0.elapsed_secs() * 0.1).fract();
        let second_axis_offset = (age.0.elapsed_secs() * 0.05).fract();
        material.bind_float_vector_effect_semantic(
            "TextureOffset",
            Vec4::new(
                first_axis_offset,
                first_axis_offset,
                second_axis_offset,
                second_axis_offset,
            ),
        );
        material
            .write_programmable_float_registers_to_persistent_buffers(&mut effect_uniform_buffers);
    }
    let mut activation_changed = false;
    let mut reflection_extent = Vec2::splat(50.0);
    let mut fallback_extent = Vec2::splat(50.0);
    let mut refraction_extent = Vec2::splat(50.0);
    let mut frames = camera_transforms.iter_many_mut(captures.iter());
    while let Some(item) = frames.fetch_next() {
        let Ok((role, mut camera, mut projection, mut transform, mut extent)) = item else {
            continue;
        };
        let (capture, demand, world_extent) = match role {
            TerrainWaterCaptureRole::Bump => {
                if !camera.is_active {
                    camera.is_active = true;
                    activation_changed = true;
                }
                continue;
            }
            TerrainWaterCaptureRole::Reflection => (
                reflection_capture.as_ref(),
                water_plane.is_some(),
                &mut reflection_extent,
            ),
            TerrainWaterCaptureRole::Refraction => (
                refraction_capture.as_ref(),
                refraction_enabled,
                &mut refraction_extent,
            ),
            TerrainWaterCaptureRole::FallbackReflection => (
                fallback_capture.as_ref(),
                demand.fallback,
                &mut fallback_extent,
            ),
        };
        let active = reflection_capture.is_some() && demand && capture.is_some();
        if camera.is_active != active {
            camera.is_active = active;
            activation_changed = true;
        }
        if let Some((frame, capture_projection, size)) = capture {
            if transform.set_if_neq(*frame)
                || projection.get_clip_from_view() != capture_projection.get_clip_from_view()
            {
                *projection = Projection::custom(capture_projection.clone());
            }
            extent.set_if_neq(TerrainWaterCaptureExtent(*size));
        }
        *world_extent = extent.0;
    }
    if activation_changed || phase.is_changed() {
        info!(target: "openzt2_water_targets", enabled = demand.high_detail,
            surface_pass_count = water_surfaces.iter().count(),
            visible_surface_pass_count = visible_count,
            "water render-target demand");
    }
    let Some((reflected_transform, reflection_projection, _)) = reflection_capture else {
        return;
    };
    let source_from_local = Mat4::from_cols(Vec4::X, -Vec4::Z, Vec4::Y, Vec4::W);
    let source_to_refraction_clip =
        refraction_capture
            .as_ref()
            .map_or(Mat4::IDENTITY, |(transform, projection, _)| {
                projection.get_clip_from_view()
                    * transform.to_matrix().inverse()
                    * source_from_local
            });
    let reflection_world_to_view = reflected_transform.to_matrix().inverse();
    let source_to_reflection_clip =
        reflection_projection.get_clip_from_view() * reflection_world_to_view * source_from_local;
    let clip_to_uv = Mat4::from_cols(
        Vec4::new(0.5, 0.0, 0.0, 0.0),
        Vec4::new(0.0, -0.5, 0.0, 0.0),
        Vec4::Z,
        Vec4::new(0.5, 0.5, 0.0, 1.0),
    );
    let fallback_clip =
        fallback_capture
            .as_ref()
            .map_or(Mat4::IDENTITY, |(frame, projection, _)| {
                projection.get_clip_from_view() * frame.to_matrix().inverse() * source_from_local
            });
    for (draw, kind, handle, visibility) in &mut tank_draws {
        if kind.uses_stitch_geometry() || *visibility == Visibility::Hidden {
            continue;
        }
        let (Ok((geometry, _, appearance, scroll, light, _)), Ok(wave_state)) =
            (tanks.get(draw.tank), waves.get(draw.tank))
        else {
            continue;
        };
        let Some((presentation, ..)) = terrain_assets
            .get(&appearance.terrain)
            .and_then(|terrain| terrain.tank_water_material_presentation(appearance.id))
        else {
            continue;
        };
        let Some(material) = effect_pass_materials.get_mut_untracked(handle.id()) else {
            continue;
        };
        let fallback = matches!(
            kind,
            TerrainWaterSurfacePassKind::FallbackReflection | TerrainWaterSurfacePassKind::Scum
        );
        let above = zoo_transform.translation.y > geometry.water_height;
        let reflection_size = Vec2::splat(25.0)
            / if fallback {
                fallback_extent
            } else {
                reflection_extent
            };
        let refraction_size = Vec2::splat(25.0) / refraction_extent;
        material.bind_float_vector_effect_semantic(
            "ReflectSize",
            Vec4::new(reflection_size.x, reflection_size.y, 1.0, 0.0),
        );
        material.bind_float_vector_effect_semantic(
            "RefractSize",
            Vec4::new(refraction_size.x, refraction_size.y, 1.0, 0.0),
        );
        let ambient = light.map_or(Vec3::ONE, |light| light.raw_base_rgba().truncate());
        bind_authored_water_frame_base_tints(
            material,
            presentation,
            ambient,
            !above || fallback,
            (!fallback).then_some(1.0),
            (!above && !fallback && scroll.is_none()).then_some(1.0),
            training_opacity_mix,
        );
        if let (Some(scroll), Some(light)) = (scroll, light) {
            bind_water_scum(material, presentation, scroll, light, training_opacity_mix);
        }
        material.bind_float_matrix_effect_semantic(
            "WorldToRefract",
            clip_to_uv * source_to_refraction_clip,
        );
        material.bind_float_matrix_effect_semantic(
            "WorldToReflect",
            clip_to_uv
                * if fallback {
                    fallback_clip
                } else {
                    source_to_reflection_clip
                },
        );
        if wave_state.is_changed()
            || geometry.is_changed()
            || handle.is_changed()
            || visibility.is_changed()
        {
            wave_state.bind_wave_channels_to_effect_pass(geometry.water_height, material);
        }
        material
            .write_programmable_float_registers_to_persistent_buffers(&mut effect_uniform_buffers);
    }
    for (material_handle, surface, wave_key, parent, visibility, pass_kind, _) in
        &mut water_surfaces
    {
        if pass_kind.uses_stitch_geometry() {
            continue;
        }
        let Ok(geometric_wave_state) = waves.get(wave_key.0) else {
            continue;
        };
        if *visibility == Visibility::Hidden {
            continue;
        }
        let Some(material) = effect_pass_materials.get_mut_untracked(material_handle.id()) else {
            continue;
        };
        let fallback = *pass_kind == TerrainWaterSurfacePassKind::FallbackReflection
            || *pass_kind == TerrainWaterSurfacePassKind::Scum;
        let reflection_size = Vec2::splat(25.0)
            / if fallback {
                fallback_extent
            } else {
                reflection_extent
            };
        let refraction_size = Vec2::splat(25.0) / refraction_extent;
        material.bind_float_vector_effect_semantic(
            "ReflectSize",
            Vec4::new(reflection_size.x, reflection_size.y, 1.0, 0.0),
        );
        material.bind_float_vector_effect_semantic(
            "RefractSize",
            Vec4::new(refraction_size.x, refraction_size.y, 1.0, 0.0),
        );
        let Some((scroll, fade, light, _)) =
            surface_groups.get(parent.parent()).ok().and_then(|owner| {
                region_sources
                    .get(owner.0)
                    .and_then(|_| regions.get(owner.0).ok())
            })
        else {
            continue;
        };
        let ambient_colour = light.map_or(Vec3::ONE, |light| light.raw_base_rgba().truncate());
        let Some(source) = region_sources.get(wave_key.0) else {
            continue;
        };
        if let Some(presentation) = terrain_assets
            .get(source.terrain())
            .and_then(|asset| source.biome(asset))
            .and_then(|biome| biome.water_presentation.as_ref())
        {
            let fraction = fade.fraction();
            let above = zoo_transform.translation.y > surface.surface_plane_height;
            bind_authored_water_frame_base_tints(
                material,
                presentation,
                ambient_colour,
                !above || fallback,
                (!fallback && fraction > 0.0).then_some(fraction),
                (!above && scroll.is_none() && fraction > 0.0).then_some(fraction),
                training_opacity_mix,
            );
            if let (Some(scroll), Some(light)) = (scroll, light) {
                bind_water_scum(material, presentation, scroll, light, training_opacity_mix);
            }
        }
        material.bind_float_matrix_effect_semantic(
            "WorldToRefract",
            clip_to_uv * source_to_refraction_clip,
        );
        material.bind_float_matrix_effect_semantic(
            "WorldToReflect",
            clip_to_uv
                * if fallback {
                    fallback_capture
                        .as_ref()
                        .map_or(Mat4::IDENTITY, |(frame, projection, _)| {
                            projection.get_clip_from_view()
                                * frame.to_matrix().inverse()
                                * source_from_local
                        })
                } else {
                    source_to_reflection_clip
                },
        );
        if geometric_wave_state.is_changed()
            || surface.is_changed()
            || wave_key.is_changed()
            || material_handle.is_changed()
            || visibility.is_changed()
        {
            geometric_wave_state
                .bind_wave_channels_to_effect_pass(surface.surface_plane_height, material);
        }
        material
            .write_programmable_float_registers_to_persistent_buffers(&mut effect_uniform_buffers);
    }
}

fn bind_water_scum(
    material: &mut EffectPassMaterial,
    presentation: &openzt2_game_data::terrain::TerrainBiomeWaterPresentation,
    scroll: &TerrainWaterScumScroll,
    light: &TerrainWaterRegionLight,
    training_opacity_mix: f32,
) {
    let Some((bottom, top)) = presentation
        .surface_material
        .configuration
        .scum
        .as_ref()
        .and_then(|scum| scum.bottom_size.zip(scum.top_size))
    else {
        return;
    };
    let offsets = scroll.offsets;
    material.bind_float_vector_effect_semantic(
        "ScumOffsetLo",
        Vec4::new(bottom[0].recip(), bottom[1].recip(), offsets.x, offsets.y),
    );
    material.bind_float_vector_effect_semantic(
        "ScumOffsetHi",
        Vec4::new(top[0].recip(), top[1].recip(), offsets.z, offsets.w),
    );
    material.bind_float_vector_effect_semantic("ScumAmbient", Vec4::W);
    material.bind_float_vector_effect_semantic(
        "ScumLightDir",
        light.source_light_direction().extend(0.0),
    );
    material.bind_float_vector_effect_semantic(
        "ScumDiffuse",
        light.raw_base_rgba().truncate().extend(1.0),
    );
    material.bind_float_vector_effect_semantic(
        "ReflectOpacity",
        Vec4::new(1.0, 0.5 - 1.5 * training_opacity_mix, 0.0, 0.05),
    );
}

#[derive(Clone, Debug)]
struct TerrainWaterCaptureProjection {
    slopes: Vec4,
    near: f32,
    far: f32,
    invert_horizontal: bool,
}

impl CameraProjection for TerrainWaterCaptureProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        let width = self.slopes.y - self.slopes.x;
        let height = self.slopes.w - self.slopes.z;
        let depth = self.far - self.near;
        let horizontal_sign = if self.invert_horizontal { -1.0 } else { 1.0 };
        Mat4::from_cols(
            Vec4::new(horizontal_sign * 2.0 / width, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 / height, 0.0, 0.0),
            Vec4::new(
                horizontal_sign * (self.slopes.y + self.slopes.x) / width,
                (self.slopes.w + self.slopes.z) / height,
                self.near / depth,
                -1.0,
            ),
            Vec4::new(0.0, 0.0, self.near * self.far / depth, 0.0),
        )
    }

    fn get_clip_from_view_for_sub(&self, view: &bevy::camera::SubCameraView) -> Mat4 {
        let minimum = view.offset / view.full_size.as_vec2();
        let maximum = (view.offset + view.size.as_vec2()) / view.full_size.as_vec2();
        let width = self.slopes.y - self.slopes.x;
        let height = self.slopes.w - self.slopes.z;
        Self {
            slopes: Vec4::new(
                self.slopes.x + minimum.x * width,
                self.slopes.x + maximum.x * width,
                self.slopes.w - maximum.y * height,
                self.slopes.w - minimum.y * height,
            ),
            ..self.clone()
        }
        .get_clip_from_view()
    }

    fn update(&mut self, _width: f32, _height: f32) {}

    fn far(&self) -> f32 {
        self.far
    }

    fn get_frustum_corners(&self, near: f32, far: f32) -> [Vec3A; 8] {
        std::array::from_fn(|index| {
            let distance = if index < 4 { near.abs() } else { far.abs() };
            let x = if index % 4 < 2 {
                self.slopes.y
            } else {
                self.slopes.x
            };
            let y = if matches!(index % 4, 1 | 2) {
                self.slopes.w
            } else {
                self.slopes.z
            };
            Vec3A::new(x * distance, y * distance, -distance)
        })
    }
}

fn construct_water_capture_projection(
    source: Transform,
    projection: &Projection,
    plane_height: f32,
    minimum: Vec3,
    maximum: Vec3,
    reflection: bool,
) -> Option<(Transform, TerrainWaterCaptureProjection, Vec2)> {
    if !minimum.is_finite()
        || !maximum.is_finite()
        || (!reflection && source.translation.y < plane_height)
    {
        return None;
    }
    let mut frame = source;
    if reflection && frame.translation.y >= plane_height {
        frame.translation.y = 2.0 * plane_height - frame.translation.y;
        let forward = source.forward();
        let up = source.up();
        frame = frame.looking_to(
            Vec3::new(forward.x, -forward.y, forward.z),
            Vec3::new(up.x, -up.y, up.z),
        );
    }
    let far_distance = (0..8)
        .map(|corner| {
            let point = Vec3::new(
                if corner & 1 == 0 {
                    minimum.x
                } else {
                    maximum.x
                },
                if corner & 2 == 0 {
                    minimum.y
                } else {
                    maximum.y
                },
                if corner & 4 == 0 {
                    minimum.z
                } else {
                    maximum.z
                },
            );
            (point - frame.translation).dot(*frame.forward())
        })
        .fold(0.0_f32, f32::max)
        .min(projection.far());
    if far_distance <= 0.1 {
        return None;
    }
    let normal = if reflection { Vec3::Y } else { Vec3::NEG_Y };
    let candidate = [*frame.right(), *frame.up(), *frame.forward()]
        .into_iter()
        .min_by(|left, right| left.dot(normal).abs().total_cmp(&right.dot(normal).abs()))?;
    let up = candidate.cross(normal).normalize();
    let capture = Transform::from_translation(frame.translation).looking_to(normal, up);
    let near = (plane_height - frame.translation.y).abs();
    if near == 0.0 {
        return None;
    }
    let corners = projection.get_frustum_corners(-0.1, -far_distance);
    let mut slopes = Vec4::new(
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
    );
    for (index, corner) in [7, 4, 5, 6].into_iter().enumerate() {
        let mut point = frame.transform_point(Vec3::from(corners[corner]));
        let delta = point - frame.translation;
        let numerator = plane_height - frame.translation.y;
        if numerator * delta.y > 0.000_001 {
            point = frame.translation + delta * (numerator / delta.y).min(1.0);
        }
        point.y = plane_height;
        let local = point - capture.translation;
        let x = local.dot(*capture.right()) / near;
        let y = local.dot(*capture.up()) / near;
        slopes.x = slopes.x.min(x);
        slopes.y = slopes.y.max(x);
        slopes.z = slopes.z.min(y - if index == 0 { 0.0 } else { 1.0 });
        slopes.w = slopes.w.max(y);
    }
    for (lower, upper) in [(0, 1), (2, 3)] {
        let extent = near * (slopes[upper] - slopes[lower]);
        if extent < 50.0 {
            let margin = (50.0 - extent) * 0.5 / near;
            slopes[lower] -= margin;
            slopes[upper] += margin;
        }
    }
    let extent = Vec2::new(slopes.y - slopes.x, slopes.w - slopes.z) * near;
    Some((
        capture,
        TerrainWaterCaptureProjection {
            slopes,
            near,
            far: near + 1000.0,
            invert_horizontal: reflection,
        },
        extent,
    ))
}

#[cfg(test)]
mod water_target_demand_tests {
    use super::*;
    use bevy::camera::CameraProjection;
    use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;

    #[test]
    fn capture_near_plane_is_the_water_surface_with_reversed_depth() {
        let source = Transform::from_xyz(0.0, 20.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z);
        let projection = Projection::Perspective(PerspectiveProjection::default());
        for reflection in [false, true] {
            let (camera, capture, extent) = construct_water_capture_projection(
                source,
                &projection,
                3.0,
                Vec3::splat(-100.0),
                Vec3::splat(100.0),
                reflection,
            )
            .expect("the scene lies in the source view");
            let near_world = camera.transform_point(Vec3::new(0.0, 0.0, -capture.near));
            assert!((near_world.y - 3.0).abs() < 0.0001);
            let near_clip = capture.get_clip_from_view() * Vec4::new(0.0, 0.0, -capture.near, 1.0);
            let far_clip = capture.get_clip_from_view() * Vec4::new(0.0, 0.0, -capture.far, 1.0);
            assert!((near_clip.z / near_clip.w - 1.0).abs() < 0.0001);
            assert!((far_clip.z / far_clip.w).abs() < 0.0001);
            assert!(extent.x >= 49.999 && extent.y >= 49.999);
        }
    }

    #[test]
    fn water_bounds_leave_and_reenter_the_view_without_a_centre_ray_hit() {
        use openzt2_game_data::terrain::TerrainWaterGeometricWavePresentation;
        use openzt2_game_data::terrain::TerrainWaterRippleWavePresentation;
        let mut random = NativeVisualRandomStream::from_seed(7);
        let wave_state = NativeTerrainWaterGeometricWaveShaderState::from_authored_geometric_wave(
            TerrainWaterGeometricWavePresentation {
                minimum_amplitude_percent: 10.0,
                maximum_amplitude_percent: 20.0,
                chop_percent: 25.0,
            },
            TerrainWaterRippleWavePresentation {
                lifespan_seconds: 4.0,
                startup_seconds: 1.0,
                minimum_amplitude_percent: 10.0,
                maximum_amplitude_percent: 20.0,
                chop_percent: 25.0,
                speed_metres_per_second: 1.0,
                ramp_minimum_metres: 1.0,
                ramp_maximum_metres: 5.0,
            },
            &mut || random.unit_f64(),
        );
        let mut app = App::new();
        let region = app.world_mut().spawn(wave_state).id();
        app.add_systems(Update, openzt2_zoo_state::terrain::terrain_water_wave_register_projection::publish_terrain_water_wave_registers);
        app.update();
        let registers = app
            .world()
            .get::<TerrainWaterWaveRegisters>(region)
            .unwrap();
        let projection = PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.0,
            near: 0.1,
            far: 100.0,
            ..default()
        };
        let camera = GlobalTransform::from(
            Transform::from_xyz(0.0, 20.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
        );
        let surface = AuthoredTerrainWaterSurfaceEffectPass {
            surface_plane_height: 0.0,
            horizontal_minimum: Vec2::new(15.0, 0.0),
            horizontal_maximum: Vec2::new(18.0, 2.0),
            maximum_absolute_input_height: 0.0,
        };
        assert!(surface.intersects_camera_frustum(&projection.compute_frustum(&camera), registers));
        let moved_camera = GlobalTransform::from(
            Transform::from_xyz(-60.0, 20.0, 0.0)
                .looking_at(Vec3::new(-60.0, 0.0, 0.0), Vec3::NEG_Z),
        );
        assert!(!surface
            .intersects_camera_frustum(&projection.compute_frustum(&moved_camera), registers));
        assert!(surface.intersects_camera_frustum(&projection.compute_frustum(&camera), registers));
    }
}
