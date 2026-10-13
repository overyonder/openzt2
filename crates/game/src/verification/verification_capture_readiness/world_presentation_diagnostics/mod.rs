use std::any::TypeId;

mod stable_topology_attribution;

use self::stable_topology_attribution::StableTopologyCaptureQueries;

use bevy::{
    asset::AssetId,
    camera::{visibility::VisibleEntities, RenderTarget},
    ecs::{query::QueryEntityError, system::SystemParam},
    prelude::*,
    world_serialization::WorldInstance,
};

use crate::{
    plugins::{
        model_render::ModelExpanded,
        terrain::{
            terrain_chunk_presentation_types::TerrainRenderRevision,
            terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
            terrain_ground_path_surface_projection::{
                GroundPathSurfaceImageDependencies, GroundPathSurfaceProjectionDirty,
                GroundPathSurfaceProjectionRevision,
            },
        },
        topology::{
            path_curb_presentation::PathCurbsHydrated,
            topology_graph_types::{FenceEdge, PathTile},
            topology_presentation_types::TopologyPresentation,
        },
        ui::{authored_tooltip_presentation::UiTooltipPolicy, picking::UiPointerCapture},
        world_spawn::{
            persistent_id_types::PersistentId,
            prefab_presentation_types::{
                PrefabModel, PrefabPresentation, PrefabPresentationHydrated,
            },
            selected_world_identity::SelectedWorldIdentity,
            world_membership_types::{WorldMember, WorldRoot},
        },
    },
    verification::{
        verification_journey_state_types::{
            VerificationCaptureProgress, VerificationJourneySession,
        },
        verification_journey_types::VerificationJourneyAction,
    },
};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

const FENCE_FITTING_TRANSITION_LIMIT: u8 = 16;

#[derive(Resource, Default)]
pub(crate) struct FenceTerrainFittingHistory {
    lifetime: Option<(usize, u32, Entity)>,
    owners: [FenceTerrainFittingOwnerHistory; 4],
}

#[derive(Default)]
struct FenceTerrainFittingOwnerHistory {
    last: Option<FenceTerrainFittingAttempt>,
    attempt_seen: bool,
    emitted: u8,
    omitted: u64,
}

#[derive(Debug, PartialEq)]
pub(crate) struct FenceTerrainFittingAttempt {
    slot: usize,
    persistent_id: u64,
    root: Entity,
    entity: Entity,
    endpoints: [IVec3; 2],
    skew_entities: [Entity; 2],
    triggers: [bool; 4],
    horizontal_length_m: f32,
    owner_before: Transform,
    owner_after: Transform,
    skew_before: [Option<Transform>; 2],
    skew_after: [Option<Transform>; 2],
    samples: [Option<FenceTerrainEndpointSample>; 2],
    child_query_result: Option<Result<(), QueryEntityError>>,
    outcome: &'static str,
}

impl FenceTerrainFittingAttempt {
    pub(crate) fn record_child_query_result(&mut self, result: Result<(), QueryEntityError>) {
        self.child_query_result = Some(result);
    }
}

#[derive(Debug, PartialEq)]
struct FenceTerrainEndpointSample {
    world_xz: Vec2,
    index_origin: Vec2,
    index_span_m: f32,
    chunk_entity: Option<Entity>,
    chunk_root: Option<Entity>,
    chunk_coord: Option<IVec2>,
    source_coord: Option<IVec2>,
    terrain_asset: Option<AssetId<TerrainAsset>>,
    edited_overlay_available: bool,
    height_m: Option<f32>,
    availability: &'static str,
}

#[derive(SystemParam)]
pub(crate) struct FenceTerrainFittingObservation<'w, 's> {
    history: Option<ResMut<'w, FenceTerrainFittingHistory>>,
    session: Option<Res<'w, VerificationJourneySession>>,
    roots: Query<'w, 's, Entity, (With<WorldRoot>, With<SelectedWorldIdentity>)>,
    owners: Query<'w, 's, (&'static PersistentId, &'static WorldMember), With<FenceEdge>>,
    chunk_members: Query<'w, 's, &'static WorldMember, With<TerrainChunk>>,
    server: Option<Res<'w, AssetServer>>,
}

impl FenceTerrainFittingObservation<'_, '_> {
    pub(crate) fn begin_frame(&mut self) {
        let Some(history) = self.history.as_mut() else {
            return;
        };
        let lifetime = self
            .session
            .as_ref()
            .filter(|session| !session.finished)
            .and_then(|session| {
                self.roots
                    .single()
                    .ok()
                    .map(|root| (session.journey_index, session.world_loads_started, root))
            });
        if history.lifetime != lifetime {
            info!(target: "openzt2_verification", previous = ?history.lifetime, current = ?lifetime,
                omitted = ?history.owners.each_ref().map(|owner| owner.omitted),
                "fence fitting observation lifetime changed; history discarded");
            **history = FenceTerrainFittingHistory {
                lifetime,
                ..default()
            };
        }
    }

    pub(crate) fn observes_owner(&self, entity: Entity) -> bool {
        let Some((_, _, root)) = self.history.as_ref().and_then(|history| history.lifetime) else {
            return false;
        };
        self.owners
            .get(entity)
            .is_ok_and(|(id, member)| member.root == root && matches!(id.0, 3 | 5 | 7 | 9))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin_owner(
        &self,
        entity: Entity,
        endpoints: [IVec3; 2],
        skew_entities: [Entity; 2],
        triggers: [bool; 4],
        horizontal_length_m: f32,
        owner: Transform,
        skew: [Option<Transform>; 2],
    ) -> Option<FenceTerrainFittingAttempt> {
        if !self.observes_owner(entity) {
            return None;
        }
        let (id, member) = self.owners.get(entity).ok()?;
        let slot = match id.0 {
            3 => 0,
            5 => 1,
            7 => 2,
            9 => 3,
            _ => return None,
        };
        Some(FenceTerrainFittingAttempt {
            slot,
            persistent_id: id.0,
            root: member.root,
            entity,
            endpoints,
            skew_entities,
            triggers,
            horizontal_length_m,
            owner_before: owner,
            owner_after: owner,
            skew_before: skew,
            skew_after: skew,
            samples: [None, None],
            child_query_result: None,
            outcome: "not finished",
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_endpoint_sample(
        &self,
        attempt: &mut FenceTerrainFittingAttempt,
        endpoint: usize,
        world_xz: Vec2,
        index: &TerrainIndex,
        chunk_entity: Option<Entity>,
        chunk: Option<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
        asset_available: bool,
        height_m: Option<f32>,
    ) {
        attempt.samples[endpoint] = Some(FenceTerrainEndpointSample {
            world_xz,
            index_origin: index.origin,
            index_span_m: index.chunk_span_m,
            chunk_entity,
            chunk_root: chunk_entity
                .and_then(|entity| self.chunk_members.get(entity).ok())
                .map(|member| member.root),
            source_coord: chunk.map(|(chunk, _)| chunk.source_coord),
            chunk_coord: chunk.map(|(chunk, _)| chunk.coord),
            terrain_asset: chunk.map(|(chunk, _)| chunk.asset.id()),
            edited_overlay_available: chunk.is_some_and(|(_, edited)| edited.is_some()),
            height_m,
            availability: if chunk_entity.is_none() {
                "spatial lookup unavailable"
            } else if chunk.is_none() {
                "indexed chunk unavailable"
            } else if !asset_available {
                "terrain asset unavailable"
            } else if height_m.is_none() {
                "canonical sampler returned none"
            } else {
                "sample available"
            },
        });
    }

    pub(crate) fn finish_owner(
        &mut self,
        attempt: Option<FenceTerrainFittingAttempt>,
        outcome: &'static str,
        owner: Transform,
        skew: [Option<Transform>; 2],
        frame: Option<u32>,
    ) {
        let (Some(mut attempt), Some(history)) = (attempt, self.history.as_mut()) else {
            return;
        };
        attempt.outcome = outcome;
        attempt.owner_after = owner;
        attempt.skew_after = skew;
        let lifetime = history.lifetime;
        let previous = &mut history.owners[attempt.slot];
        let first_membership = previous.last.as_ref().is_none_or(|last| {
            last.entity != attempt.entity
                || last.root != attempt.root
                || last.endpoints != attempt.endpoints
        });
        if first_membership {
            previous.attempt_seen = false;
        }
        let production_attempted = attempt.triggers.iter().any(|trigger| *trigger);
        let first_attempt = production_attempted && !previous.attempt_seen;
        previous.attempt_seen |= production_attempted;
        let sampleable = |attempt: &FenceTerrainFittingAttempt| {
            attempt.samples.iter().all(|sample| {
                sample
                    .as_ref()
                    .is_some_and(|sample| sample.height_m.is_some())
            })
        };
        let newly_observed_sampleable =
            sampleable(&attempt) && previous.last.as_ref().is_none_or(|last| !sampleable(last));
        if previous.last.as_ref() == Some(&attempt)
            || previous.emitted >= FENCE_FITTING_TRANSITION_LIMIT
        {
            previous.omitted = previous.omitted.saturating_add(1);
        } else {
            let production_fit_completed = outcome == "production fit completed";
            info!(target: "openzt2_verification", ?frame,
                journey = ?lifetime.map(|(journey, ..)| journey),
                first_membership, first_attempt, newly_observed_sampleable,
                terrain_changed = attempt.triggers[0], endpoint_cells_changed = attempt.triggers[1],
                skew_presentation_changed = attempt.triggers[2], topology_grid_changed = attempt.triggers[3],
                production_gate_skipped = !attempt.triggers.iter().any(|trigger| *trigger),
                production_fit_completed, observation = ?attempt,
                terrain_paths = ?attempt.samples.each_ref().map(|sample| sample.as_ref().and_then(|sample| sample.terrain_asset).and_then(|asset| self.server.as_ref().and_then(|server| server.get_path(asset)))),
                same_world_chunks = ?attempt.samples.each_ref().map(|sample| sample.as_ref().map(|sample| sample.chunk_root.map(|root| root == attempt.root))),
                omitted_observations = previous.omitted,
                transition_limit = FENCE_FITTING_TRANSITION_LIMIT,
                omissions = "other fence IDs, owners outside current world and missing fitter query membership excluded; None sample means endpoint not reached; skipped-gate samples are observation only; sampler rejection substage unavailable",
                "sampled imported fence fitting transition; no entrance identity");
            previous.emitted += 1;
            if previous.emitted == FENCE_FITTING_TRANSITION_LIMIT {
                info!(target: "openzt2_verification", root = ?attempt.root, persistent_id = attempt.persistent_id,
                    "fence fitting transition limit reached; subsequent records omitted until lifetime reset");
            }
        }
        previous.last = Some(attempt);
    }
}

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub(super) fn log_capture_world_presentation_dependencies(
    (session, fitting_history): (
        Res<VerificationJourneySession>,
        Option<Res<FenceTerrainFittingHistory>>,
    ),
    cameras: Query<(Entity, &Camera, &RenderTarget, &VisibleEntities)>,
    topology: Query<
        (
            Entity,
            Option<&PathTile>,
            Option<&FenceEdge>,
            Option<&PathCurbsHydrated>,
            Option<&TopologyPresentation>,
        ),
        Or<(With<PathTile>, With<FenceEdge>)>,
    >,
    prefabs: Query<(Entity, &PrefabPresentation, Has<PrefabPresentationHydrated>)>,
    models: Query<(
        Entity,
        &PrefabModel,
        &InheritedVisibility,
        Has<ModelExpanded>,
        Option<&WorldAssetRoot>,
        Option<&WorldInstance>,
    )>,
    terrain: Query<(
        Entity,
        &TerrainRenderRevision,
        Option<&GroundPathSurfaceProjectionRevision>,
        Has<GroundPathSurfaceProjectionDirty>,
    )>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    positions: Query<&GlobalTransform>,
    world_instances: Res<WorldInstanceSpawner>,
    path_images: Res<GroundPathSurfaceImageDependencies>,
    images: Res<Assets<Image>>,
    server: Res<AssetServer>,
    tooltips: Query<(Entity, &UiTooltipPolicy, &Visibility, &Text)>,
    pointer: Res<UiPointerCapture>,
    attribution: StableTopologyCaptureQueries,
) {
    // First increments step_frame after advancing the capture. Sample only its
    // first preparation frame and the frame that creates the screenshot request.
    let stage = match session.step_capture {
        VerificationCaptureProgress::Preparing { .. } if session.step_frame == 1 => "preparing",
        VerificationCaptureProgress::Requested { frame, .. } if session.step_frame == frame + 1 => {
            "requested"
        }
        _ => return,
    };
    let capture_name = session
        .steps
        .get(session.step_index)
        .and_then(|step| match &step.action {
            VerificationJourneyAction::Capture(name) => Some(name.as_str()),
            _ => None,
        });
    if let Some(history) = fitting_history {
        info!(target: "openzt2_verification", stage, ?capture_name, lifetime = ?history.lifetime,
            emitted = ?history.owners.each_ref().map(|owner| owner.emitted),
            omitted = ?history.owners.each_ref().map(|owner| owner.omitted),
            persistent_ids = ?[3u64, 5, 7, 9],
            "capture bounded fence fitting history coverage; unchanged and over-limit observations omitted");
    }
    let primary_window = attribution.primary_windows.single().ok();
    let capture_target = session
        .capture_target
        .as_ref()
        .and_then(|target| target.normalize(primary_window));
    let capture_cameras = cameras
        .iter()
        .filter(|(_, camera, target, _)| {
            camera.is_active
                && capture_target.is_some()
                && target.normalize(primary_window) == capture_target
        })
        .collect::<Vec<_>>();
    let in_capture_view = |entity: Entity| {
        capture_cameras
            .iter()
            .any(|(_, _, _, visible)| visible.get(TypeId::of::<Mesh3d>()).contains(&entity))
    };
    let topology_owner = |entity: Entity| {
        std::iter::once(entity)
            .chain(parents.iter_ancestors(entity))
            .find(|ancestor| topology.contains(*ancestor))
    };
    let topology_prefabs = prefabs
        .iter()
        .filter(|(entity, ..)| topology_owner(*entity).is_some())
        .collect::<Vec<_>>();
    let topology_models = models
        .iter()
        .filter(|(entity, ..)| topology_owner(*entity).is_some())
        .collect::<Vec<_>>();
    let instance_ready = |expanded: bool,
                          root: Option<&WorldAssetRoot>,
                          instance: Option<&WorldInstance>| {
        expanded
            && (root.is_none()
                || instance.is_some_and(|instance| world_instances.instance_is_ready(**instance)))
    };
    let meshes_in_capture_view = |entity: Entity, instance: Option<&WorldInstance>| {
        usize::from(in_capture_view(entity))
            + instance.map_or(0, |instance| {
                world_instances
                    .iter_instance_entities(**instance)
                    .filter(|entity| in_capture_view(*entity))
                    .count()
            })
    };
    let visible_terrain = terrain
        .iter()
        .filter(|(entity, ..)| in_capture_view(*entity))
        .collect::<Vec<_>>();
    let mut missing_images = path_images
        .missing_projected_images(&images)
        .collect::<Vec<_>>();
    missing_images.sort_unstable_by_key(|(identifier, _)| *identifier);
    info!(target: "openzt2_verification", journey = %session.journeys[session.journey_index].name,
        step = session.step_index, frame = session.frames_run, stage, ?capture_name,
        capture_progress = ?session.step_capture, capture_target = ?session.capture_target,
        capture_cameras = ?capture_cameras.iter()
            .map(|(entity, ..)| entity).collect::<Vec<_>>(),
        paths = topology.iter().filter(|(_, path, ..)| path.is_some()).count(),
        paths_without_curb_projection = topology.iter().filter(|(_, path, _, curbs, _)| path.is_some() && curbs.is_none()).count(),
        paths_with_projected_curb_segments = topology.iter().filter(|(_, path, _, curbs, _)| path.is_some() && curbs.is_some_and(|curbs| curbs.has_projected_segments())).count(),
        topology_without_prefab_projection = topology.iter().filter(|(_, _, _, _, presentation)| presentation.is_none()).count(),
        topology_prefabs = topology_prefabs.len(), unhydrated_topology_prefabs = topology_prefabs.iter().filter(|(_, _, hydrated)| !hydrated).count(),
        topology_models = topology_models.len(), inherited_visible_topology_models = topology_models.iter().filter(|(_, _, visible, ..)| visible.get()).count(),
        unexpanded_visible_topology_models = topology_models.iter().filter(|(_, _, visible, expanded, root, instance)| visible.get() && !instance_ready(*expanded, *root, *instance)).count(),
        topology_models_with_meshes_in_capture_view = topology_models.iter().filter(|(entity, _, _, _, _, instance)| meshes_in_capture_view(*entity, *instance) > 0).count(),
        visible_terrain = visible_terrain.len(),
        pending_visible_path_rasters = visible_terrain.iter().filter(|(_, revision, projected, dirty)| *dirty || projected.is_none_or(|projected| !projected.matches_render_revision(revision))).count(),
        missing_path_images = missing_images.len(),
        "capture topology and path raster producer state");
    // Counts cover all topology; bounded samples identify the canonical requests
    // and actual expansion stages without treating this inventory as a gate.
    for (entity, path, fence, curbs, presentation) in topology
        .iter()
        .filter(|(_, path, _, curbs, presentation)| {
            (path.is_some() && curbs.is_none()) || presentation.is_none()
        })
        .take(4)
    {
        info!(target: "openzt2_verification", stage, ?entity,
            path = ?path.map(|path| (path.definition, path.cell)),
            fence = ?fence.map(|fence| fence.definition),
            curbs_projected = curbs.is_some(), topology_projected = presentation.is_some(),
            position = ?positions.get(entity).ok().map(GlobalTransform::translation),
            "capture pending topology projection");
    }
    for (entity, prefab, hydrated) in topology_prefabs
        .into_iter()
        .filter(|(_, _, hydrated)| !hydrated)
        .take(4)
    {
        info!(target: "openzt2_verification", stage, ?entity, hydrated,
            owner = ?topology_owner(entity), path = ?server.get_path(prefab.0.id()),
            load_states = ?server.get_load_states(prefab.0.id()),
            position = ?positions.get(entity).ok().map(GlobalTransform::translation),
            "capture pending topology prefab");
    }
    attribution.log(&session, stage, capture_name, &capture_cameras);
    for (entity, revision, projected, dirty) in visible_terrain.into_iter().take(4) {
        info!(target: "openzt2_verification", stage, ?entity, render_revision = revision.0,
            path_raster_current = projected.is_some_and(|projected| projected.matches_render_revision(revision)),
            dirty, position = ?positions.get(entity).ok().map(GlobalTransform::translation),
            "capture visible terrain path raster sample");
    }
    for (identifier, handle) in missing_images.into_iter().take(4) {
        info!(target: "openzt2_verification", stage, ?identifier,
            path = ?handle.and_then(|handle| server.get_path(handle.id())),
            load_states = ?handle.and_then(|handle| server.get_load_states(handle.id())),
            "capture pending path raster image");
    }
    for (entity, policy, visibility, text) in tooltips
        .iter()
        .filter(|(_, _, _, text)| !text.0.is_empty())
        .take(4)
    {
        info!(target: "openzt2_verification", stage, ?entity, name = ?names.get(entity).ok(),
            pointer_target = ?pointer.target, ?visibility, appear_seconds = policy.appear_seconds,
            autohide = policy.autohide, display_seconds = policy.display_seconds,
            text = %text.0.chars().take(160).collect::<String>(),
            "capture authored tooltip presentation sample");
    }
}
