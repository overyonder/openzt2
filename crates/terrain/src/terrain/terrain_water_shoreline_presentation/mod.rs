use std::time::Duration;

use crate::terrain::terrain_water_shoreline_geometry::projection::PreparedTerrainShorelineMeshGroups;
use crate::terrain::terrain_water_shoreline_geometry::projection::TerrainShorelineRippleBoundary;
use crate::terrain::terrain_water_shoreline_geometry::ConstructedTerrainShorelineMeshGroup;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::graphics_settings_types::GraphicsEffectQuality;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::TerrainShorelineRippleTiming;
use openzt2_game_data::terrain::TerrainWaterShoreConfiguration;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTransparentRenderStage;
use openzt2_world_state::{
    simulation_time::native_visual_random_stream::NativeVisualRandomStream,
    world_spawn::prefab_presentation_types::PrefabPresentation,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::TerrainWaterWaveRegisters;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionSource;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_shoreline_presentation_types::TerrainWaterShorelinePresentation;
use openzt2_zoo_state::terrain::terrain_water_shoreline_presentation_types::{
    TerrainShorelineDraws, TerrainShorelineRegion, TerrainWaterShorelineSurface,
};
use openzt2_zoo_state::terrain::terrain_water_underwater_particle_types::TerrainWaterRegionLight;

#[derive(Component)]
pub(super) struct TerrainShorelineRippleEmission {
    scene: Handle<ScenePrefabAsset>,
    timing: TerrainShorelineRippleTiming,
    interval: Timer,
    spawned_this_period: usize,
    lifetime: Duration,
}

#[derive(Component)]
pub(super) struct TerrainShorelineRippleInstance {
    lifetime: Timer,
}

pub(super) fn expire_terrain_shoreline_ripples(
    time: Res<Time>,
    mut instances: Query<(Entity, &mut TerrainShorelineRippleInstance)>,
    mut commands: Commands,
) {
    for (entity, mut instance) in &mut instances {
        if instance.lifetime.tick(time.delta()).is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(super) fn emit_terrain_shoreline_ripples(
    mut commands: Commands,
    time: Res<Time>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    scene_assets: Res<Assets<ScenePrefabAsset>>,
    graphics: Res<GraphicsSettings>,
    regions: TerrainWaterRegionQuery,
    mut random: ResMut<NativeVisualRandomStream>,
    mut owners: Query<
        (
            Entity,
            Option<Ref<TerrainShorelineRippleBoundary>>,
            Option<&TerrainWaterRegionSource>,
            Option<&Children>,
            &GlobalTransform,
            Option<&WorldMember>,
            Option<&mut TerrainShorelineRippleEmission>,
        ),
        Or<(
            With<TerrainShorelineRippleBoundary>,
            With<TerrainShorelineRippleEmission>,
        )>,
    >,
    instances: Query<&TerrainShorelineRippleInstance>,
    parents: Query<&ChildOf>,
    members: Query<&WorldMember>,
) {
    for (owner, boundary, source, children, global, member, emission) in &mut owners {
        let configuration = boundary
            .as_ref()
            .filter(|boundary| !boundary.0.is_empty())
            .and_then(|_| {
                let region = regions.get(owner)?;
                let terrain = terrain_assets.get(region.terrain())?;
                let biome = region.biome(terrain)?;
                let shoreline = &biome.water_presentation.as_ref()?.shoreline;
                let timing = if region.edited_state().is_none() {
                    terrain
                        .shoreline_ripple_for_region(source?.source_region_index())?
                        .1
                } else {
                    shoreline.ripple_timing()
                };
                let (scene, lifetime) = terrain.shoreline_ripple_instance(
                    shoreline.ripple_system.as_deref()?,
                    &biome.name,
                    &graphics,
                )?;
                let period = Duration::try_from_secs_f32(timing.period_seconds).ok()?;
                (!period.is_zero() && !lifetime.is_zero())
                    .then_some((scene, timing, period, lifetime))
            });
        let children = children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| Some((child, instances.get(child).ok()?)));
        let Some((scene, timing, period, lifetime)) = configuration else {
            if emission.is_some() {
                for (child, _) in children {
                    commands.entity(child).try_despawn();
                }
                commands
                    .entity(owner)
                    .remove::<TerrainShorelineRippleEmission>();
            }
            continue;
        };
        let Some(boundary) = boundary else { continue };
        let reset = boundary.is_changed()
            || emission.as_ref().is_none_or(|emission| {
                emission.scene != *scene
                    || emission.timing != timing
                    || emission.lifetime != lifetime
            });
        if reset {
            for (child, _) in children {
                commands.entity(child).try_despawn();
            }
            let next = TerrainShorelineRippleEmission {
                scene: scene.clone(),
                timing,
                interval: Timer::new(period, TimerMode::Once),
                spawned_this_period: 0,
                lifetime,
            };
            if let Some(mut emission) = emission {
                *emission = next;
            } else {
                commands.entity(owner).insert(next);
            }
            continue;
        }
        let Some(mut emission) = emission else {
            continue;
        };
        if !scene_assets.contains(scene) {
            continue;
        }
        if time.delta() > emission.interval.remaining() {
            emission.interval.reset();
            emission.spawned_this_period = 0;
            continue;
        }
        emission.interval.tick(time.delta());
        let count = boundary.0.len();
        let maximum = ((count as f64 * f64::from(timing.maximum_coverage)) as usize).max(1);
        let per_period =
            ((count as f64 * f64::from(timing.maximum_coverage_per_period)) as usize).max(1);
        let allowance = (emission.interval.fraction() * per_period as f32) as usize;
        let available = allowance.saturating_sub(emission.spawned_this_period);
        if available == 0 || children.count() == maximum {
            continue;
        }
        let emit = usize::from(random.next_u15()) % available;
        emission.spawned_this_period += emit;
        let world_to_parent = global.affine().inverse();
        let member = member.or_else(|| {
            parents
                .iter_ancestors::<ChildOf>(owner)
                .find_map(|entity| members.get(entity).ok())
        });
        for _ in 0..emit {
            let slot = usize::from(random.next_u15()) % count;
            let mut ripple = commands.spawn((
                TerrainShorelineRippleInstance {
                    lifetime: Timer::new(lifetime, TimerMode::Once),
                },
                PrefabPresentation::new(scene.clone()),
                Transform::from_matrix(Mat4::from(
                    world_to_parent * boundary.0[slot].compute_affine(),
                )),
                Visibility::Inherited,
                ChildOf(owner),
            ));
            if let Some(member) = member {
                ripple.insert(*member);
            }
        }
    }
}

#[derive(Component)]
pub(super) struct TerrainShorelineMeshSlot(usize);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn submit_prepared_terrain_shoreline_mesh_groups(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    graphics: Res<GraphicsSettings>,
    materials: Res<Assets<MaterialAsset>>,
    mut effect_passes: ResMut<Assets<EffectPassMaterial>>,
    mut uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    owners: Query<&TerrainShorelineDraws>,
    mut existing: Query<(
        &ChildOf,
        &TerrainShorelineMeshSlot,
        &mut TerrainWaterShorelineSurface,
        &mut Mesh3d,
        &mut MeshMaterial3d<EffectPassMaterial>,
    )>,
    mut prepared: Query<(
        Entity,
        &TerrainWaterShorelinePresentation,
        &TerrainWaterWaveRegisters,
        &TerrainWaterRegionLight,
        &mut PreparedTerrainShorelineMeshGroups,
    )>,
) {
    for (owner, presentation, waves, light, mut prepared) in &mut prepared {
        let Some(source) = regions.get(owner) else {
            continue;
        };
        let Some(terrain) = terrain_assets.get(source.terrain()) else {
            continue;
        };
        let Some(configuration) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        let shore = configuration.surface_material.configuration.shore.as_ref();
        let draws: bevy::platform::collections::HashMap<_, _> = owners
            .get(owner)
            .ok()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(|draw| {
                let (parent, slot, ..) = existing.get(draw).ok()?;
                Some(((parent.parent(), slot.0), draw))
            })
            .collect();
        for (key, &entity) in &draws {
            if !prepared.slots.contains(key) {
                commands.entity(entity).try_despawn();
            }
        }
        let mut retry = Vec::new();
        for (parent, slot, group) in std::mem::take(&mut prepared.groups) {
            if let Err(group) = project_authored_terrain_shoreline_mesh_group(
                &mut commands,
                parent,
                owner,
                slot,
                draws.get(&(parent, slot)).copied(),
                &mut existing,
                group,
                terrain,
                graphics.effects,
                &materials,
                &mut effect_passes,
                &mut uniform_buffers,
                &mut meshes,
                presentation,
                waves,
                Some(light.raw_base_rgba().truncate()),
                shore,
            ) {
                retry.push((parent, slot, group));
            }
        }
        if retry.is_empty() {
            commands
                .entity(owner)
                .remove::<PreparedTerrainShorelineMeshGroups>();
        } else {
            prepared.groups = retry;
        }
    }
}

pub(super) fn initialize_terrain_shoreline_material_animation(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    memberships: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    initialized: Query<(), With<TerrainWaterShorelinePresentation>>,
) {
    for (entity, source) in regions.iter() {
        if initialized.contains(entity) {
            continue;
        }
        let resolved = memberships.iter().any(|membership| {
            membership.terrain() == source.terrain().id()
                && membership.accepted_points(entity).next().is_some()
        });
        if !resolved {
            continue;
        }
        let Some(terrain) = terrain_assets.get(source.terrain()) else {
            continue;
        };
        let Some(configuration) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        commands
            .entity(entity)
            .insert(TerrainWaterShorelinePresentation::from_configuration(
                configuration.surface_material.configuration.shore.as_ref(),
            ));
    }
}

/// Returns the unsubmitted geometry while any material dependency is unavailable.
#[allow(clippy::too_many_arguments)]
fn project_authored_terrain_shoreline_mesh_group(
    commands: &mut Commands,
    parent: Entity,
    material_owner: Entity,
    slot: usize,
    previous: Option<Entity>,
    existing: &mut Query<(
        &ChildOf,
        &TerrainShorelineMeshSlot,
        &mut TerrainWaterShorelineSurface,
        &mut Mesh3d,
        &mut MeshMaterial3d<EffectPassMaterial>,
    )>,
    group: ConstructedTerrainShorelineMeshGroup,
    terrain: &TerrainAsset,
    quality: GraphicsEffectQuality,
    materials: &Assets<MaterialAsset>,
    effect_passes: &mut Assets<EffectPassMaterial>,
    uniform_buffers: &mut Assets<ShaderBuffer>,
    meshes: &mut Assets<Mesh>,
    presentation: &TerrainWaterShorelinePresentation,
    wave_state: &TerrainWaterWaveRegisters,
    active_environment_colour: Option<Vec3>,
    shore: Option<&TerrainWaterShoreConfiguration>,
) -> Result<Entity, ConstructedTerrainShorelineMeshGroup> {
    let Some(material) = terrain
        .water_material("Materials/watershore.bfmat")
        .and_then(|handle| materials.get(handle))
    else {
        return Err(group);
    };
    let templates = material.evaluated_pass_material_assets_for_effect_quality(quality);
    let [template] = templates else {
        return Err(group);
    };
    let Some(mut pass) = effect_passes.get(template).cloned() else {
        return Err(group);
    };
    if !bind_shoreline_material_textures(&mut pass, terrain, shore) {
        return Err(group);
    }
    bind_shoreline_material_values(&mut pass, shore, presentation, active_environment_colour);
    wave_state.bind_wave_channels_to_effect_pass(group.surface_plane_height, &mut pass);
    pass.retain_authored_draw_order();
    pass.recreate_uniform_buffers_for_runtime_material(uniform_buffers);
    let next = TerrainWaterShorelineSurface {
        biome_index: group.biome_index,
        surface_plane_height: group.surface_plane_height,
    };
    let mesh = meshes.add(group.mesh);
    let material = effect_passes.add(pass);
    if let Some(previous) = previous {
        if let Ok((_, _, mut surface, mut geometry, mut handle)) = existing.get_mut(previous) {
            surface.set_if_neq(next);
            geometry.0 = mesh;
            handle.0 = material;
            return Ok(previous);
        }
    }
    let entity = commands
        .spawn((
            next,
            TerrainShorelineRegion(material_owner),
            TerrainShorelineMeshSlot(slot),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            NoFrustumCulling,
            Transform::IDENTITY,
            Visibility::Inherited,
            RenderLayers::layer(TERRAIN_WATER_RENDER_LAYER),
            ChildOf(parent),
        ))
        .id();
    commands.entity(entity).insert(
        AuthoredEffectTechniquePassSubmissionOrder::new(entity, 0)
            .expect("the shoreline's sole authored pass index fits u16")
            .with_render_stage(AuthoredEffectTransparentRenderStage::WaterSurface),
    );
    Ok(entity)
}

pub(super) fn advance_terrain_shoreline_material_animation(
    time: Res<Time>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    mut materials: Query<(Entity, &mut TerrainWaterShorelinePresentation)>,
) {
    for (owner, mut presentation) in &mut materials {
        let Some(source) = regions.get(owner) else {
            continue;
        };
        if let Some(configuration) = terrain_assets
            .get(source.terrain())
            .and_then(|terrain| source.biome(terrain))
            .and_then(|biome| biome.water_presentation.as_ref())
        {
            presentation
                .apply_configuration(configuration.surface_material.configuration.shore.as_ref());
        }
        presentation.advance(time.delta_secs());
    }
}

pub(super) fn update_terrain_shoreline_material_animation_bindings(
    terrain_assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    materials: Query<(
        &TerrainWaterShorelinePresentation,
        Ref<TerrainWaterWaveRegisters>,
        &TerrainWaterRegionLight,
    )>,
    mut surfaces: Query<(
        Ref<TerrainShorelineRegion>,
        Ref<TerrainWaterShorelineSurface>,
        Ref<MeshMaterial3d<EffectPassMaterial>>,
        &mut Visibility,
    )>,
    mut effect_passes: ResMut<Assets<EffectPassMaterial>>,
    mut uniform_buffers: ResMut<Assets<ShaderBuffer>>,
) {
    for (owner, surface, handle, mut visibility) in &mut surfaces {
        let Ok((presentation, waves, light)) = materials.get(owner.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(source) = regions.get(owner.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(terrain) = terrain_assets.get(source.terrain()) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(configuration) = source
            .biome(terrain)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(pass) = effect_passes.get_mut_untracked(handle.id()) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        bind_shoreline_material_values(
            pass,
            configuration.surface_material.configuration.shore.as_ref(),
            presentation,
            Some(light.raw_base_rgba().truncate()),
        );
        if waves.is_changed() || owner.is_changed() || surface.is_changed() || handle.is_changed() {
            waves.bind_wave_channels_to_effect_pass(surface.surface_plane_height, pass);
        }
        pass.write_programmable_float_registers_to_persistent_buffers(&mut uniform_buffers);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

pub(super) fn bind_shoreline_material_textures(
    material: &mut EffectPassMaterial,
    terrain: &TerrainAsset,
    configuration: Option<&TerrainWaterShoreConfiguration>,
) -> bool {
    let mut all_bound = true;
    for (semantic, source, fallback) in [
        (
            "RippleTop",
            configuration.and_then(|configuration| configuration.ripple_top.as_deref()),
            "Materials/waterlayout",
        ),
        (
            "RippleBot",
            configuration.and_then(|configuration| configuration.ripple_bottom.as_deref()),
            "Materials/waterlayout",
        ),
        (
            "RippleMask",
            configuration.and_then(|configuration| configuration.ripple_mask.as_deref()),
            "Materials/watermask",
        ),
    ] {
        let Some(texture) = source
            .and_then(|source| terrain.texture_image(source))
            .or_else(|| terrain.texture_image(fallback))
        else {
            all_bound = false;
            continue;
        };
        all_bound &= material.replace_texture_asset_for_effect_semantic(semantic, texture.clone());
    }
    all_bound
}

pub(super) fn bind_shoreline_material_values(
    material: &mut EffectPassMaterial,
    configuration: Option<&TerrainWaterShoreConfiguration>,
    presentation: &TerrainWaterShorelinePresentation,
    active_environment_colour: Option<Vec3>,
) {
    let mut tint = configuration
        .and_then(|configuration| configuration.tint_rgba255)
        .map_or(Vec4::new(1.0, 1.0, 1.0, 0.5), |colour| {
            Vec4::from_array(colour.map(|channel| f32::from(channel) / 255.0))
        });
    if let Some(colour) = active_environment_colour {
        tint.x *= colour.x;
        tint.y *= colour.y;
        tint.z *= colour.z;
    }
    material.bind_float_vector_effect_semantic("Tint", tint);
    material.bind_float_vector_effect_semantic("BotUVMap", presentation.bottom_uv_map());
    material.bind_float_vector_effect_semantic("TopUVMap", presentation.top_uv_map());
}
