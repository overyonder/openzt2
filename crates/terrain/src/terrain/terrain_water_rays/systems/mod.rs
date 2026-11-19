use bevy::{camera::visibility::RenderLayers, prelude::*, render::storage::ShaderBuffer};
use openzt2_game_assets::graphics_settings_types::{
    FourLevelGraphicsDetail, GraphicsEffectQuality, GraphicsSettings,
};
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::{
    AuthoredEffectTechniquePassSubmissionOrder, AuthoredEffectTransparentRenderStage,
};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};
use openzt2_zoo_state::terrain::terrain_water_ray_types::TerrainWaterRayAnimation;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState;

use super::{build_water_ray_disk_mesh, build_water_ray_strip_mesh};

#[derive(Component)]
pub(in crate::terrain) struct TerrainWaterRayEffect {
    pub(super) secondary_strip: bool,
    effects_quality: GraphicsEffectQuality,
}

#[derive(Component, Default)]
pub(in crate::terrain) struct TerrainWaterRayMeshPoses {
    pub(super) strip: Option<Mat4>,
    pub(super) disk: Option<Mat4>,
}

#[derive(Component)]
pub(in crate::terrain) struct TerrainWaterRayPass {
    pub(super) texture_layer: usize,
    pub(super) disk: bool,
}

/// Each source row owns its effect through Bevy's linked child lifetime.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(in crate::terrain) fn maintain_terrain_water_ray_effects(
    mut commands: Commands,
    graphics: Res<GraphicsSettings>,
    assets: Res<Assets<TerrainAsset>>,
    mut asset_changes: MessageReader<AssetEvent<TerrainAsset>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    indexes: Query<Ref<TerrainWaterMembershipIndex>>,
    sources: TerrainWaterRegionQuery,
    effects: Query<(Entity, &ChildOf, &TerrainWaterRayEffect)>,
    children: Query<&Children>,
) {
    let changed_assets: bevy::platform::collections::HashSet<_> = asset_changes
        .read()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    let enabled = graphics.water_detail != FourLevelGraphicsDetail::Low;
    let secondary_strip = matches!(
        graphics.water_detail,
        FourLevelGraphicsDetail::High | FourLevelGraphicsDetail::Ultra
    );
    let mut retiring = bevy::ecs::entity::EntityHashSet::default();
    for (effect, parent, state) in &effects {
        let valid = sources.get(parent.parent()).is_some_and(|source| {
            assets.get(source.terrain()).is_some()
                && !changed_assets.contains(&source.terrain().id())
                && indexes.iter().any(|index| {
                    index.terrain() == source.terrain().id()
                        && index.accepted_points(parent.parent()).next().is_some()
                })
        });
        if !enabled
            || !valid
            || state.effects_quality != graphics.effects
            || state.secondary_strip != secondary_strip
        {
            retiring.insert(effect);
            commands.entity(effect).despawn();
        }
    }
    if !enabled {
        return;
    }
    for (owner, source) in sources.iter() {
        if children.get(owner).is_ok_and(|children| {
            effects
                .iter_many(children.iter())
                .flatten()
                .any(|(effect, _, _)| !retiring.contains(&effect))
        }) {
            continue;
        }
        let Some(asset) = assets.get(source.terrain()) else {
            continue;
        };
        if !indexes.iter().any(|index| {
            index.terrain() == source.terrain().id()
                && index.accepted_points(owner).next().is_some()
        }) {
            continue;
        }
        if source
            .biome(asset)
            .and_then(|biome| biome.water_presentation.as_ref())
            .is_none()
        {
            continue;
        }
        let Some(ray_material) = asset
            .water_material("Materials/WaterRay.bfmat")
            .and_then(|handle| authored_materials.get(handle))
        else {
            continue;
        };
        let Some(disk_material) = asset
            .water_material("Materials/WaterDisk.bfmat")
            .and_then(|handle| authored_materials.get(handle))
        else {
            continue;
        };
        let mut prepared = Vec::new();
        let mut ready = true;
        for (disk, material, layers) in [
            (false, ray_material, if secondary_strip { 2 } else { 1 }),
            (true, disk_material, 1),
        ] {
            let templates =
                material.evaluated_pass_material_assets_for_effect_quality(graphics.effects);
            if templates.is_empty() {
                ready = false;
                break;
            }
            for layer in (0..layers).rev() {
                for template in templates {
                    let Some(pass) = materials.get(template).cloned() else {
                        ready = false;
                        break;
                    };
                    prepared.push((disk, layer, pass));
                }
            }
        }
        if !ready {
            continue;
        }
        let effect = commands
            .spawn((
                TerrainWaterRayEffect {
                    secondary_strip,
                    effects_quality: graphics.effects,
                },
                TerrainWaterRayAnimation::default(),
                TerrainWaterRayMeshPoses::default(),
                Transform::IDENTITY,
                // Environment producers must establish placement and contribution
                // before any pass is submitted.
                Visibility::Hidden,
                ChildOf(owner),
            ))
            .id();
        let strip = meshes.add(build_water_ray_strip_mesh());
        let disk_mesh = meshes.add(build_water_ray_disk_mesh());
        for (submission_index, (disk, texture_layer, mut pass)) in prepared.into_iter().enumerate()
        {
            pass.retain_authored_draw_order();
            pass.recreate_uniform_buffers_for_runtime_material(&mut buffers);
            commands.spawn((
                TerrainWaterRayPass {
                    texture_layer,
                    disk,
                },
                Mesh3d(if disk {
                    disk_mesh.clone()
                } else {
                    strip.clone()
                }),
                MeshMaterial3d(materials.add(pass)),
                Transform::IDENTITY,
                Visibility::Inherited,
                RenderLayers::layer(TERRAIN_WATER_RENDER_LAYER),
                AuthoredEffectTechniquePassSubmissionOrder::new(effect, submission_index)
                    .expect("water ray technique pass count fits u16")
                    .with_render_stage(AuthoredEffectTransparentRenderStage::WaterSurface),
                ChildOf(effect),
            ));
        }
    }
}

pub(in crate::terrain) fn advance_terrain_water_ray_materials(
    time: Res<Time>,
    view: Option<Res<TerrainWaterViewState>>,
    mut effects: Query<(
        &TerrainWaterRayEffect,
        &mut TerrainWaterRayAnimation,
        &mut Visibility,
    )>,
    passes: Query<(
        &ChildOf,
        &TerrainWaterRayPass,
        Ref<MeshMaterial3d<EffectPassMaterial>>,
    )>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    for (effect, mut animation, mut visibility) in &mut effects {
        if time.delta_secs() > 0.0 {
            animation.advance_texture_offsets(time.delta_secs(), effect.secondary_strip);
        }
        // Unknown camera membership cannot enable an effect. Environment
        // inputs are also required and have no fallback value.
        if view.as_ref().and_then(|view| view.underwater_status()) != Some(true) {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
    for (parent, pass, material) in &passes {
        if time.delta_secs() == 0.0 && !material.is_changed() {
            continue;
        }
        let Ok((_, animation, _)) = effects.get(parent.parent()) else {
            continue;
        };
        let Some(material) = materials.get_mut_untracked(&material.0) else {
            continue;
        };
        let transforms = if pass.disk {
            animation.disk_texture_transforms()
        } else {
            &animation.strip_texture_transforms()[pass.texture_layer]
        };
        if material
            .bind_fixed_function_matrix_effect_semantics(&[
                ("TextureTransform0", transforms[0]),
                ("TextureTransform1", transforms[1]),
            ])
            .is_ok()
        {
            material.write_changed_fixed_function_transforms_to_persistent_buffer(&mut buffers);
        }
    }
}
