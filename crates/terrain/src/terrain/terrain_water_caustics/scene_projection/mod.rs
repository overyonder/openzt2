mod invalidation;
use crate::terrain::water_regions::authored_membership::TerrainWaterMembershipChanged;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::{
    entity::{EntityHashMap, EntityHashSet},
    relationship::RelationshipTarget,
};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::transform::helper::TransformHelper;
pub(in crate::terrain) use invalidation::invalidate_bounds_after_dependency_discard;
use invalidation::*;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::material::material_asset_types::{MaterialAsset, MaterialModeTarget};
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
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
    parents: &'a Query<'w, 's, &'static ChildOf>,
    inputs: std::cell::RefCell<HashSet<BoundInput>>,
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
    fn watch_node(&self, entity: Entity) {
        self.inputs.borrow_mut().insert(BoundInput::Node(entity));
    }

    fn world_transform(&self, entity: Entity) -> Option<GlobalTransform> {
        self.inputs.borrow_mut().extend(
            std::iter::once(entity)
                .chain(self.parents.iter_ancestors::<ChildOf>(entity))
                .map(BoundInput::Pose),
        );
        self.transforms.compute_global_transform(entity).ok()
    }
    fn skin_bound(
        &self,
        entity: Entity,
        bound: &PrefabSkinRenderBound,
    ) -> Option<PrefabBoundingSphere> {
        let mut stack = vec![entity];
        let mut skin = None;
        while let Some(candidate) = stack.pop() {
            self.watch_node(candidate);
            if let Ok(found) = self.skins.get(candidate) {
                skin = Some(found);
                break;
            }
            if let Some(children) = self.nodes.get(candidate).ok()?.0 {
                stack.extend(children.iter());
            }
        }
        let skin = skin?;
        for &joint in &skin.joints {
            self.watch_node(joint);
        }
        let joint = |name: &str| {
            skin.joints.iter().copied().find(|joint| {
                self.names
                    .get(*joint)
                    .is_ok_and(|actual| actual.as_str() == name)
            })
        };
        let root = self
            .world_transform(joint(&bound.0.skeleton_root)?)?
            .to_matrix();
        let mut spheres = bound.0.bones.iter().map(|bone| {
            let matrix = self.world_transform(joint(&bone.joint)?)?.to_matrix();
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
    context.watch_node(entity);
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
                context.watch_node(child);
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
                context.inputs.borrow_mut().insert(BoundInput::Pose(child));
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

#[derive(Component, PartialEq)]
pub(in crate::terrain) struct SceneCausticWorldBound(Option<PrefabBoundingSphere>);

const CAUSTIC_SPATIAL_BIN_METRES: f32 = 16.0;

#[derive(Component, Default)]
pub(in crate::terrain) struct SceneCausticSpatialIndex {
    root_bins: EntityHashMap<IVec2>,
    by_bin: HashMap<IVec2, EntityHashSet>,
}

impl SceneCausticSpatialIndex {
    fn coordinates(point: Vec2) -> IVec2 {
        (point / CAUSTIC_SPATIAL_BIN_METRES).floor().as_ivec2()
    }

    fn remove_root(&mut self, root: Entity) {
        if let Some(previous) = self.root_bins.remove(&root) {
            if let bevy::platform::collections::hash_map::Entry::Occupied(mut bin) =
                self.by_bin.entry(previous)
            {
                bin.get_mut().remove(&root);
                if bin.get().is_empty() {
                    bin.remove();
                }
            }
        }
    }

    fn update_root(&mut self, root: Entity, point: Option<Vec2>) {
        let next = point.map(Self::coordinates);
        if next == self.root_bins.get(&root).copied() {
            return;
        }
        self.remove_root(root);
        if let Some(bin) = next {
            self.root_bins.insert(root, bin);
            self.by_bin.entry(bin).or_default().insert(root);
        }
    }
}

pub(in crate::terrain) fn unindex_discarded_caustic_world_bound(
    discarded: On<Discard<SceneCausticWorldBound>>,
    mut index: Query<&mut SceneCausticSpatialIndex>,
) {
    if let Ok(mut index) = index.single_mut() {
        index.remove_root(discarded.entity);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(in crate::terrain) fn project_scene_caustic_targets(
    mut commands: Commands,
    mut roots: Query<(
        Entity,
        &PrefabCausticPolicy,
        Option<&mut TerrainWaterCausticSceneTarget>,
    )>,
    nodes: BoundNodes,
    skins: Query<&'static SkinnedMesh>,
    names: Query<&'static Name>,
    skin_bounds: Query<&'static PrefabSkinRenderBound>,
    transforms: TransformHelper,
    indexes: Query<Ref<TerrainWaterMembershipIndex>>,
    regions: TerrainWaterRegionQuery,
    terrains: Res<Assets<TerrainAsset>>,
    parents: Query<&'static ChildOf>,
    mut changes: BoundChanges,
    (mut water, mut membership_changes, mut spatial, mut removed_sources): (
        WaterChanges,
        MessageReader<TerrainWaterMembershipChanged>,
        Query<&mut SceneCausticSpatialIndex>,
        RemovedComponents<SceneWaterBoundSource>,
    ),
    dependencies: BoundDependencies,
    targets: Query<Entity, With<TerrainWaterCausticSceneTarget>>,
    mut world_bounds: Query<(Entity, &mut SceneCausticWorldBound)>,
) {
    let mut spatial = spatial.single_mut().ok();
    let mut dirty = HashSet::new();
    changes.collect(&dependencies, &mut dirty);
    dirty.extend(changes.policies.iter());
    dirty.extend(
        roots
            .iter()
            .filter_map(|(entity, ..)| (!world_bounds.contains(entity)).then_some(entity)),
    );
    for entity in &dependencies.invalidated {
        dirty.insert(entity);
        commands
            .entity(entity)
            .try_remove::<SceneBoundInvalidated>();
    }
    for root in changes.removed_policies.read() {
        dependencies.mark_users(BoundInput::Node(root), &mut dirty);
        dirty.insert(root);
    }
    let mut membership = HashSet::new();
    let mut added_bounds = EntityHashMap::default();
    for root in dirty {
        let Ok((_, policy, _)) = roots.get(root) else {
            dependencies.replace(&mut commands, root, HashSet::new());
            if dependencies.water_source.contains(root) {
                commands.entity(root).try_remove::<SceneWaterBoundSource>();
            }
            if world_bounds.contains(root) {
                commands.entity(root).remove::<SceneCausticWorldBound>();
            }
            if targets.contains(root) {
                commands
                    .entity(root)
                    .remove::<TerrainWaterCausticSceneTarget>();
            }
            continue;
        };
        let context = SceneBounds {
            nodes: &nodes,
            skins: &skins,
            names: &names,
            bounds: &skin_bounds,
            transforms: &transforms,
            parents: &parents,
            inputs: default(),
        };
        let sphere = policy
            .receives
            .then(|| local_bound(root, root, &context))
            .flatten()
            .filter(|bound| bound.radius > 0.0)
            .and_then(|bound| {
                Some(transformed(
                    bound,
                    context.world_transform(root)?.to_matrix(),
                ))
            });
        if let Some(spatial) = spatial.as_deref_mut() {
            let point = sphere.map(|bound| Vec3::from_array(bound.center).xz());
            spatial.update_root(root, point);
        }
        dependencies.replace(&mut commands, root, context.inputs.into_inner());
        if let Ok((_, mut bound)) = world_bounds.get_mut(root) {
            bound.set_if_neq(SceneCausticWorldBound(sphere));
        } else {
            added_bounds.insert(root, sphere);
            commands.entity(root).insert(SceneCausticWorldBound(sphere));
        }
        membership.insert(root);
    }
    let mut rebuild = changes.gone_index.read().count() != 0;
    membership.extend(removed_sources.read());
    for region in water.regions() {
        if let Ok(users) = dependencies.water.get(region) {
            membership.extend(users.iter());
        }
    }
    rebuild |= water
        .terrain
        .read()
        .filter_map(changed_asset)
        .any(|id| indexes.iter().any(|index| index.terrain() == id));
    let mut published = HashSet::new();
    let mut affected_bins = HashSet::new();
    for change in membership_changes.read() {
        published.insert(change.terrain);
        if let Some(regions) = &change.regions {
            for owner in regions {
                if let Ok(users) = dependencies.water.get(*owner) {
                    membership.extend(users.iter());
                }
            }
        }
        let (Some(points), Some(asset)) = (&change.points, terrains.get(change.terrain)) else {
            rebuild = true;
            continue;
        };
        let spacing = asset.canonical_terrain_grid().units.cell_size_metres;
        let tile_span = spacing * 4.0;
        for &point in points {
            // Shared samples affect both bordering tiles. A point probe can
            // also select the opposing side of an adjacent tile.
            let tile = (point / 4).as_vec2();
            let source_min = (tile - Vec2::splat(2.0)) * tile_span;
            let source_max = (tile + Vec2::splat(2.0)) * tile_span;
            let min = SceneCausticSpatialIndex::coordinates(Vec2::new(source_min.x, -source_max.y));
            let max = SceneCausticSpatialIndex::coordinates(Vec2::new(source_max.x, -source_min.y));
            for y in min.y..=max.y {
                affected_bins.extend((min.x..=max.x).map(|x| IVec2::new(x, y)));
            }
        }
    }
    rebuild |= indexes
        .iter()
        .any(|index| index.is_changed() && !published.contains(&index.terrain()));
    if rebuild || (spatial.is_none() && !affected_bins.is_empty()) {
        membership.extend(world_bounds.iter().map(|(root, _)| root));
    } else if let Some(spatial) = spatial {
        for bin in affected_bins {
            if let Some(roots) = spatial.by_bin.get(&bin) {
                membership.extend(roots.iter().copied());
            }
        }
    }
    for root in membership {
        let Ok((_, _, old)) = roots.get_mut(root) else {
            continue;
        };
        let mut water_input = None;
        let target = (|| {
            let bound = added_bounds
                .get(&root)
                .copied()
                .or_else(|| world_bounds.get(root).ok().map(|(_, bound)| bound.0))??;
            let center = Vec3::from_array(bound.center);
            for index in &indexes {
                let asset = terrains.get(index.terrain())?;
                let owner = index.owner_at_world_xz(center.xz(), asset)??;
                water_input = Some(owner);
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
        let previous = dependencies
            .water_source
            .get(root)
            .ok()
            .map(|source| source.0);
        if water_input != previous {
            if let Some(owner) = water_input {
                commands.entity(root).insert(SceneWaterBoundSource(owner));
            } else {
                commands.entity(root).remove::<SceneWaterBoundSource>();
            }
        }
        match (old, target) {
            (Some(mut old), Some(next)) => {
                old.set_if_neq(next);
            }
            (None, Some(next)) => {
                commands.entity(root).insert(next);
            }
            (Some(_), None) => {
                commands
                    .entity(root)
                    .remove::<TerrainWaterCausticSceneTarget>();
            }
            (None, None) => {}
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
    targets: Query<(Entity, &TerrainWaterCausticSceneTarget)>,
    policies: Query<(), With<PrefabCausticPolicy>>,
    parents: Query<&ChildOf>,
    mut changes: PassChanges,
    (
        mut asset_dependencies,
        root_primitives,
        primitive_roots,
        draws,
        draws_by_water,
        bounds_by_water,
    ): (
        Query<(Entity, &mut SceneCausticMaterialDependencies)>,
        Query<&SceneCausticPrimitives>,
        Query<&SceneCausticPhysicalRoot>,
        Query<(), With<TerrainWaterCausticScenePass>>,
        Query<&SceneCausticWaterDraws>,
        Query<&SceneWaterBoundUsers>,
    ),
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
    let mut dirty = HashSet::new();
    let mut subtrees = HashSet::new();
    let mut views: Vec<_> = cameras
        .iter()
        .filter_map(|(entity, view, primary)| {
            let layer = view
                .map(|v| v.render_layer)
                .or_else(|| primary.then_some(PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER))?;
            Some((entity, layer))
        })
        .collect();
    views.sort_unstable();
    let views_changed = !changes.changed_views.is_empty()
        | (changes.removed_views.read().count() != 0)
        | (changes.removed_cameras.read().count() != 0)
        | (changes.removed_zoo_cameras.read().count() != 0);
    if views_changed || settings.is_changed() {
        subtrees.extend(targets.iter().map(|(root, _)| root));
    }
    for root in changes.targets.iter().chain(changes.removed_targets.read()) {
        if let Ok(primitives) = root_primitives.get(root) {
            dirty.extend(primitives.iter());
        }
        if targets.contains(root) {
            subtrees.insert(root);
        }
    }
    for node in changes
        .topology
        .iter()
        .chain(changes.removed_parents.read())
        .chain(changes.removed_policies.read())
    {
        if let Ok(primitives) = root_primitives.get(node) {
            dirty.extend(primitives.iter());
        }
        if primitives.contains(node) || changes.children.contains(node) {
            subtrees.insert(node);
        }
    }
    let mut stack: Vec<_> = subtrees.into_iter().collect();
    while let Some(node) = stack.pop() {
        if primitives.contains(node) {
            dirty.insert(node);
        }
        if let Ok(children) = changes.children.get(node) {
            stack.extend(children.iter().filter(|child| !policies.contains(*child)));
        }
    }
    dirty.extend(changes.changed_sources.iter());
    for source in changes
        .removed_mesh
        .read()
        .chain(changes.removed_material.read())
        .chain(changes.removed_handle.read())
    {
        if asset_dependencies.contains(source) || primitive_roots.contains(source) {
            dirty.insert(source);
        }
    }
    for owner in changes.water.regions() {
        if let Ok(draws) = draws_by_water.get(owner) {
            dirty.extend(
                parents
                    .iter_many(draws.iter())
                    .flatten()
                    .map(ChildOf::parent),
            );
        }
        if let Ok(bounds) = bounds_by_water.get(owner) {
            for root in bounds.iter().filter(|root| targets.contains(*root)) {
                if let Ok(primitives) = root_primitives.get(root) {
                    dirty.extend(primitives.iter());
                }
            }
        }
    }
    let mut assets: HashSet<_> = changes
        .water
        .terrain
        .read()
        .filter_map(changed_asset)
        .map(|id| PassAssetInput::Asset(id.untyped()))
        .chain(
            changes
                .materials
                .read()
                .filter_map(changed_asset)
                .map(|id| PassAssetInput::Asset(id.untyped())),
        )
        .collect();
    for event in changes.effects.read() {
        if let Some(id) = changed_asset(event) {
            assets.insert(PassAssetInput::Asset(id.untyped()));
            // A modified source transform is copied by the live binding update.
            if !matches!(event, AssetEvent::Modified { .. }) {
                assets.insert(PassAssetInput::SourceEffect(id));
            }
        }
    }
    if !assets.is_empty() {
        dirty.extend(
            asset_dependencies
                .iter()
                .filter_map(|(entity, dependencies)| {
                    (!dependencies.0.is_disjoint(&assets)).then_some(entity)
                }),
        );
    }
    for primitive in dirty {
        if let Ok(children) = changes.children.get(primitive) {
            for child in children.iter().filter(|&child| draws.contains(child)) {
                commands.entity(child).try_despawn();
            }
        }
        let previous_root = primitive_roots.get(primitive).ok().map(|root| root.0);
        let root = std::iter::once(primitive)
            .chain(parents.iter_ancestors::<ChildOf>(primitive))
            .find(|entity| policies.contains(*entity))
            .and_then(|root| targets.get(root).ok());
        let (Ok((_, mesh, source_material, source_pass)), Some((root, target))) =
            (primitives.get(primitive), root)
        else {
            if previous_root.is_some() {
                commands
                    .entity(primitive)
                    .try_remove::<SceneCausticPhysicalRoot>();
            }
            if asset_dependencies.contains(primitive) {
                commands
                    .entity(primitive)
                    .try_remove::<SceneCausticMaterialDependencies>();
            }
            continue;
        };
        if previous_root != Some(root) {
            commands
                .entity(primitive)
                .insert(SceneCausticPhysicalRoot(root));
        }
        let mut inputs = HashSet::new();
        // Unavailable dependencies remain indexed so that asset arrival retries the projection.
        (|| -> Option<()> {
            let source = regions.get(target.water_owner)?;
            inputs.insert(PassAssetInput::Asset(source.terrain().id().untyped()));
            let asset = terrains.get(source.terrain())?;
            let water = source.biome(asset)?.water_presentation.as_ref()?;
            inputs.insert(PassAssetInput::Asset(source_material.0.id().untyped()));
            let original = materials.get(&source_material.0)?;
            let mode = match original.mode("caustic") {
                Some(MaterialModeTarget::ThisMaterial) => original,
                other => {
                    let handle = match other {
                        Some(MaterialModeTarget::Material(handle)) => handle,
                        _ => asset.water_material("causticdefault")?,
                    };
                    inputs.insert(PassAssetInput::Asset(handle.id().untyped()));
                    materials.get(handle)?
                }
            };
            inputs.insert(PassAssetInput::SourceEffect(source_pass.0.id()));
            let source_effect = effects.get(&source_pass.0)?.clone();
            let templates =
                mode.evaluated_pass_material_assets_for_effect_quality(settings.effects);
            for handle in templates {
                inputs.insert(PassAssetInput::Asset(handle.id().untyped()));
            }
            for &(camera, layer) in &views {
                let mut prepared = Vec::new();
                for handle in templates {
                    let Some(mut pass) = effects.get(handle).cloned() else {
                        prepared.clear();
                        break;
                    };
                    pass.retain_authored_draw_order();
                    pass.recreate_uniform_buffers_for_runtime_material(&mut buffers);
                    if !super::bind_caustic_textures(
                        &mut pass,
                        asset,
                        &water.surface_material.configuration.caustics,
                    ) {
                        prepared.clear();
                        break;
                    }
                    if !copy_source_sampling(&mut pass, &source_effect).unwrap_or(false) {
                        inputs.insert(PassAssetInput::Asset(source_pass.0.id().untyped()));
                        prepared.clear();
                        break;
                    }
                    prepared.push(pass);
                }
                let mut invocation = None;
                for (index, pass) in prepared.into_iter().enumerate() {
                    let entity = commands
                        .spawn((
                            TerrainWaterCausticScenePass,
                            Mesh3d(mesh.0.clone()),
                            MeshMaterial3d(effects.add(pass)),
                            Transform::IDENTITY,
                            Visibility::Hidden,
                            RenderLayers::layer(layer),
                            ChildOf(primitive),
                            SceneCausticWaterOwner(target.water_owner),
                            SceneCausticRenderView(camera),
                        ))
                        .id();
                    let invocation = *invocation.get_or_insert(entity);
                    commands.entity(entity).insert(
                        AuthoredEffectTechniquePassSubmissionOrder::new(invocation, index)
                            .expect("the authored caustic pass index fits u16")
                            .with_render_stage(
                                AuthoredEffectTransparentRenderStage::TerrainCaustics,
                            ),
                    );
                }
            }
            Some(())
        })();
        let next = SceneCausticMaterialDependencies(inputs);
        if let Ok((_, mut current)) = asset_dependencies.get_mut(primitive) {
            current.set_if_neq(next);
        } else {
            commands.entity(primitive).insert(next);
        }
    }
}

pub(in crate::terrain) fn update_scene_caustic_bindings(
    sources: Query<&MeshMaterial3d<EffectPassMaterial>, Without<TerrainWaterCausticScenePass>>,
    targets: Query<&TerrainWaterCausticSceneTarget>,
    roots: Query<&SceneCausticPhysicalRoot>,
    regions: TerrainWaterRegionQuery,
    lighting: Query<(&TerrainWaterCausticAnimation, &TerrainWaterRegionLight)>,
    terrains: Res<Assets<TerrainAsset>>,
    cameras: Query<&Camera>,
    transforms: TransformHelper,
    mut passes: Query<
        (
            &ChildOf,
            &SceneCausticRenderView,
            &SceneCausticWaterOwner,
            &MeshMaterial3d<EffectPassMaterial>,
            &mut Visibility,
        ),
        With<TerrainWaterCausticScenePass>,
    >,
    mut effects: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    for (primitive, view, water, handle, mut visibility) in &mut passes {
        let bound = (|| -> std::io::Result<bool> {
            let Ok(root) = roots.get(primitive.parent()) else {
                return Ok(false);
            };
            if !targets
                .get(root.0)
                .is_ok_and(|target| target.water_owner == water.0)
            {
                return Ok(false);
            }
            let Some(source) = regions.get(water.0) else {
                return Ok(false);
            };
            let Ok((animation, light)) = lighting.get(water.0) else {
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
            let Ok(camera) = cameras.get(view.0) else {
                return Ok(false);
            };
            if !camera.is_active {
                return Ok(false);
            }
            let Ok(camera_transform) = transforms.compute_global_transform(view.0) else {
                return Ok(false);
            };
            let Some(source_effect) = sources
                .get(primitive.parent())
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
