use super::*;
use bevy::asset::UntypedAssetId;
use bevy::ecs::system::SystemParam;
use bevy::platform::collections::HashMap;
use openzt2_game_assets::graphics_settings_types::GraphicsEffectQuality;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterRegionEdit, TerrainWaterRegionRetired, TerrainWaterRegionSource,
};

/// Records the inputs actually read by each projection, including missing assets.
pub(super) struct Dependencies<K> {
    inputs: HashMap<Entity, HashSet<K>>,
    users: HashMap<K, HashSet<Entity>>,
}

impl<K> Default for Dependencies<K> {
    fn default() -> Self {
        Self {
            inputs: default(),
            users: default(),
        }
    }
}

impl<K: Copy + Eq + std::hash::Hash> Dependencies<K> {
    pub(super) fn contains(&self, entity: Entity) -> bool {
        self.inputs.contains_key(&entity)
    }
    pub(super) fn remove(&mut self, entity: Entity) {
        if let Some(inputs) = self.inputs.remove(&entity) {
            for key in inputs {
                if let Some(users) = self.users.get_mut(&key) {
                    users.remove(&entity);
                    if users.is_empty() {
                        self.users.remove(&key);
                    }
                }
            }
        }
    }
    pub(super) fn record(&mut self, entity: Entity, inputs: HashSet<K>) {
        if self.inputs.get(&entity) == Some(&inputs) {
            return;
        }
        self.remove(entity);
        for &key in &inputs {
            self.users.entry(key).or_default().insert(entity);
        }
        self.inputs.insert(entity, inputs);
    }
    pub(super) fn affected(&self, key: K, dirty: &mut HashSet<Entity>) {
        if let Some(users) = self.users.get(&key) {
            dirty.extend(users.iter().copied());
        }
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum BoundInput {
    Node(Entity),
    Pose(Entity),
}

#[derive(Default)]
pub(in crate::terrain) struct BoundCache {
    pub(super) deps: Dependencies<BoundInput>,
    pub(super) regions: Dependencies<Entity>,
    pub(super) active: bool,
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
    pub(super) fn collect(&mut self, cache: &BoundCache, dirty: &mut HashSet<Entity>) {
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
            cache.deps.affected(BoundInput::Node(entity), dirty);
        }
        for entity in self
            .poses
            .iter()
            .chain(self.gone_transform.read())
            .chain(self.gone_parent.read())
        {
            cache.deps.affected(BoundInput::Pose(entity), dirty);
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
pub(super) enum PassInput {
    Asset(UntypedAssetId),
    SourceEffect(AssetId<EffectPassMaterial>),
    Region(Entity),
    Root(Entity),
    Ancestor(Entity),
}

#[derive(Default)]
pub(in crate::terrain) struct PassCache {
    pub(super) deps: Dependencies<PassInput>,
    pub(super) draws: HashMap<Entity, Vec<Entity>>,
    pub(super) sources: HashMap<Entity, Entity>,
    pub(super) views: Vec<(Entity, usize)>,
    pub(super) quality: Option<GraphicsEffectQuality>,
}

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
    pub(super) removed_draws: RemovedComponents<'w, 's, TerrainWaterCausticScenePass>,
}
