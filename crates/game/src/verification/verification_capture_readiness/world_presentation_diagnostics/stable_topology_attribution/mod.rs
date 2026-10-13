use std::{any::TypeId, path::Path};

use bevy::{
    camera::{visibility::VisibleEntities, RenderTarget},
    ecs::{query::QueryData, system::SystemParam},
    prelude::*,
    window::PrimaryWindow,
    world_serialization::WorldInstance,
};
use openzt2_game_data::AssetId;

use crate::{
    plugins::{
        model_render::ModelExpanded,
        terrain::{
            terrain_chunk_identity_type::TerrainChunkId,
            terrain_chunk_presentation_types::{TerrainRenderRevision, TerrainSurfaceImage},
            terrain_chunk_types::{TerrainChunk, TerrainIndex},
            terrain_ground_path_surface_projection::{
                GroundPathSurfaceImageDependencies, GroundPathSurfaceProjectionDirty,
                GroundPathSurfaceProjectionRevision,
            },
            terrain_world_sampling::terrain_chunk_at,
        },
        topology::{
            path_curb_presentation::PathCurbsHydrated,
            topology_graph_types::{FenceEdge, PathTile, TopologyNode},
            topology_presentation_types::{
                FenceSegmentPrefab, FenceTerrainEndpointPresentationCells,
                FenceTerrainEndpointSkewPresentation, TopologyPresentation,
            },
        },
        world_spawn::{
            persistent_id_types::PersistentId,
            prefab_presentation_types::{
                PrefabModel, PrefabPresentation, PrefabPresentationHydrated,
            },
            selected_world_identity::SelectedWorldIdentity,
            selected_world_terrain_asset_handle::SelectedWorldTerrainAssetHandle,
            world_membership_types::{WorldMember, WorldRoot},
        },
    },
    verification::verification_journey_state_types::VerificationJourneySession,
};
use openzt2_game_assets::{
    asset_source::AssetArchives,
    {
        world_definitions::{
            world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
            world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        },
        world_scenario::{
            world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
            world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
        },
    },
};

const DETAIL_LIMIT: usize = 4;

#[derive(QueryData)]
struct TopologyOwnerObservation {
    entity: Entity,
    member: &'static WorldMember,
    persistent: Option<&'static PersistentId>,
    path: Option<&'static PathTile>,
    fence: Option<&'static FenceEdge>,
    curbs: Option<&'static PathCurbsHydrated>,
    presentation: Option<&'static TopologyPresentation>,
    selected_prefab: Option<&'static FenceSegmentPrefab>,
    fitted_endpoints: Option<&'static FenceTerrainEndpointPresentationCells>,
    endpoint_skew: Option<&'static FenceTerrainEndpointSkewPresentation>,
}

#[derive(QueryData)]
struct PresentationNodeObservation {
    parent: Option<&'static ChildOf>,
    name: Option<&'static Name>,
    local: Option<&'static Transform>,
    global: Option<&'static GlobalTransform>,
    visibility: Option<&'static Visibility>,
    inherited: Option<&'static InheritedVisibility>,
    node: Option<&'static TopologyNode>,
    persistent: Option<&'static PersistentId>,
    prefab: Option<&'static PrefabPresentation>,
    hydrated: Has<PrefabPresentationHydrated>,
    model: Option<&'static PrefabModel>,
    expanded: Has<ModelExpanded>,
    world_asset_root: Option<&'static WorldAssetRoot>,
    instance: Option<&'static WorldInstance>,
    mesh: Option<&'static Mesh3d>,
}

#[derive(SystemParam)]
pub(in crate::verification::verification_capture_readiness) struct StableTopologyCaptureQueries<
    'w,
    's,
> {
    roots: Query<
        'w,
        's,
        (
            Entity,
            &'static WorldRoot,
            Option<&'static SelectedWorldIdentity>,
            Option<&'static SelectedWorldTerrainAssetHandle>,
        ),
    >,
    owners: Query<'w, 's, TopologyOwnerObservation, Or<(With<PathTile>, With<FenceEdge>)>>,
    nodes: Query<'w, 's, PresentationNodeObservation>,
    children: Query<'w, 's, &'static Children>,
    chunks: Query<
        'w,
        's,
        (
            Option<&'static TerrainChunkId>,
            &'static WorldMember,
            &'static TerrainChunk,
            Option<&'static TerrainSurfaceImage>,
            Option<&'static TerrainRenderRevision>,
            Option<&'static GroundPathSurfaceProjectionRevision>,
            Has<GroundPathSurfaceProjectionDirty>,
        ),
    >,
    terrain_index: Res<'w, TerrainIndex>,
    definitions: Res<'w, WorldDefinitions>,
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    scenarios: Res<'w, WorldScenarios>,
    scenario_assets: Res<'w, Assets<WorldScenarioDocumentAsset>>,
    archives: Res<'w, AssetArchives>,
    server: Res<'w, AssetServer>,
    instances: Res<'w, WorldInstanceSpawner>,
    path_images: Res<'w, GroundPathSurfaceImageDependencies>,
    images: Res<'w, Assets<Image>>,
    pub(super) primary_windows: Query<'w, 's, Entity, With<PrimaryWindow>>,
}

fn retain_lowest_ids(samples: &mut Vec<(u64, Entity)>, candidate: (u64, Entity)) {
    let index = samples.partition_point(|existing| *existing < candidate);
    if index < DETAIL_LIMIT {
        samples.insert(index, candidate);
        samples.truncate(DETAIL_LIMIT);
    }
}

impl StableTopologyCaptureQueries<'_, '_> {
    // The caller guards the two CPU capture checkpoints before any collection.
    // Cost scales with current topology, sampled descendant trees and retained
    // projected-image dependencies (also searched for each sampled path).
    // Bounded mesh samples test each exact camera's borrowed visibility list.
    // Only scalar sample IDs and borrowed image handles are retained locally.
    pub(super) fn log(
        &self,
        session: &VerificationJourneySession,
        stage: &str,
        capture: Option<&str>,
        cameras: &[(Entity, &Camera, &RenderTarget, &VisibleEntities)],
    ) {
        let span = info_span!(target: "openzt2_verification", "world presentation CPU checkpoint",
            journey = %session.journeys[session.journey_index].name,
            step = session.step_index, frame = session.frames_run, stage, ?capture);
        let _checkpoint = span.enter();
        let current = self.roots.single().ok();
        info!(target: "openzt2_verification", roots = self.roots.iter().count(),
            current_root = ?current.map(|(entity, ..)| entity),
            world_root = ?current.map(|(_, root, ..)| root),
            selected_world = ?current.and_then(|(_, _, identity, _)| identity),
            archive_revision = self.archives.revision(), scenario_revision = ?self.scenarios.revision(),
            scenario_catalogue_complete = self.scenarios.catalogue_documents_are_loaded(&self.scenario_assets),
            definition_catalogue_revision = self.definitions.catalogue_revision(),
            definition_catalogue_complete = self.definitions.is_complete(),
            definition_loading_progress = ?self.definitions.loading_progress(),
            definition_view_available = self.definitions.get(&self.definition_assets).is_some(),
            source_metadata = "current ArchiveSet winner, not historical loaded-asset provenance",
            sampling = "lowest persistent IDs per current-world category, independent of visibility; no entrance identity",
            per_category_limit = DETAIL_LIMIT,
            "capture canonical world publication and sample contract");
        let Some((root, _, identity, terrain)) = current else {
            return;
        };
        let scenarios = self.scenarios.get(&self.scenario_assets);
        self.log_asset(
            "selected map",
            identity
                .and_then(|id| scenarios?.handle_for_map(id.map))
                .map(Handle::id),
        );
        self.log_asset(
            "selected start",
            identity
                .and_then(|id| scenarios?.handle_for_start(id.start))
                .map(Handle::id),
        );
        self.log_asset("selected terrain", terrain.map(|terrain| terrain.0.id()));

        // One borrowed topology scan retains only four scalar identities per
        // category. No scene history or ordinary-frame collection is installed.
        let definitions = self.definitions.get(&self.definition_assets);
        let mut samples = [Vec::new(), Vec::new(), Vec::new()];
        let mut totals = [0usize; 3];
        let mut unidentified = [0usize; 3];
        let mut unresolved_path_definitions = 0usize;
        for owner in &self.owners {
            if owner.member.root != root {
                continue;
            }
            let path_definition = owner
                .path
                .and_then(|path| definitions?.find_path(path.definition));
            unresolved_path_definitions +=
                usize::from(owner.path.is_some() && path_definition.is_none());
            let categories = [
                owner.path.is_some(),
                path_definition.is_some_and(|path| AssetId(path.curb.0) != AssetId::default()),
                owner.fence.is_some(),
            ];
            for (index, eligible) in categories.into_iter().enumerate() {
                if !eligible {
                    continue;
                }
                totals[index] += 1;
                if let Some(persistent) = owner.persistent {
                    retain_lowest_ids(&mut samples[index], (persistent.0, owner.entity));
                } else {
                    unidentified[index] += 1;
                }
            }
        }
        for (index, category) in ["path", "curb", "fence"].into_iter().enumerate() {
            info!(target: "openzt2_verification", category, total = totals[index],
                missing_persistent_ids = unidentified[index], samples = samples[index].len(),
                omitted_owners = totals[index].saturating_sub(samples[index].len()), unresolved_path_definitions,
                "capture stable topology sample coverage");
            for &(persistent_id, entity) in &samples[index] {
                let Ok(owner) = self.owners.get(entity) else {
                    continue;
                };
                let span = info_span!(target: "openzt2_verification", "world topology subject", category, persistent_id, ?entity, ?root);
                let _subject = span.enter();
                let definition = owner
                    .path
                    .map(|path| path.definition)
                    .or_else(|| owner.fence.map(|fence| fence.definition));
                let definition_handle = definitions.and_then(|definitions| {
                    if owner.path.is_some() {
                        definitions.handle_for_path(definition?)
                    } else {
                        definitions.handle_for_fence(definition?)
                    }
                });
                info!(target: "openzt2_verification", ?definition,
                    path_cell = ?owner.path.map(|path| path.cell),
                    authored_curb = ?owner.path.and_then(|path| definitions?.find_path(path.definition)).map(|path| path.curb),
                    edge_nodes = ?owner.fence.map(|fence| (fence.a, fence.b)),
                    fitted_endpoints = ?owner.fitted_endpoints,
                    endpoint_skew = ?owner.endpoint_skew,
                    selected_prefab = ?owner.selected_prefab, presentation = ?owner.presentation,
                    curb_root = ?owner.curbs.and_then(PathCurbsHydrated::presentation_root),
                    "capture actual topology owner");
                self.log_asset("topology definition", definition_handle.map(Handle::id));
                if let Some(path) = owner
                    .path
                    .and_then(|path| definitions?.find_path(path.definition))
                {
                    if category == "curb" {
                        self.log_asset(
                            "authored curb definition",
                            definitions
                                .and_then(|definitions| {
                                    definitions.handle_for_fence(AssetId(path.curb.0))
                                })
                                .map(Handle::id),
                        );
                    }
                    let surface = AssetId(path.surface_texture.0);
                    let dependency = self
                        .path_images
                        .projected_images()
                        .find(|(identifier, _)| *identifier == surface);
                    info!(target: "openzt2_verification", ?surface,
                        in_projected_dependency_set = dependency.is_some(),
                        loaded = ?dependency.map(|(_, handle)| handle.is_some_and(|handle| self.images.contains(handle.id()))),
                        "capture authored path surface dependency; not effective overlay membership");
                    self.log_asset(
                        "authored path surface",
                        dependency.and_then(|(_, handle)| handle).map(Handle::id),
                    );
                }
                self.log_node(entity);
                if let Some(fence) = owner.fence {
                    self.log_node(fence.a);
                    self.log_node(fence.b);
                }
                if let Some(skew) = owner.endpoint_skew {
                    for entity in skew.transform_entities() {
                        self.log_node(entity);
                    }
                }
                let presentation_root = if category == "curb" {
                    owner.curbs.and_then(PathCurbsHydrated::presentation_root)
                } else {
                    Some(entity)
                };
                self.log_descendants(presentation_root, cameras);
                self.log_raster(root, entity, cameras);
            }
        }
        let mut dependencies = Vec::new();
        let mut dependency_count = 0usize;
        let mut missing_count = 0;
        for (identifier, handle) in self.path_images.projected_images() {
            dependency_count += 1;
            missing_count +=
                usize::from(handle.is_none_or(|handle| !self.images.contains(handle.id())));
            let index = dependencies.partition_point(|(existing, _)| *existing < identifier);
            if index < DETAIL_LIMIT {
                dependencies.insert(index, (identifier, handle));
                dependencies.truncate(DETAIL_LIMIT);
            }
        }
        info!(target: "openzt2_verification", dependency_count, missing_count,
            omitted_dependencies = dependency_count.saturating_sub(dependencies.len()),
            mask_association = "None: effective per-path mask/overlay selections are transient, not retained",
            curb_endpoint_association = "None: generated curb segment cells are not retained; actual prefab transforms are logged",
            "capture retained path raster dependency scope");
        for (identifier, handle) in dependencies {
            info!(target: "openzt2_verification", ?identifier, image = ?handle.map(Handle::id),
                loaded = handle.is_some_and(|handle| self.images.contains(handle.id())),
                "capture retained projected image dependency");
            self.log_asset("projected image dependency", handle.map(Handle::id));
        }
    }

    fn log_asset<A: Asset>(&self, role: &str, handle: Option<bevy::asset::AssetId<A>>) {
        let path = handle.and_then(|id| self.server.get_path(id));
        let winner = path
            .as_ref()
            .and_then(|path| self.archives.resolved_winning_entry(path.path()));
        info!(target: "openzt2_verification", role, ?handle, ?path,
            load_states = ?handle.and_then(|id| self.server.get_load_states(id)),
            winner_normalized_path = ?winner.as_ref().map(|entry| &entry.normalized_asset_path),
            winner_original_path = ?winner.as_ref().map(|entry| &entry.original_archive_path),
            winner_order = ?winner.as_ref().map(|entry| (entry.archive_priority_index, entry.zip_entry_ordinal)),
            "capture canonical asset source metadata");
    }

    fn log_node(&self, entity: Entity) {
        let node = self.nodes.get(entity).ok();
        info!(target: "openzt2_verification", ?entity, present = node.is_some(),
            persistent_id = ?node.as_ref().and_then(|node| node.persistent).map(|id| id.0),
            parent = ?node.as_ref().and_then(|node| node.parent).map(ChildOf::parent),
            name = ?node.as_ref().and_then(|node| node.name),
            cell = ?node.as_ref().and_then(|node| node.node).map(|node| node.cell),
            local = ?node.as_ref().and_then(|node| node.local),
            global = ?node.as_ref().and_then(|node| node.global).map(GlobalTransform::to_matrix),
            "capture actual topology node transform");
    }

    fn log_descendants(
        &self,
        root: Option<Entity>,
        cameras: &[(Entity, &Camera, &RenderTarget, &VisibleEntities)],
    ) {
        let mut counts = [0usize; 3];
        info!(target: "openzt2_verification", ?root, "capture topology presentation traversal");
        if let Some(root) = root {
            // Counts scale with these sampled owners' actual descendants. Detail
            // logs are bounded separately for prefabs, models and mesh draws.
            for entity in std::iter::once(root)
                .chain(self.children.iter_descendants_depth_first::<Children>(root))
            {
                let Ok(node) = self.nodes.get(entity) else {
                    continue;
                };
                if let Some(prefab) = node.prefab {
                    counts[0] += 1;
                    if counts[0] <= DETAIL_LIMIT {
                        self.log_node(entity);
                        info!(target: "openzt2_verification", ?entity, hydrated = node.hydrated, "capture topology prefab");
                        self.log_asset("topology prefab", Some(prefab.0.id()));
                    }
                }
                if let Some(model) = node.model {
                    counts[1] += 1;
                    if counts[1] <= DETAIL_LIMIT {
                        self.log_node(entity);
                        let winner = self
                            .archives
                            .resolved_winning_entry(Path::new(model.model_path.as_ref()));
                        info!(target: "openzt2_verification", ?entity, model = ?model.model, path = %model.model_path,
                            scene_name = ?model.scene_name, expanded = node.expanded, world_asset_root = ?node.world_asset_root,
                            instance = ?node.instance.map(|instance| **instance),
                            instance_ready = ?node.instance.map(|instance| self.instances.instance_is_ready(**instance)),
                            winner_order = ?winner.as_ref().map(|entry| (entry.archive_priority_index, entry.zip_entry_ordinal)),
                            winner_original_path = ?winner.as_ref().map(|entry| &entry.original_archive_path),
                            "capture topology model identity and expansion");
                        self.log_asset("topology model", model.handle.as_ref().map(Handle::id));
                        self.log_asset(
                            "instantiated model scene",
                            node.world_asset_root.map(|root| root.0.id()),
                        );
                    }
                }
                if let Some(mesh) = node.mesh {
                    counts[2] += 1;
                    if counts[2] <= DETAIL_LIMIT {
                        self.log_node(entity);
                        info!(target: "openzt2_verification", ?entity, mesh = ?mesh.0.id(),
                            visibility = ?node.visibility, inherited_visibility = ?node.inherited.map(|visibility| visibility.get()),
                            "capture actual topology mesh draw");
                        self.log_asset("topology mesh", Some(mesh.0.id()));
                        for (camera, _, _, visible) in cameras {
                            info!(target: "openzt2_verification", ?entity, ?camera,
                                listed = visible.get(TypeId::of::<Mesh3d>()).contains(&entity),
                                "capture exact target-camera mesh association");
                        }
                    }
                }
            }
        }
        info!(target: "openzt2_verification", prefabs = counts[0], models = counts[1], meshes = counts[2],
            omitted_details = ?counts.map(|count| count.saturating_sub(DETAIL_LIMIT)),
            "capture topology descendant detail coverage");
    }

    fn log_raster(
        &self,
        world: Entity,
        owner: Entity,
        cameras: &[(Entity, &Camera, &RenderTarget, &VisibleEntities)],
    ) {
        let position = self
            .nodes
            .get(owner)
            .ok()
            .and_then(|node| node.global)
            .map(GlobalTransform::translation)
            .filter(|position| position.is_finite());
        let chunk_entity =
            position.and_then(|position| terrain_chunk_at(&self.terrain_index, position.xz()));
        let chunk = chunk_entity
            .and_then(|entity| self.chunks.get(entity).ok())
            .filter(|(_, member, ..)| member.root == world);
        info!(target: "openzt2_verification", ?position, ?chunk_entity, same_world_chunk = chunk.is_some(),
            association = "owner origin through TerrainIndex; effective raster overlay membership is not retained",
            "capture topology-origin raster association");
        let Some((id, _, chunk, image, revision, projected, dirty)) = chunk else {
            return;
        };
        info!(target: "openzt2_verification", chunk_id = ?id.map(|id| id.0), coord = ?chunk.coord,
            source_coord = ?chunk.source_coord, origin = ?chunk.origin, spacing_m = chunk.spacing_m,
            side = chunk.side, render_revision = ?revision, projected_revision = ?projected, dirty,
            raster_current = ?revision.zip(projected).map(|(revision, projected)| projected.matches_render_revision(revision)),
            image = ?image.map(|image| image.0.id()), image_loaded = image.is_some_and(|image| self.images.contains(image.0.id())),
            image_extent = ?image.and_then(|image| self.images.get(&image.0)).map(|image| image.texture_descriptor.size),
            "capture actual terrain raster owner");
        self.log_asset("raster terrain", Some(chunk.asset.id()));
        self.log_asset("raster image", image.map(|image| image.0.id()));
        if let Some(entity) = chunk_entity {
            for (camera, _, _, visible) in cameras {
                info!(target: "openzt2_verification", ?entity, ?camera,
                    listed = visible.get(TypeId::of::<Mesh3d>()).contains(&entity),
                    "capture exact target-camera terrain association");
            }
        }
    }
}
