use bevy::camera::visibility::RenderLayers;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::pbr::DistanceFog;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::{MaterialAsset, MaterialModeTarget};
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::material::runtime::resolved_effect_transform_bindings::EffectRenderViewTransforms;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::scene_prefab::PrefabBoundingSphere;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::{
    AuthoredEffectTechniquePassSubmissionOrder, AuthoredEffectTransparentRenderStage,
};
use openzt2_world_state::model_render::authored_model_material_pass_projection::{
    EffectPassMaterialRenderView, PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER,
};
use openzt2_world_state::world_spawn::prefab_model_level_of_detail_visibility_range::PrefabModelLevelOfDetailVisibilityRange;
use openzt2_world_state::world_spawn::prefab_presentation_types::{
    PrefabCausticPolicy, PrefabLocalRenderBound, PrefabPrimitiveMaterial, PrefabSkinRenderBound,
};
use openzt2_zoo_state::environment::environment_state_types::AuthoredWindShaderPresentationState;
use openzt2_zoo_state::terrain::terrain_water_caustic_types::TerrainWaterCausticAnimation;
use openzt2_zoo_state::terrain::terrain_water_caustic_types::{
    TerrainWaterCausticScenePass, TerrainWaterCausticSceneTarget,
};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};
use openzt2_zoo_state::terrain::terrain_water_underwater_particle_types::TerrainWaterRegionLight;

fn copy_source_sampling(
    pass: &mut EffectPassMaterial,
    source: &EffectPassMaterial,
) -> std::io::Result<bool> {
    if pass.has_vertex_float_effect_semantic("WorldToCaustic0") {
        pass.copy_reflected_texture_sampling_from(source, "Texture0", "TextureTransform0")
    } else {
        pass.copy_texture_stage_sampling_from(source, 0, 0)
    }
}

type BoundNodes<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Children>,
        Option<&'static Transform>,
        Option<&'static PrefabLocalRenderBound>,
        Has<SkinnedMesh>,
        Has<PrefabCausticPolicy>,
        Option<&'static PrefabModelLevelOfDetailVisibilityRange>,
        Option<&'static Visibility>,
    ),
>;

fn transformed(mut sphere: PrefabBoundingSphere, matrix: Mat4) -> PrefabBoundingSphere {
    sphere.center = matrix
        .transform_point3(Vec3::from_array(sphere.center))
        .to_array();
    // The compact-bound transform takes the largest source row's squared length.
    let scale = (0..3)
        .map(|row| matrix.row(row).truncate().length_squared())
        .fold(0.0_f32, f32::max)
        .sqrt();
    sphere.radius *= scale;
    sphere
}

fn merge(destination: &mut PrefabBoundingSphere, incoming: PrefabBoundingSphere) {
    if incoming.radius <= 0.0 {
        return;
    }
    if destination.radius <= 0.0 {
        *destination = incoming;
        return;
    }
    let delta = Vec3::from_array(incoming.center) - Vec3::from_array(destination.center);
    let distance = delta.length();
    if distance < f32::from_bits(0x3727c5ac) {
        destination.radius = destination.radius.max(incoming.radius);
    } else if incoming.radius > distance + destination.radius {
        *destination = incoming;
    } else if distance + incoming.radius >= destination.radius {
        let difference = incoming.radius - destination.radius;
        destination.center = ((Vec3::from_array(incoming.center)
            + Vec3::from_array(destination.center)
            + delta * distance.recip() * difference)
            * 0.5)
            .to_array();
        destination.radius = (destination.radius + incoming.radius + distance) * 0.5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unequal_spheres_enclose_the_two_extreme_points() {
        let mut sphere = PrefabBoundingSphere {
            center: [0.0; 3],
            radius: 1.0,
        };
        merge(
            &mut sphere,
            PrefabBoundingSphere {
                center: [10.0, 0.0, 0.0],
                radius: 3.0,
            },
        );
        assert_eq!(sphere.center, [6.0, 0.0, 0.0]);
        assert_eq!(sphere.radius, 7.0);
        merge(
            &mut sphere,
            PrefabBoundingSphere {
                center: [5.0, 0.0, 0.0],
                radius: 1.0,
            },
        );
        assert_eq!((sphere.center, sphere.radius), ([6.0, 0.0, 0.0], 7.0));
    }
}

struct SceneBounds<'a, 'w, 's> {
    nodes: &'a BoundNodes<'w, 's>,
    skins: &'a Query<'w, 's, &'static SkinnedMesh>,
    names: &'a Query<'w, 's, &'static Name>,
    bounds: &'a Query<'w, 's, &'static PrefabSkinRenderBound>,
    transforms: &'a TransformHelper<'w, 's>,
}

fn transform_skin_sphere(mut sphere: PrefabBoundingSphere, matrix: Mat4) -> PrefabBoundingSphere {
    sphere.center = matrix
        .transform_point3(Vec3::from_array(sphere.center))
        .to_array();
    sphere.radius *= matrix.determinant().cbrt();
    sphere
}

fn merge_skin_spheres(destination: &mut PrefabBoundingSphere, incoming: PrefabBoundingSphere) {
    let delta = Vec3::from_array(destination.center) - Vec3::from_array(incoming.center);
    let squared_distance = delta.length_squared();
    let difference = incoming.radius - destination.radius;
    if difference <= 0.0 && squared_distance <= difference * difference {
        return;
    }
    if difference > 0.0 && squared_distance < difference * difference {
        *destination = incoming;
        return;
    }
    let distance = squared_distance.sqrt();
    if distance > f32::from_bits(0x358637bd) {
        destination.center = (Vec3::from_array(incoming.center)
            + delta * ((distance - difference) / (2.0 * distance)))
            .to_array();
    }
    destination.radius = (distance + incoming.radius + destination.radius) * 0.5;
}

impl SceneBounds<'_, '_, '_> {
    fn skin_bound(
        &self,
        entity: Entity,
        bound: &PrefabSkinRenderBound,
    ) -> Option<PrefabBoundingSphere> {
        let mut stack = vec![entity];
        let mut skin = None;
        while let Some(candidate) = stack.pop() {
            if let Ok(found) = self.skins.get(candidate) {
                skin = Some(found);
                break;
            }
            if let Some(children) = self.nodes.get(candidate).ok()?.0 {
                stack.extend(children.iter());
            }
        }
        let skin = skin?;
        let joint = |name: &str| {
            skin.joints.iter().copied().find(|joint| {
                self.names
                    .get(*joint)
                    .is_ok_and(|actual| actual.as_str() == name)
            })
        };
        let root = self
            .transforms
            .compute_global_transform(joint(&bound.0.skeleton_root)?)
            .ok()?
            .to_matrix();
        let mut spheres = bound.0.bones.iter().map(|bone| {
            let matrix = self
                .transforms
                .compute_global_transform(joint(&bone.joint)?)
                .ok()?
                .to_matrix();
            Some(transform_skin_sphere(bone.sphere, matrix))
        });
        let mut sphere = spheres.next()??;
        for next in spheres {
            merge_skin_spheres(&mut sphere, next?);
        }
        Some(transform_skin_sphere(
            sphere,
            Mat4::from_cols_array(&bound.0.skin_transform) * root.inverse(),
        ))
    }
}

fn local_bound(
    entity: Entity,
    root: Entity,
    context: &SceneBounds,
) -> Option<PrefabBoundingSphere> {
    let nodes = context.nodes;
    let (children, _, own, skin, physical, _, _) = nodes.get(entity).ok()?;
    if entity != root && physical {
        return Some(PrefabBoundingSphere {
            center: [0.0; 3],
            radius: 0.0,
        });
    }
    if let Ok(bound) = context.bounds.get(entity) {
        return context.skin_bound(entity, bound);
    }
    // Authored geometry bounds own their renderable's complete bound; generated
    // render-pass children do not contribute additional geometry bounds.
    if let Some(own) = own {
        return Some(own.0);
    }
    if skin {
        return None;
    }
    if let Some(children) = children {
        let candidates = || {
            children.iter().filter_map(|child| {
                let (_, _, _, _, _, range, visibility) = nodes.get(child).ok()?;
                let range = range.filter(|range| range.group == entity)?;
                Some((child, range.ordinal, visibility))
            })
        };
        let selected = candidates()
            .filter(|(_, _, visibility)| *visibility != Some(&Visibility::Hidden))
            .min_by_key(|(_, ordinal, _)| *ordinal)
            .or_else(|| candidates().min_by_key(|(_, ordinal, _)| *ordinal));
        if let Some((child, _, _)) = selected {
            // LOD bounds copy the selected child's local sphere without its transform.
            return local_bound(child, root, context);
        }
    }
    let mut result = PrefabBoundingSphere {
        center: [0.0; 3],
        radius: 0.0,
    };
    if let Some(children) = children {
        for child in children.iter() {
            let child_bound = local_bound(child, root, context)?;
            let (_, transform, _, _, _, _, _) = nodes.get(child).ok()?;
            if child_bound.radius >= f32::from_bits(0x3727c5ac) {
                merge(
                    &mut result,
                    transformed(
                        child_bound,
                        transform.map_or(Mat4::IDENTITY, Transform::to_matrix),
                    ),
                );
            }
        }
    }
    if let Some(own) = own {
        merge(&mut result, own.0);
    }
    Some(result)
}

pub(in crate::terrain) fn project_scene_caustic_targets(
    mut commands: Commands,
    roots: Query<(
        Entity,
        &PrefabCausticPolicy,
        Option<&TerrainWaterCausticSceneTarget>,
    )>,
    nodes: BoundNodes,
    skins: Query<&'static SkinnedMesh>,
    names: Query<&'static Name>,
    skin_bounds: Query<&'static PrefabSkinRenderBound>,
    transforms: TransformHelper,
    indexes: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    terrains: Res<Assets<TerrainAsset>>,
) {
    for (root, policy, old) in &roots {
        let target = (|| {
            if !policy.receives {
                return None;
            }
            let bound = local_bound(
                root,
                root,
                &SceneBounds {
                    nodes: &nodes,
                    skins: &skins,
                    names: &names,
                    bounds: &skin_bounds,
                    transforms: &transforms,
                },
            )?;
            if bound.radius <= 0.0 {
                return None;
            }
            let bound = transformed(
                bound,
                transforms.compute_global_transform(root).ok()?.to_matrix(),
            );
            let center = Vec3::from_array(bound.center);
            for index in &indexes {
                let asset = terrains.get(index.terrain())?;
                let owner = index.owner_at_world_xz(center.xz(), asset)??;
                let source = regions.get(owner)?;
                if source.terrain().id() != index.terrain() {
                    continue;
                }
                if center.y - bound.radius <= source.surface_height(asset)? {
                    return Some(TerrainWaterCausticSceneTarget { water_owner: owner });
                }
            }
            None
        })();
        if target.as_ref() == old {
            continue;
        }
        if let Some(target) = target {
            commands.entity(root).insert(target);
        } else {
            commands
                .entity(root)
                .remove::<TerrainWaterCausticSceneTarget>();
        }
    }
}

pub(in crate::terrain) fn project_scene_caustic_passes(
    mut commands: Commands,
    primitives: Query<(
        Entity,
        Ref<Mesh3d>,
        Ref<PrefabPrimitiveMaterial>,
        &MeshMaterial3d<EffectPassMaterial>,
    )>,
    targets: Query<&TerrainWaterCausticSceneTarget>,
    policies: Query<(), With<PrefabCausticPolicy>>,
    parents: Query<&ChildOf>,
    existing: Query<(Entity, &TerrainWaterCausticScenePass)>,
    regions: TerrainWaterRegionQuery,
    terrains: Res<Assets<TerrainAsset>>,
    materials: Res<Assets<MaterialAsset>>,
    settings: Res<GraphicsSettings>,
    cameras: Query<
        (
            Entity,
            Option<&EffectPassMaterialRenderView>,
            Has<ZooCamera>,
        ),
        With<Camera3d>,
    >,
    mut effects: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let refresh = materials.is_changed() || settings.is_changed();
    let mut retained = HashSet::new();
    for (entity, pass) in &existing {
        if refresh
            || !primitives
                .get(pass.source_primitive)
                .is_ok_and(|(_, mesh, material, _)| !mesh.is_changed() && !material.is_changed())
            || !targets
                .get(pass.physical_presentation)
                .is_ok_and(|target| target.water_owner == pass.water_owner)
            || cameras.get(pass.render_view).is_err()
        {
            commands.entity(entity).despawn();
        } else {
            retained.insert((pass.source_primitive, pass.render_view, pass.water_owner));
        }
    }
    for (primitive, mesh, source_material, source_pass) in &primitives {
        let Some(root) = std::iter::once(primitive)
            .chain(parents.iter_ancestors::<ChildOf>(primitive))
            .find(|entity| policies.contains(*entity))
        else {
            continue;
        };
        let Ok(target) = targets.get(root) else {
            continue;
        };
        let Some((asset, source)) = regions
            .get(target.water_owner)
            .and_then(|source| Some((terrains.get(source.terrain())?, source)))
        else {
            continue;
        };
        let Some(water) = source
            .biome(asset)
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        let Some(original) = materials.get(&source_material.0) else {
            continue;
        };
        let mode = match original.mode("caustic") {
            Some(MaterialModeTarget::ThisMaterial) => Some(original),
            Some(MaterialModeTarget::Material(handle)) => materials.get(handle),
            None => asset
                .water_material("causticdefault")
                .and_then(|handle| materials.get(handle)),
        };
        let Some(mode) = mode else {
            continue;
        };
        let Some(source_effect) = effects.get(&source_pass.0).cloned() else {
            continue;
        };
        for (camera, view, primary) in &cameras {
            let layer = if let Some(view) = view {
                view.render_layer
            } else if primary {
                PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER
            } else {
                continue;
            };
            if retained.contains(&(primitive, camera, target.water_owner)) {
                continue;
            }
            let templates =
                mode.evaluated_pass_material_assets_for_effect_quality(settings.effects);
            let mut prepared = Vec::new();
            for handle in templates {
                let Some(mut pass) = effects.get(handle).cloned() else {
                    prepared.clear();
                    break;
                };
                pass.recreate_uniform_buffers_for_runtime_material(&mut buffers);
                if !super::bind_caustic_textures(
                    &mut pass,
                    asset,
                    &water.surface_material.configuration.caustics,
                ) || !copy_source_sampling(&mut pass, &source_effect).unwrap_or(false)
                {
                    prepared.clear();
                    break;
                }
                prepared.push(pass);
            }
            let mut invocation = None;
            for (index, pass) in prepared.into_iter().enumerate() {
                let entity = commands
                    .spawn((
                        TerrainWaterCausticScenePass {
                            physical_presentation: root,
                            source_primitive: primitive,
                            water_owner: target.water_owner,
                            render_view: camera,
                        },
                        Mesh3d(mesh.0.clone()),
                        MeshMaterial3d(effects.add(pass)),
                        Transform::IDENTITY,
                        Visibility::Hidden,
                        RenderLayers::layer(layer),
                        ChildOf(primitive),
                    ))
                    .id();
                let invocation = *invocation.get_or_insert(entity);
                commands.entity(entity).insert(
                    AuthoredEffectTechniquePassSubmissionOrder::new(invocation, index)
                        .expect("the authored caustic pass index fits u16")
                        .with_render_stage(AuthoredEffectTransparentRenderStage::TerrainCaustics),
                );
            }
        }
    }
}

pub(in crate::terrain) fn update_scene_caustic_bindings(
    sources: Query<&MeshMaterial3d<EffectPassMaterial>, Without<TerrainWaterCausticScenePass>>,
    targets: Query<&TerrainWaterCausticSceneTarget>,
    regions: TerrainWaterRegionQuery,
    lighting: Query<(&TerrainWaterCausticAnimation, &TerrainWaterRegionLight)>,
    terrains: Res<Assets<TerrainAsset>>,
    cameras: Query<(&Camera, &Projection, Option<&DistanceFog>, Has<ZooCamera>)>,
    wind: Query<&AuthoredWindShaderPresentationState>,
    transforms: TransformHelper,
    mut passes: Query<(
        &TerrainWaterCausticScenePass,
        &MeshMaterial3d<EffectPassMaterial>,
        &mut Visibility,
    )>,
    mut effects: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    for (pass, handle, mut visibility) in &mut passes {
        let bound = (|| -> std::io::Result<bool> {
            if !targets
                .get(pass.physical_presentation)
                .is_ok_and(|target| target.water_owner == pass.water_owner)
            {
                return Ok(false);
            }
            let Some(source) = regions.get(pass.water_owner) else {
                return Ok(false);
            };
            let Ok((animation, light)) = lighting.get(pass.water_owner) else {
                return Ok(false);
            };
            let Some(asset) = terrains.get(source.terrain()) else {
                return Ok(false);
            };
            let Some(water) = source
                .biome(asset)
                .and_then(|biome| biome.water_presentation.as_ref())
            else {
                return Ok(false);
            };
            let Some(height) = source.surface_height(asset) else {
                return Ok(false);
            };
            let Ok((camera, projection, fog, primary)) = cameras.get(pass.render_view) else {
                return Ok(false);
            };
            if !camera.is_active {
                return Ok(false);
            }
            let Ok(camera_transform) = transforms.compute_global_transform(pass.render_view) else {
                return Ok(false);
            };
            let Ok(model_transform) = transforms.compute_global_transform(pass.source_primitive)
            else {
                return Ok(false);
            };
            let Some(source_effect) = sources
                .get(pass.source_primitive)
                .ok()
                .and_then(|handle| effects.get(&handle.0))
                .cloned()
            else {
                return Ok(false);
            };
            let Some(mut effect) = effects.get_mut(&handle.0) else {
                return Ok(false);
            };
            if !copy_source_sampling(&mut effect, &source_effect)? {
                return Ok(false);
            }
            let view = EffectRenderViewTransforms::from_camera(&camera_transform, projection);
            effect.bind_per_draw_transform_semantics(
                model_transform.to_matrix(),
                &view,
                true,
                true,
            );
            effect.bind_fixed_function_transform_expressions(
                model_transform.to_matrix(),
                pass.render_view,
                &view,
                true,
                true,
                None,
                None,
                None,
            );
            let configuration = &water.surface_material.configuration.caustics;
            let tint = Vec4::from_array(configuration.tint_rgba) * light.raw_base_rgba();
            let packed = tint
                .to_array()
                .map(|channel| ((f64::from(channel) * 255.0 + 0.5) as i32 as u32).min(255));
            effect.set_fixed_function_ambient_colour_argb(
                packed[3] << 24 | packed[0] << 16 | packed[1] << 8 | packed[2],
            );
            let matrices =
                super::caustic_world_projection_matrices(animation, configuration, height);
            let ready = if effect.has_vertex_float_effect_semantic("WorldToCaustic0") {
                let Ok(wind) = wind.get(pass.water_owner) else {
                    return Ok(false);
                };
                let fog = fog.or_else(|| {
                    (!primary)
                        .then(|| {
                            cameras
                                .iter()
                                .find_map(|(_, _, fog, primary)| primary.then_some(fog).flatten())
                        })
                        .flatten()
                });
                let Some(mut fog) = fog.and_then(|fog| view.fog_shader_registers(fog, projection))
                else {
                    return Ok(false);
                };
                let source_from_world = super::source_from_bevy_world();
                fog[1] = (source_from_world * fog[1].truncate().extend(0.0))
                    .truncate()
                    .extend(fog[1].w);
                effect.bind_changed_environment_registers(
                    Some(&wind.shader_registers()),
                    None,
                    Some(&fog),
                );
                effect.bind_float_vector_effect_semantic("CausticTint", tint);
                effect.bind_float_vector_effect_semantic(
                    "HeightRamp",
                    Vec4::new(
                        height - configuration.fade_start,
                        -configuration.fade_distance.recip(),
                        0.0,
                        1.0,
                    ),
                );
                effect.bind_float_matrix_effect_semantic("WorldToCaustic0", matrices[0]);
                effect.bind_float_matrix_effect_semantic("WorldToCaustic1", matrices[1]);
                let model = source_from_world * model_transform.to_matrix();
                effect.bind_float_matrix_effect_semantic("ModelToWorld", model);
                effect.bind_float_matrix_effect_semantic("VecToWorld", model.inverse().transpose());
                let clip = projection.get_clip_from_view()
                    * camera_transform.to_matrix().inverse()
                    * source_from_world.transpose();
                effect.bind_float_matrix_effect_semantic("WorldToProj", clip);
                true
            } else {
                super::bind_caustic_camera_projection(
                    &mut effect,
                    matrices,
                    super::source_from_bevy_world() * camera_transform.to_matrix(),
                    &mut buffers,
                )?
            };
            effect.write_changed_fixed_function_registers_to_persistent_buffer(&mut buffers);
            effect.write_programmable_float_registers_to_persistent_buffers(&mut buffers);
            Ok(ready)
        })();
        match bound {
            Ok(true) => {
                visibility.set_if_neq(Visibility::Inherited);
            }
            Ok(false) => {
                visibility.set_if_neq(Visibility::Hidden);
            }
            Err(error) => {
                visibility.set_if_neq(Visibility::Hidden);
                error!(%error, "caustic source sampling or projection evaluation failed");
            }
        }
    }
}
