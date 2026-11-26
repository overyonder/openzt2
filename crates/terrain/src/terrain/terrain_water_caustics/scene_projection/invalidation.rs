use super::*;
use bevy::asset::UntypedAssetId;
use bevy::ecs::system::SystemParam;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterRegionEdit, TerrainWaterRegionRetired, TerrainWaterRegionSource,
};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum BoundInput {
    Node(Entity),
    Pose(Entity),
}

impl BoundInput {
    pub(super) fn entity(self) -> Entity {
        match self {
            Self::Node(entity) | Self::Pose(entity) => entity,
        }
    }
}

#[derive(Component)]
#[relationship(relationship_target = SceneBoundInputs)]
pub(in crate::terrain) struct SceneBoundRoot(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = SceneBoundRoot, linked_spawn)]
pub(in crate::terrain) struct SceneBoundInputs(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = SceneBoundUsers)]
pub(in crate::terrain) struct SceneBoundSource(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = SceneBoundSource)]
pub(in crate::terrain) struct SceneBoundUsers(Vec<Entity>);

#[derive(Component)]
pub(super) struct SceneBoundDependency(pub(super) BoundInput);

#[derive(Component)]
#[relationship(relationship_target = SceneWaterBoundUsers)]
pub(in crate::terrain) struct SceneWaterBoundSource(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = SceneWaterBoundSource)]
pub(in crate::terrain) struct SceneWaterBoundUsers(Vec<Entity>);

#[derive(Component)]
pub(super) struct SceneBoundInvalidated;

#[derive(SystemParam)]
pub(in crate::terrain) struct BoundDependencies<'w, 's> {
    pub(super) roots: Query<'w, 's, &'static SceneBoundInputs>,
    pub(super) sources: Query<'w, 's, &'static SceneBoundUsers>,
    pub(super) edges: Query<'w, 's, (&'static SceneBoundRoot, &'static SceneBoundDependency)>,
    pub(super) water: Query<'w, 's, &'static SceneWaterBoundUsers>,
    pub(super) water_source: Query<'w, 's, &'static SceneWaterBoundSource>,
    pub(super) invalidated: Query<'w, 's, Entity, With<SceneBoundInvalidated>>,
}

impl BoundDependencies<'_, '_> {
    pub(super) fn mark_users(&self, input: BoundInput, dirty: &mut HashSet<Entity>) {
        if let Ok(edges) = self.sources.get(input.entity()) {
            dirty.extend(
                self.edges
                    .iter_many(edges.iter())
                    .flatten()
                    .filter(|(_, kind)| kind.0 == input)
                    .map(|(root, _)| root.0),
            );
        }
    }

    pub(super) fn replace(
        &self,
        commands: &mut Commands,
        root: Entity,
        mut inputs: HashSet<BoundInput>,
    ) {
        if let Ok(old) = self.roots.get(root) {
            for edge in old.iter() {
                if self
                    .edges
                    .get(edge)
                    .is_ok_and(|(_, kind)| !inputs.remove(&kind.0))
                {
                    commands.entity(edge).try_despawn();
                }
            }
        }
        for input in inputs {
            commands.spawn((
                SceneBoundRoot(root),
                SceneBoundSource(input.entity()),
                SceneBoundDependency(input),
            ));
        }
    }
}

pub(in crate::terrain) fn invalidate_bounds_after_dependency_discard(
    discarded: On<Discard<SceneBoundUsers>>,
    users: Query<&SceneBoundUsers>,
    edges: Query<&SceneBoundRoot>,
    mut commands: Commands,
) {
    if let Ok(links) = users.get(discarded.entity) {
        for owner in edges.iter_many(links.iter()).flatten() {
            commands.entity(owner.0).try_insert(SceneBoundInvalidated);
        }
    }
}

#[derive(SystemParam)]
pub(in crate::terrain) struct BoundChanges<'w, 's> {
    pub(super) policies: Query<'w, 's, Entity, Changed<PrefabCausticPolicy>>,
    pub(super) removed_policies: RemovedComponents<'w, 's, PrefabCausticPolicy>,
    nodes: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<Children>,
            Changed<PrefabLocalRenderBound>,
            Changed<PrefabSkinRenderBound>,
            Changed<SkinnedMesh>,
            Changed<PrefabModelLevelOfDetailVisibilityRange>,
            Changed<Name>,
            Changed<PrefabCausticPolicy>,
        )>,
    >,
    lod_visibility: Query<
        'w,
        's,
        Entity,
        (
            With<PrefabModelLevelOfDetailVisibilityRange>,
            Changed<Visibility>,
        ),
    >,
    poses: Query<'w, 's, Entity, Or<(Changed<Transform>, Changed<ChildOf>)>>,
    gone_children: RemovedComponents<'w, 's, Children>,
    gone_bound: RemovedComponents<'w, 's, PrefabLocalRenderBound>,
    gone_skin_bound: RemovedComponents<'w, 's, PrefabSkinRenderBound>,
    gone_skin: RemovedComponents<'w, 's, SkinnedMesh>,
    gone_lod: RemovedComponents<'w, 's, PrefabModelLevelOfDetailVisibilityRange>,
    gone_visibility: RemovedComponents<'w, 's, Visibility>,
    gone_name: RemovedComponents<'w, 's, Name>,
    gone_transform: RemovedComponents<'w, 's, Transform>,
    gone_parent: RemovedComponents<'w, 's, ChildOf>,
    pub(super) gone_index: RemovedComponents<'w, 's, TerrainWaterMembershipIndex>,
}

impl BoundChanges<'_, '_> {
    pub(super) fn collect(
        &mut self,
        dependencies: &BoundDependencies,
        dirty: &mut HashSet<Entity>,
    ) {
        for entity in self
            .nodes
            .iter()
            .chain(&self.lod_visibility)
            .chain(self.gone_children.read())
            .chain(self.gone_bound.read())
            .chain(self.gone_skin_bound.read())
            .chain(self.gone_skin.read())
            .chain(self.gone_lod.read())
            .chain(self.gone_visibility.read())
            .chain(self.gone_name.read())
        {
            dependencies.mark_users(BoundInput::Node(entity), dirty);
        }
        for entity in self
            .poses
            .iter()
            .chain(self.gone_transform.read())
            .chain(self.gone_parent.read())
        {
            dependencies.mark_users(BoundInput::Pose(entity), dirty);
        }
    }
}

#[derive(SystemParam)]
pub(in crate::terrain) struct WaterChanges<'w, 's> {
    regions: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<TerrainWaterRegionEdit>,
            Changed<TerrainWaterRegionSource>,
            Added<TerrainWaterRegionRetired>,
        )>,
    >,
    edits: RemovedComponents<'w, 's, TerrainWaterRegionEdit>,
    sources: RemovedComponents<'w, 's, TerrainWaterRegionSource>,
    retired: RemovedComponents<'w, 's, TerrainWaterRegionRetired>,
    pub(super) terrain: MessageReader<'w, 's, AssetEvent<TerrainAsset>>,
}
impl WaterChanges<'_, '_> {
    pub(super) fn regions(&mut self) -> impl Iterator<Item = Entity> + '_ {
        self.regions
            .iter()
            .chain(self.edits.read())
            .chain(self.sources.read())
            .chain(self.retired.read())
    }
}
pub(super) fn changed_asset<A: Asset>(event: &AssetEvent<A>) -> Option<AssetId<A>> {
    match *event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::Removed { id }
        | AssetEvent::LoadedWithDependencies { id } => Some(id),
        AssetEvent::Unused { .. } => None,
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum PassAssetInput {
    Asset(UntypedAssetId),
    SourceEffect(AssetId<EffectPassMaterial>),
}

#[derive(Component, PartialEq)]
pub(in crate::terrain) struct SceneCausticMaterialDependencies(pub(super) HashSet<PassAssetInput>);

#[derive(Component)]
#[relationship(relationship_target = SceneCausticPrimitives)]
pub(in crate::terrain) struct SceneCausticPhysicalRoot(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = SceneCausticPhysicalRoot)]
pub(in crate::terrain) struct SceneCausticPrimitives(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = SceneCausticWaterDraws)]
pub(in crate::terrain) struct SceneCausticWaterOwner(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = SceneCausticWaterOwner, linked_spawn)]
pub(in crate::terrain) struct SceneCausticWaterDraws(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = SceneCausticViewDraws)]
pub(in crate::terrain) struct SceneCausticRenderView(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = SceneCausticRenderView, linked_spawn)]
pub(in crate::terrain) struct SceneCausticViewDraws(Vec<Entity>);

#[derive(SystemParam)]
pub(in crate::terrain) struct PassChanges<'w, 's> {
    pub(super) water: WaterChanges<'w, 's>,
    pub(super) materials: MessageReader<'w, 's, AssetEvent<MaterialAsset>>,
    pub(super) effects: MessageReader<'w, 's, AssetEvent<EffectPassMaterial>>,
    pub(super) targets: Query<'w, 's, Entity, Changed<TerrainWaterCausticSceneTarget>>,
    pub(super) removed_targets: RemovedComponents<'w, 's, TerrainWaterCausticSceneTarget>,
    pub(super) topology: Query<'w, 's, Entity, Or<(Changed<ChildOf>, Added<PrefabCausticPolicy>)>>,
    pub(super) removed_parents: RemovedComponents<'w, 's, ChildOf>,
    pub(super) removed_policies: RemovedComponents<'w, 's, PrefabCausticPolicy>,
    pub(super) children: Query<'w, 's, &'static Children>,
    pub(super) changed_sources: Query<
        'w,
        's,
        Entity,
        (
            With<PrefabPrimitiveMaterial>,
            Or<(
                Changed<Mesh3d>,
                Changed<PrefabPrimitiveMaterial>,
                Changed<MeshMaterial3d<EffectPassMaterial>>,
            )>,
        ),
    >,
    pub(super) removed_mesh: RemovedComponents<'w, 's, Mesh3d>,
    pub(super) removed_material: RemovedComponents<'w, 's, PrefabPrimitiveMaterial>,
    pub(super) removed_handle: RemovedComponents<'w, 's, MeshMaterial3d<EffectPassMaterial>>,
    pub(super) removed_views: RemovedComponents<'w, 's, EffectPassMaterialRenderView>,
    pub(super) removed_cameras: RemovedComponents<'w, 's, Camera3d>,
    pub(super) removed_zoo_cameras: RemovedComponents<'w, 's, ZooCamera>,
    pub(super) changed_views: Query<
        'w,
        's,
        (),
        (
            With<Camera3d>,
            Or<(
                Added<Camera3d>,
                Added<ZooCamera>,
                Changed<EffectPassMaterialRenderView>,
            )>,
        ),
    >,
}
