use std::any::TypeId;

mod stable_topology_attribution;

use self::stable_topology_attribution::StableTopologyCaptureQueries;

use bevy::{
    camera::{visibility::VisibleEntities, RenderTarget},
    prelude::*,
    world_serialization::WorldInstance,
};

use crate::{
    verification_journey_state_types::{VerificationCaptureProgress, VerificationJourneySession},
    verification_journey_types::VerificationJourneyAction,
};
use openzt2_construction::topology::path_curb_presentation::PathCurbsHydrated;
use openzt2_construction::topology::topology_presentation_types::TopologyPresentation;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_terrain::terrain::terrain_ground_path_surface_projection::GroundPathSurfaceImageDependencies;
use openzt2_terrain::terrain::terrain_ground_path_surface_projection::GroundPathSurfaceProjectionDirty;
use openzt2_terrain::terrain::terrain_ground_path_surface_projection::GroundPathSurfaceProjectionRevision;
use openzt2_ui::authored_tooltip_presentation::UiTooltipPolicy;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentationHydrated;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderRevision;
use openzt2_zoo_state::topology::fence_terrain_fitting_diagnostic_types::FenceTerrainFittingHistory;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;

/// Fence fitting history lives for one journey world; the fitter reads this
/// scope in Update, after the journey has advanced for the frame.
#[allow(clippy::needless_pass_by_value)]
pub(super) fn scope_fence_terrain_fitting_history_to_running_journey(
    session: Res<VerificationJourneySession>,
    mut history: ResMut<FenceTerrainFittingHistory>,
) {
    history.set_journey_scope(
        (!session.finished).then_some((session.journey_index, session.world_loads_started)),
    );
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
        info!(target: "openzt2_verification", stage, ?capture_name, lifetime = ?history.lifetime(),
            emitted = ?history.emitted_transitions(),
            omitted = ?history.omitted_transitions(),
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
