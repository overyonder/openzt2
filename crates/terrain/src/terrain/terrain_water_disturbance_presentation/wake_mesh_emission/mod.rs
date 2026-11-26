use bevy::asset::RenderAssetUsages;
use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use openzt2_game_assets::effect::explicit_particle_emission::ParticleEffectPointEmitter;
use openzt2_game_assets::effect::explicit_particle_emission::{
    ParticleAliveCount, ParticleSpawnBatch,
};
use openzt2_game_assets::effect::particle_wave_surface::ParticleEffectWaveSurface;
use openzt2_game_assets::effect::ParticleEffectDocumentAsset;
use openzt2_game_assets::graphics_settings_types::{FourLevelGraphicsDetail, GraphicsSettings};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::{
    TerrainWaterWaveRegisters, WaterSurfaceWakeSample, WaterWakePresentationPath,
};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery, TerrainWaterRegionView,
};

use crate::terrain::terrain_water_shoreline_geometry::stitch_geometry::{
    construct_terrain_water_geometric_wake_tile, construct_terrain_water_stitch_tile,
};

#[derive(Component)]
pub(in crate::terrain) struct WaterWakeEmission {
    source_position: Vec2,
    proximity: Timer,
}

#[derive(Component)]
pub(in crate::terrain) struct WaterWakeMeshParticle;

pub(crate) fn update_water_wake_wave_uniforms(
    regions: TerrainWaterRegionQuery,
    terrain_assets: Res<Assets<TerrainAsset>>,
    waves: Query<&TerrainWaterWaveRegisters>,
    groups: Query<&ChildOf, With<WaterWakeEmission>>,
    mut particles: Query<(&ChildOf, &mut ParticleEffectWaveSurface), With<WaterWakeMeshParticle>>,
) {
    for (parent, mut uniforms) in &mut particles {
        let Ok(region_parent) = groups.get(parent.parent()) else {
            continue;
        };
        let owner = region_parent.parent();
        let (Some(region), Ok(wave)) = (regions.get(owner), waves.get(owner)) else {
            continue;
        };
        let Some(terrain) = terrain_assets.get(region.terrain()) else {
            continue;
        };
        let Some(height) = region.surface_height(terrain) else {
            continue;
        };
        *uniforms = wave.particle_wave_surface(height);
    }
}

pub(crate) fn advance_water_wake_mesh_lifetimes(
    mut commands: Commands,
    time: Res<Time>,
    regions: TerrainWaterRegionQuery,
    mut emissions: Query<(Entity, &mut WaterWakeEmission, &ChildOf, Option<&Children>)>,
) {
    for (entity, mut emission, parent, children) in &mut emissions {
        if regions.get(parent.parent()).is_none() {
            commands.entity(entity).despawn();
            continue;
        }
        if emission.proximity.tick(time.delta()).is_finished()
            && children.is_none_or(|children| children.is_empty())
        {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(SystemParam)]
pub(in crate::terrain) struct WaterWakePresentation<'w, 's> {
    commands: Commands<'w, 's>,
    samples: MessageReader<'w, 's, WaterSurfaceWakeSample>,
    regions: TerrainWaterRegionQuery<'w, 's>,
    sources: Query<'w, 's, (&'static GlobalTransform, &'static WaterWakePresentationPath)>,
    memberships: Query<'w, 's, &'static TerrainWaterMembershipIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    effects: Res<'w, Assets<ParticleEffectDocumentAsset>>,
    settings: Res<'w, GraphicsSettings>,
    meshes: ResMut<'w, Assets<Mesh>>,
    random: ResMut<'w, NativeVisualRandomStream>,
    region_children: Query<'w, 's, &'static Children>,
    emissions: Query<'w, 's, (&'static WaterWakeEmission, Option<&'static Children>)>,
    particles: Query<
        'w,
        's,
        (
            &'static ParticleEffectPointEmitter,
            &'static ParticleAliveCount,
            &'static ParticleSpawnBatch,
        ),
        With<WaterWakeMeshParticle>,
    >,
    waves: Query<'w, 's, &'static TerrainWaterWaveRegisters>,
}

pub(crate) fn present_water_wake_samples(mut state: WaterWakePresentation) {
    let mut stitch = Vec::new();
    let mut geometric = Vec::new();
    for sample in state.samples.read() {
        let Ok((transform, path)) = state.sources.get(sample.surface) else {
            continue;
        };
        let routed = (*sample, transform.to_matrix());
        match path {
            WaterWakePresentationPath::TerrainStitch => stitch.push(routed),
            WaterWakePresentationPath::GeometricWave => geometric.push(routed),
        }
    }
    let mut admissions = EntityHashMap::default();
    present_routed_water_wake_samples::<false>(&mut state, stitch, &mut admissions);
    present_routed_water_wake_samples::<true>(&mut state, geometric, &mut admissions);
}

#[derive(Default)]
struct WaterWakeRegionAdmissions {
    particle_count: u32,
    nearby_origins: Vec<Vec2>,
}

fn present_routed_water_wake_samples<const HIGH: bool>(
    state: &mut WaterWakePresentation,
    samples: Vec<(WaterSurfaceWakeSample, Mat4)>,
    admissions: &mut EntityHashMap<WaterWakeRegionAdmissions>,
) {
    let WaterWakePresentation {
        commands,
        regions,
        memberships,
        terrain_assets,
        effects,
        settings,
        meshes,
        random,
        region_children,
        emissions,
        particles,
        waves,
        ..
    } = state;
    let level = match settings.water_detail {
        FourLevelGraphicsDetail::Low => 0,
        FourLevelGraphicsDetail::Medium => 1,
        FourLevelGraphicsDetail::High => 2,
        FourLevelGraphicsDetail::Ultra => 3,
    };
    let level = if HIGH { level } else { level.min(1) };
    for (sample, source_transform) in samples {
        let Some(source) = regions.get(sample.surface) else {
            continue;
        };
        let Some(terrain) = terrain_assets.get(source.terrain()) else {
            continue;
        };
        let Some(configuration) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
            .map(|presentation| &presentation.surface_material.configuration)
        else {
            continue;
        };
        let systems = if HIGH {
            &configuration.high_wake_systems
        } else {
            &configuration.wake_systems
        };
        let counts = if HIGH {
            &configuration.high_wake_maximum_counts
        } else {
            &configuration.wake_maximum_counts
        };
        let instance_count = if HIGH {
            configuration.high_wake_instance_count
        } else {
            2
        };
        let Some(system) = systems.get(level).or_else(|| {
            systems
                .first()
                .filter(|first| systems.iter().all(|system| system == *first))
        }) else {
            continue;
        };
        if system.is_empty() {
            continue;
        }
        let Some(&maximum_count) = counts.get(level) else {
            continue;
        };
        let maximum_count = maximum_count as u32;
        let admitted = admissions.entry(sample.surface).or_insert_with(|| {
            let mut count = WaterWakeRegionAdmissions::default();
            if let Ok(children) = region_children.get(sample.surface) {
                for (emission, children) in emissions.iter_many(children.iter()).flatten() {
                    if !emission.proximity.is_finished() {
                        count.nearby_origins.push(emission.source_position);
                    }
                    if let Some(children) = children {
                        for (particle, sample, batch) in
                            particles.iter_many(children.iter()).flatten()
                        {
                            count.particle_count = count.particle_count.saturating_add(
                                particle.sampled_live_count(sample, batch).unwrap_or(1),
                            );
                        }
                    }
                }
            }
            count
        });
        if admitted.particle_count >= maximum_count
            || admitted
                .nearby_origins
                .iter()
                .any(|position| position.distance_squared(sample.source_position) < 0.1)
        {
            continue;
        }
        let Some(effect_handle) = terrain.water_particle_effect(system) else {
            continue;
        };
        let Some(effect) = effects.get(effect_handle) else {
            continue;
        };
        let Some((emitter, _)) = effect.decal_emitter() else {
            continue;
        };
        let Some(membership) = memberships
            .iter()
            .find(|membership| membership.terrain() == source.terrain().id())
        else {
            continue;
        };
        let strength_fraction = if sample.native_strength < 0.2 {
            0.0
        } else if sample.native_strength > 5.0 {
            1.0
        } else {
            ((f64::from(sample.native_strength) - f64::from(0.2_f32)) / (5.0 - f64::from(0.2_f32)))
                as f32
        };
        let radius = 1.5 + (10.0 - 1.5) * strength_fraction;
        let direction = native_wake_unit_direction(sample.direction, random);
        let Some((positions, indices)) = construct_wake_stitch_geometry::<HIGH>(
            terrain,
            membership,
            sample.surface,
            &source,
            sample.source_position,
            radius,
        ) else {
            continue;
        };
        let Some(height) = source.surface_height(terrain) else {
            continue;
        };
        let wave_uniforms = if HIGH {
            let Ok(wave) = waves.get(sample.surface) else {
                continue;
            };
            Some(wave.particle_wave_surface(height))
        } else {
            None
        };
        let centre = if HIGH {
            Vec3::ZERO
        } else {
            Vec3::new(sample.source_position.x, height, -sample.source_position.y)
        };
        let group_transform =
            Transform::from_matrix(source_transform.inverse() * Mat4::from_translation(centre));
        let group = commands
            .spawn((
                WaterWakeEmission {
                    source_position: sample.source_position,
                    proximity: Timer::from_seconds(1.0, TimerMode::Once),
                },
                group_transform,
                Visibility::Inherited,
                ChildOf(sample.surface),
            ))
            .id();
        let jitter_radius = radius * 0.1;
        let texture_radius = radius - jitter_radius;
        for index in 0..instance_count {
            let jitter = if index == 0 {
                Vec2::ZERO
            } else {
                Vec2::new(
                    (random.unit_f64() * 2.0 - 1.0) as f32,
                    (random.unit_f64() * 2.0 - 1.0) as f32,
                ) * jitter_radius
                    / texture_radius
            };
            let mesh = construct_wake_particle_mesh(
                &positions,
                &indices,
                sample.source_position,
                centre,
                direction,
                texture_radius,
                jitter,
            );
            let mut particle = commands.spawn((
                WaterWakeMeshParticle,
                ParticleEffectPointEmitter::new(effect_handle.clone(), emitter)
                    .with_single_particle_mesh(meshes.add(mesh)),
                Transform::default(),
                Visibility::Inherited,
                ChildOf(group),
            ));
            if let Some(uniforms) = wave_uniforms {
                particle.insert((uniforms, bevy::camera::visibility::RenderLayers::layer(
                    openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER,
                )));
            }
        }
        admitted.particle_count = admitted.particle_count.saturating_add(instance_count);
        admitted.nearby_origins.push(sample.source_position);
    }
}

fn native_wake_unit_direction(mut direction: Vec2, random: &mut NativeVisualRandomStream) -> Vec2 {
    while direction.length() < 0.0001 {
        direction = Vec2::new(
            (random.unit_f64() * 2.0 - 1.0) as f32,
            (random.unit_f64() * 2.0 - 1.0) as f32,
        );
    }
    direction.normalize()
}

fn construct_wake_stitch_geometry<const HIGH: bool>(
    terrain: &TerrainAsset,
    membership: &TerrainWaterMembershipIndex,
    owner: Entity,
    source: &TerrainWaterRegionView<'_>,
    centre: Vec2,
    radius: f32,
) -> Option<(Vec<[f32; 3]>, Vec<u32>)> {
    let grid = terrain.canonical_terrain_grid();
    let tile_size = grid.units.cell_size_metres * 4.0;
    if grid.width < 5 || grid.height < 5 || tile_size <= 0.0 {
        return None;
    }
    let maximum_tile = IVec2::new(
        ((grid.width - 1) / 4) as i32 - 1,
        ((grid.height - 1) / 4) as i32 - 1,
    );
    let minimum = ((centre - Vec2::splat(radius)) / tile_size)
        .as_ivec2()
        .clamp(IVec2::ZERO, maximum_tile);
    let maximum = ((centre + Vec2::splat(radius)) / tile_size)
        .as_ivec2()
        .clamp(IVec2::ZERO, maximum_tile);
    if minimum.x >= maximum.x || minimum.y >= maximum.y {
        return None;
    }
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for y in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            let tile = UVec2::new(x as u32, y as u32);
            if !membership.side_owners(tile)?.contains(&Some(owner)) {
                continue;
            }
            let geometry = if HIGH {
                construct_terrain_water_geometric_wake_tile(
                    terrain, membership, owner, source, tile,
                )?
            } else {
                construct_terrain_water_stitch_tile(terrain, membership, owner, source, tile)?
            };
            let offset = u32::try_from(positions.len()).ok()?;
            positions.extend(geometry.positions);
            indices.extend(geometry.indices.into_iter().map(|index| index + offset));
        }
    }
    (!indices.is_empty()).then_some((positions, indices))
}

fn construct_wake_particle_mesh(
    source_positions: &[[f32; 3]],
    indices: &[u32],
    source_centre: Vec2,
    centre: Vec3,
    direction: Vec2,
    radius: f32,
    jitter: Vec2,
) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
    let positions = source_positions
        .iter()
        .map(|&[x, y, z]| (Vec3::new(x, z, -y) - centre).to_array())
        .collect::<Vec<_>>();
    let coordinates = source_positions
        .iter()
        .map(|&[x, y, _]| {
            let offset = (Vec2::new(x, y) - source_centre) / radius;
            (Vec2::new(
                offset.dot(Vec2::new(-direction.y, direction.x)),
                offset.dot(direction),
            ) + jitter)
                .to_array()
        })
        .collect::<Vec<_>>();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![Vec3::Y.to_array(); source_positions.len()],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, coordinates);
    mesh.insert_indices(Indices::U32(indices.to_vec()));
    mesh
}
