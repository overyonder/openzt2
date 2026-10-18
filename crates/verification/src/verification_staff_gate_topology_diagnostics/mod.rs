use bevy::{camera::RenderTarget, diagnostic::FrameCount, ecs::system::SystemParam, prelude::*};
use openzt2_game_data::AssetId;

use openzt2_construction::topology::fence_anchor_press_diagnostic_types::InitialFenceAnchorPress;
use openzt2_construction::topology::fence_anchor_press_diagnostic_types::InitialFenceAnchorPressDiagnosticState;
use openzt2_construction::topology::fence_terrain_endpoint_skew_presentation::fit_fence_presentations_to_terrain_endpoint_heights;
use openzt2_construction::topology::topology_construction_interaction::drive_selected_topology_placement_pointer_sequence;
use openzt2_construction::topology::topology_construction_preview_types::FencePreview;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_interface_state::input::input_types::PrimaryPointerInputState;
use openzt2_player_controls::camera::systems::pointer_ray_projection::project_pointer_ray;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailed;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitted;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::topology::fence_terrain_fitting_diagnostic_types::FenceEndpointFittingReports;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditAcknowledged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
use openzt2_zoo_state::topology::topology_graph_types::EdgeKey;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;

use super::verification_journey_state_types::{
    VerificationJourneySession, VerificationSessionStage,
};
use super::verification_journey_types::{VerificationJourneyAction, VerificationPointerTarget};

const SEGMENT_SAMPLE_LIMIT: usize = 8;

#[derive(Resource, Default)]
struct StaffGateTopologyObservation {
    journey: Option<(usize, u64)>,
    current: Option<FenceRunObservation>,
    previous_endpoint: Option<AcceptedFenceEndpoint>,
    anchor_step: Option<usize>,
    hover_recorded: bool,
    pointer_ray: Option<AnchorPointerRayObservation>,
}

struct AcceptedFenceEndpoint {
    transaction: Entity,
    root: Entity,
    edge: Entity,
    edge_id: PersistentId,
    definition: AssetId,
    node: Entity,
    node_id: PersistentId,
    cell: IVec3,
    // One fitting observation, never a replacement terrain or presentation owner.
    fitted_surface: Option<(Option<u32>, TopologyGrid, Vec3)>,
    fitting_status: &'static str,
}

struct AnchorPointerRayObservation {
    frame: Option<u32>,
    screen: Vec2,
    camera: Option<Entity>,
    ray: Option<Ray3d>,
}

struct FenceRunObservation {
    transaction: Entity,
    preview: Entity,
    root: Entity,
    acknowledged: bool,
    changed: bool,
    committed: bool,
    failed: bool,
    boundary_observed: bool,
    restart_observed: bool,
}

#[derive(SystemParam)]
struct FenceRunRecords<'w, 's> {
    edits: Query<'w, 's, &'static TopologyEdit>,
    nodes: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static TopologyNode,
            &'static WorldMember,
        ),
    >,
    edges: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static FenceEdge,
            Option<&'static Gate>,
            &'static WorldMember,
        ),
    >,
    index: Res<'w, TopologyIndex>,
    roots: Query<'w, 's, Entity, With<WorldRoot>>,
    regions: Query<'w, 's, (Entity, &'static HabitatRegion, &'static WorldMember)>,
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
}

pub(super) fn register_staff_gate_topology_diagnostics(app: &mut App) {
    app.init_resource::<StaffGateTopologyObservation>()
        .init_resource::<InitialFenceAnchorPressDiagnosticState>()
        .init_resource::<FenceEndpointFittingReports>()
        .add_systems(
            Update,
            observe_initial_fence_anchor_press
                .after(drive_selected_topology_placement_pointer_sequence)
                .in_set(GameSet::Intent),
        )
        .add_systems(
            Update,
            observe_fence_endpoint_fitting_reports
                .after(fit_fence_presentations_to_terrain_endpoint_heights)
                .in_set(GameSet::Presentation),
        )
        .add_systems(
            PostUpdate,
            observe_anchor_hover_pointer_ray_projection
                .after(project_pointer_ray)
                .in_set(GameSet::Presentation),
        )
        .add_systems(
            First,
            clear_inactive_staff_gate_topology_observation
                .after(super::verification_journey_execution::advance_verification_journey_by_one_frame),
        )
        .add_systems(
            OnExit(GamePhase::InGame),
            clear_staff_gate_topology_observation,
        )
        .add_systems(
            FixedUpdate,
            observe_requested_fence_run
                .after(openzt2_construction::construction::construction_transaction_preparation::begin_construction_transaction_from_valid_preview)
                .before(FixedConstructionTransactionMaterializationSet)
                .in_set(FixedConstructionSet::Prepare)
                .run_if(observing_staff_gate_journey),
        )
        .add_systems(
            FixedUpdate,
            observe_prepared_fence_run
                .after(FixedConstructionPreparedDomainEditMaterializationSet)
                .before(openzt2_construction::construction::construction_domain_preparation_collection::collect_construction_domain_preparation_results)
                .in_set(FixedConstructionSet::Prepare)
                .run_if(observing_staff_gate_journey),
        )
        .add_systems(
            FixedPostUpdate,
            observe_fence_run_publication.run_if(observing_staff_gate_journey),
        )
        // Construction applies after habitat cleanup. Its change is rebuilt on
        // the next running fixed tick; share cleanup's guards and flush its
        // commands before borrowing the regions.
        .add_systems(
            FixedUpdate,
            (ApplyDeferred, observe_published_fence_run_boundaries)
                .chain()
                .after(FixedGameSet::Cleanup)
                .before(FixedConstructionSet::Prepare)
                .run_if(in_state(GamePhase::InGame))
                .run_if(openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running)
                .run_if(observing_staff_gate_journey),
        )
        .add_systems(
            Update,
            observe_fence_preview_after_commit
                .after(GameSet::Intent)
                .before(GameSet::Ui)
                .run_if(observing_staff_gate_journey),
        );
}

fn observing_staff_gate_journey(session: Res<VerificationJourneySession>) -> bool {
    session.stage == VerificationSessionStage::Journey
        && session
            .journeys
            .get(session.journey_index)
            .is_some_and(|journey| journey.name == "populated-zoo")
}

fn clear_staff_gate_topology_observation(mut observation: ResMut<StaffGateTopologyObservation>) {
    *observation = default();
}

fn clear_inactive_staff_gate_topology_observation(
    session: Res<VerificationJourneySession>,
    tool: Res<ConstructionTool>,
    roots: Query<(), With<WorldRoot>>,
    transactions: Query<(), With<EditTransaction>>,
    records: FenceRunRecords,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    let journey = (session.stage == VerificationSessionStage::Journey
        && session
            .journeys
            .get(session.journey_index)
            .is_some_and(|journey| journey.name == "populated-zoo"))
    .then(|| {
        session
            .journey_started_at
            .map(|(frame, _)| (session.journey_index, frame))
    })
    .flatten();
    if journey.is_none()
        || observation.journey != journey
        || observation
            .current
            .as_ref()
            .is_some_and(|run| !roots.contains(run.root))
    {
        *observation = StaffGateTopologyObservation::default();
        observation.journey = journey;
        return;
    }
    if let Some(run) = observation
        .current
        .as_ref()
        .filter(|run| !transactions.contains(run.transaction))
    {
        info!(target: "openzt2_verification", transaction = ?run.transaction,
            "staff gate topology observed transaction no longer available");
        observation.current = None;
    }
    if observation
        .previous_endpoint
        .as_ref()
        .is_some_and(|endpoint| {
            !roots.contains(endpoint.root)
                || !records
                    .edges
                    .get(endpoint.edge)
                    .is_ok_and(|(id, edge, _, member)| {
                        *id == endpoint.edge_id
                            && edge.definition == endpoint.definition
                            && edge.b == endpoint.node
                            && member.root == endpoint.root
                    })
                || !records
                    .nodes
                    .get(endpoint.node)
                    .is_ok_and(|(id, node, member)| {
                        *id == endpoint.node_id
                            && node.cell == endpoint.cell
                            && member.root == endpoint.root
                    })
        })
    {
        info!(target: "openzt2_verification", "staff gate preceding accepted endpoint no longer available");
        observation.previous_endpoint = None;
    }
    let anchor_step = observation.previous_endpoint.as_ref().and_then(|endpoint| {
        (matches!(*tool, ConstructionTool::Fence(definition) if definition != endpoint.definition)
            && session.step_resolved_frame.is_some()
            && session.steps.get(session.step_index).is_some_and(|step| {
                matches!(
                    step.action,
                    VerificationJourneyAction::Click(VerificationPointerTarget::Position(_))
                )
            }))
        .then_some(session.step_index)
    });
    if observation.anchor_step != anchor_step {
        if observation.anchor_step.is_some() {
            info!(target: "openzt2_verification", step = ?observation.anchor_step,
                "staff gate replacement anchor observation ended without consumed initial press");
            observation.previous_endpoint = None;
        }
        observation.anchor_step = anchor_step;
        observation.hover_recorded = false;
        observation.pointer_ray = None;
    }
    observation.journey = journey;
}

#[allow(clippy::too_many_arguments)]
fn observe_requested_fence_run(
    session: Res<VerificationJourneySession>,
    mut requests: MessageReader<PrepareConstruction>,
    previews: Query<(&FencePreview, &ConstructionPreview, &WorldMember)>,
    grid: Res<TopologyGrid>,
    pointer: Res<PrimaryPointerInputState>,
    ray: Res<WorldPointerRay>,
    cursors: Query<&ConstructionCursor>,
    identities: Query<&SelectedWorldIdentity>,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    for request in requests.read() {
        let Ok((fence, construction, member)) = previews.get(request.preview) else {
            continue;
        };
        observation.previous_endpoint = None;
        observation.anchor_step = None;
        observation.pointer_ray = None;
        if let Some(previous) = observation.current.take() {
            info!(target: "openzt2_verification", transaction = ?previous.transaction,
                replacement = ?request.transaction, "staff gate topology observation replaced");
        }
        observation.current = Some(FenceRunObservation {
            transaction: request.transaction,
            preview: request.preview,
            root: member.root,
            acknowledged: false,
            changed: false,
            committed: false,
            failed: false,
            boundary_observed: false,
            restart_observed: false,
        });
        info!(target: "openzt2_verification", transaction = ?request.transaction,
            preview = ?request.preview, definition = ?fence.definition, root = ?member.root,
            from = ?fence.from, to = ?fence.to, validity = ?construction.validity,
            grid_origin = ?grid.origin, grid_spacing_m = grid.spacing_m,
            "staff gate topology requested run before preparation");
        // These live inputs are context at the observation barrier, not an
        // atomic copy of the input/ray/hit which originally submitted the run.
        info!(target: "openzt2_verification", transaction = ?request.transaction,
            journey_step_context = session.step_index, journey_frame_context = session.frames_run,
            pointer_context = ?*pointer, ray_context = ?ray.0,
            cursor_context = ?cursors.single().ok(), world_context = ?identities.get(member.root).ok(),
            "staff gate topology non-atomic request context");
    }
}

fn observe_prepared_fence_run(
    mut prepared: MessageReader<TopologyEditPrepared>,
    mut rejected: MessageReader<TopologyEditPreparationRejected>,
    records: FenceRunRecords,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    for message in prepared.read() {
        let Some(run) = observation
            .current
            .as_ref()
            .filter(|run| run.transaction == message.transaction)
        else {
            continue;
        };
        let Ok(edit) = records.edits.get(run.transaction) else {
            warn!(target: "openzt2_verification", transaction = ?run.transaction,
                "staff gate topology prepared message has unavailable edit");
            continue;
        };
        let segments = edit
            .created
            .iter()
            .filter(|snapshot| matches!(snapshot, TopologySnapshot::Fence { .. }));
        let total = segments.clone().count();
        let gates = segments
            .clone()
            .filter(|snapshot| matches!(snapshot, TopologySnapshot::Fence { gate: Some(_), .. }))
            .count();
        let first = segments.clone().next().and_then(|snapshot| match snapshot {
            TopologySnapshot::Fence { a, .. } => records.snapshot_node_cell(edit, *a, run.root),
            _ => None,
        });
        let last = segments
            .clone()
            .next_back()
            .and_then(|snapshot| match snapshot {
                TopologySnapshot::Fence { b, .. } => records.snapshot_node_cell(edit, *b, run.root),
                _ => None,
            });
        info!(target: "openzt2_verification", transaction = ?run.transaction,
            cost = ?message.cost, created_snapshots = edit.created.len(), removed_snapshots = edit.removed.len(),
            segments = total, gate_segments = gates, first_cell = ?first, last_cell = ?last,
            sampled_segments = total.min(SEGMENT_SAMPLE_LIMIT), omitted_segments = total.saturating_sub(SEGMENT_SAMPLE_LIMIT),
            "staff gate topology actual prepared run");
        for (ordinal, snapshot) in segments.take(SEGMENT_SAMPLE_LIMIT).enumerate() {
            if let TopologySnapshot::Fence {
                id,
                definition,
                a,
                b,
                gate,
            } = snapshot
            {
                info!(target: "openzt2_verification", transaction = ?run.transaction,
                    ordinal, id = ?id, definition = ?definition, a = ?a, b = ?b, gate = ?gate,
                    from = ?records.snapshot_node_cell(edit, *a, run.root),
                    to = ?records.snapshot_node_cell(edit, *b, run.root),
                    "staff gate topology prepared segment sample");
            }
        }
    }
    for message in rejected.read() {
        if let Some(run) = observation
            .current
            .as_mut()
            .filter(|run| run.transaction == message.transaction)
        {
            run.failed = true;
            info!(target: "openzt2_verification", transaction = ?message.transaction,
                reason = ?message.reason, "staff gate topology preparation rejected");
        }
    }
}

fn observe_fence_run_publication(
    mut acknowledged: MessageReader<TopologyEditAcknowledged>,
    mut changes: MessageReader<TopologyChanged>,
    mut committed: MessageReader<ConstructionCommitted>,
    mut failed: MessageReader<ConstructionCommitFailed>,
    records: FenceRunRecords,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    for message in acknowledged.read() {
        if let Some(run) = observation
            .current
            .as_mut()
            .filter(|run| run.transaction == message.transaction)
        {
            info!(target: "openzt2_verification", transaction = ?message.transaction,
                application = ?message.application, accepted = message.accepted,
                "staff gate topology application acknowledgement");
            if message.application == EditApplication::InitialCommit {
                run.acknowledged = message.accepted;
            }
        }
    }
    for message in changes.read() {
        if let Some(run) = observation
            .current
            .as_mut()
            .filter(|run| Some(run.transaction) == message.transaction)
        {
            run.changed = true;
            info!(target: "openzt2_verification", transaction = ?run.transaction,
                bounds = ?message.bounds, "staff gate topology publication change awaiting next habitat cleanup");
        }
    }
    for message in committed.read() {
        if let Some(run) = observation
            .current
            .as_mut()
            .filter(|run| run.transaction == message.transaction)
        {
            run.committed = true;
            info!(target: "openzt2_verification", transaction = ?message.transaction,
                cost = ?message.cost, screen_position = ?message.screen_position,
                "staff gate topology construction committed");
            let root = run.root;
            observation.remember_accepted_endpoint(&records, message.transaction, root);
        }
    }
    for message in failed.read() {
        if let Some(run) = observation
            .current
            .as_mut()
            .filter(|run| run.transaction == message.transaction)
        {
            run.failed = true;
            info!(target: "openzt2_verification", transaction = ?message.transaction,
                reason = ?message.reason, "staff gate topology construction failed");
        }
    }
    if observation.current.as_ref().is_some_and(|run| run.failed) {
        observation.current = None;
        observation.previous_endpoint = None;
        observation.anchor_step = None;
        observation.pointer_ray = None;
    }
}

impl StaffGateTopologyObservation {
    fn remember_accepted_endpoint(
        &mut self,
        records: &FenceRunRecords,
        transaction: Entity,
        root: Entity,
    ) {
        self.previous_endpoint = None;
        let endpoint = records.edits.get(transaction).ok().and_then(|edit| {
            let (id, definition, a, b) =
                edit.created
                    .iter()
                    .rev()
                    .find_map(|snapshot| match *snapshot {
                        TopologySnapshot::Fence {
                            id,
                            definition,
                            a,
                            b,
                            ..
                        } => Some((id, definition, a, b)),
                        _ => None,
                    })?;
            let from = records.snapshot_node_cell(edit, a, root)?;
            let cell = records.snapshot_node_cell(edit, b, root)?;
            let edge = *records.index.edges.get(&EdgeKey::new(from, cell)?)?;
            let (live_id, live_edge, _, member) = records.edges.get(edge).ok()?;
            let (node_id, node, node_member) = records.nodes.get(live_edge.b).ok()?;
            (*live_id == id
                && live_edge.definition == definition
                && member.root == root
                && *node_id == b
                && node.cell == cell
                && node_member.root == root)
                .then_some(AcceptedFenceEndpoint {
                    transaction,
                    root,
                    edge,
                    edge_id: id,
                    definition,
                    node: live_edge.b,
                    node_id: b,
                    cell,
                    fitted_surface: None,
                    fitting_status: "fitting not observed since acceptance",
                })
        });
        info!(target: "openzt2_verification", ?transaction, ?root,
            edge = ?endpoint.as_ref().map(|endpoint| endpoint.edge),
            edge_id = ?endpoint.as_ref().map(|endpoint| endpoint.edge_id),
            node = ?endpoint.as_ref().map(|endpoint| endpoint.node),
            node_id = ?endpoint.as_ref().map(|endpoint| endpoint.node_id),
            cell = ?endpoint.as_ref().map(|endpoint| endpoint.cell),
            available = endpoint.is_some(), "staff gate preceding accepted endpoint reference");
        self.previous_endpoint = endpoint;
    }

    fn record_endpoint_fitting(
        &mut self,
        edge: Entity,
        [first_cell, second_cell]: [IVec3; 2],
        grid: TopologyGrid,
        frame: Option<u32>,
        surfaces: Option<(Vec3, Vec3)>,
        status: &'static str,
    ) {
        let Some(endpoint) = self
            .previous_endpoint
            .as_mut()
            .filter(|endpoint| endpoint.edge == edge)
        else {
            return;
        };
        endpoint.fitted_surface = surfaces
            .and_then(|(first, second)| {
                if first_cell == endpoint.cell {
                    Some(first)
                } else if second_cell == endpoint.cell {
                    Some(second)
                } else {
                    None
                }
            })
            .filter(|surface| {
                surface.is_finite() && surface.xz() == grid.cell_translation(endpoint.cell).xz()
            })
            .map(|surface| (frame, grid, surface));
        endpoint.fitting_status = if surfaces.is_some() && endpoint.fitted_surface.is_none() {
            "fitted endpoint identity or position mismatch"
        } else {
            status
        };
    }

    fn record_anchor_hover_projection(
        &mut self,
        frame: Option<u32>,
        pointer: &PrimaryPointerInputState,
        ray: Option<Ray3d>,
        camera: Option<(
            Entity,
            &Camera,
            &GlobalTransform,
            Option<&WorldMember>,
            Option<&RenderTarget>,
        )>,
    ) {
        if self.anchor_step.is_none() {
            return;
        }
        self.pointer_ray = Some(AnchorPointerRayObservation {
            frame,
            screen: pointer.screen,
            camera: camera.map(|(entity, ..)| entity),
            ray,
        });
        if self.hover_recorded {
            return;
        }
        self.hover_recorded = true;
        let Some(endpoint) = self.previous_endpoint.as_ref() else {
            return;
        };
        info!(target: "openzt2_verification", step = ?self.anchor_step, ?frame,
            transaction = ?endpoint.transaction, root = ?endpoint.root,
            edge = ?endpoint.edge, edge_id = ?endpoint.edge_id, definition = ?endpoint.definition,
            node = ?endpoint.node, node_id = ?endpoint.node_id, cell = ?endpoint.cell,
            fitted_surface = ?endpoint.fitted_surface, fitting_status = endpoint.fitting_status,
            camera_available = camera.is_some(), pointer = ?pointer, ?ray,
            "staff gate replacement anchor hover; fitting and projection observations are not atomic with pixels or consumed input");
        let Some((entity, camera, transform, member, target)) = camera else {
            info!(target: "openzt2_verification", ?frame, "staff gate replacement anchor projection unavailable: ZooCamera missing or ambiguous");
            return;
        };
        let scale = camera.target_scaling_factor();
        let viewport = camera.logical_viewport_rect();
        let projected = endpoint
            .fitted_surface
            .map(|(_, _, surface)| camera.world_to_viewport(transform, surface));
        let screen = projected
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .zip(scale.filter(|scale| scale.is_finite() && *scale > 0.0))
            .map(|(position, scale)| *position * scale)
            .filter(|screen| screen.is_finite());
        let same_root = member.is_some_and(|member| member.root == endpoint.root);
        info!(target: "openzt2_verification", ?frame, camera = ?entity,
            camera_root = ?member.map(|member| member.root), same_root,
            world_from_view = ?transform.affine(), clip_from_view = ?camera.clip_from_view(),
            ?target, ?scale, ?viewport, ?projected, ?screen,
            within_viewport = ?projected.as_ref().and_then(|result| result.as_ref().ok()).zip(viewport).map(|(position, viewport)| viewport.contains(*position)),
            projection_available = same_root && target.is_some() && screen.is_some() && viewport.is_some(),
            "staff gate replacement anchor accepted endpoint projection");
    }
}

/// Topology reports the inputs it consumed for an initial fence anchor press;
/// read them with the pointer ray and frame of the same Intent pass.
#[allow(clippy::needless_pass_by_value)]
fn observe_initial_fence_anchor_press(
    mut presses: ResMut<InitialFenceAnchorPressDiagnosticState>,
    ray_resource: Option<Res<WorldPointerRay>>,
    frame: Option<Res<FrameCount>>,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    let Some(InitialFenceAnchorPress {
        root,
        definition,
        grid,
        pointer,
        cursor,
        ui_capture,
        cell,
    }) = presses.take_pending_press()
    else {
        return;
    };
    if observation.anchor_step.is_none() {
        return;
    }
    let Some(endpoint) = observation.previous_endpoint.as_ref() else {
        return;
    };
    let provenance = observation.pointer_ray.as_ref();
    let ray = ray_resource.as_ref().and_then(|ray| ray.0);
    info!(target: "openzt2_verification", frame = ?frame.as_ref().map(|frame| frame.0), step = ?observation.anchor_step,
        ?root, ?definition, ?grid, ?pointer, ?cursor, ?ui_capture, rounded_cell = ?cell,
        ray_resource_available = ray_resource.is_some(), consumed_ray_context = ?ray,
        preceding_transaction = ?endpoint.transaction, preceding_edge = ?endpoint.edge,
        preceding_edge_id = ?endpoint.edge_id, preceding_node = ?endpoint.node,
        preceding_node_id = ?endpoint.node_id, preceding_cell = ?endpoint.cell,
        same_root = root == endpoint.root, fitted_surface = ?endpoint.fitted_surface,
        ray_producer_frame = ?provenance.and_then(|provenance| provenance.frame),
        ray_producer_camera = ?provenance.and_then(|provenance| provenance.camera),
        ray_producer_screen = ?provenance.map(|provenance| provenance.screen),
        ray_matches_provenance = ?provenance.map(|provenance| ray_resource.is_some() && provenance.ray == ray),
        pointer_matches_provenance = ?provenance.map(|provenance| provenance.screen == pointer.screen),
        "staff gate ordinary initial anchor press consumed by topology; ray provenance does not establish cursor-hit or pixel atomicity");
    observation.previous_endpoint = None;
    observation.anchor_step = None;
    observation.pointer_ray = None;
}

/// Replays topology's endpoint fitting reports in fitting order, directly
/// after the fitting pass that produced them.
fn observe_fence_endpoint_fitting_reports(
    mut reports: ResMut<FenceEndpointFittingReports>,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    for report in reports.drain() {
        observation.record_endpoint_fitting(
            report.edge,
            report.endpoint_cells,
            report.grid,
            report.frame,
            report.surfaces,
            report.status,
        );
    }
}

/// Reads the pointer ray camera projects for this frame, with the camera and
/// pointer state it was projected from.
#[allow(clippy::needless_pass_by_value)]
fn observe_anchor_hover_pointer_ray_projection(
    pointer: Res<PrimaryPointerInputState>,
    cameras: Query<
        (
            Entity,
            &Camera,
            &GlobalTransform,
            Option<&WorldMember>,
            Option<&RenderTarget>,
        ),
        With<ZooCamera>,
    >,
    ray: Res<WorldPointerRay>,
    frame: Option<Res<FrameCount>>,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    observation.record_anchor_hover_projection(
        frame.as_ref().map(|frame| frame.0),
        &pointer,
        ray.0,
        cameras.single().ok(),
    );
}

impl FenceRunRecords<'_, '_> {
    fn snapshot_node_cell(
        &self,
        edit: &TopologyEdit,
        id: PersistentId,
        root: Entity,
    ) -> Option<IVec3> {
        edit.created
            .iter()
            .find_map(|snapshot| match snapshot {
                TopologySnapshot::Node {
                    id: candidate,
                    cell,
                } if *candidate == id => Some(*cell),
                _ => None,
            })
            .or_else(|| {
                self.index.nodes.values().find_map(|entity| {
                    self.nodes
                        .get(*entity)
                        .ok()
                        .and_then(|(candidate, node, member)| {
                            (*candidate == id && member.root == root).then_some(node.cell)
                        })
                })
            })
    }
}

fn observe_published_fence_run_boundaries(
    records: FenceRunRecords,
    mut habitat_changes: MessageReader<HabitatChanged>,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    let habitat_change_count = habitat_changes.read().count();
    let Some(run) = observation
        .current
        .as_mut()
        .filter(|run| run.changed && run.acknowledged && !run.boundary_observed)
    else {
        return;
    };
    if !records.roots.contains(run.root) {
        observation.current = None;
        return;
    }
    let Ok(edit) = records.edits.get(run.transaction) else {
        warn!(target: "openzt2_verification", transaction = ?run.transaction,
            "staff gate topology publication edit unavailable after habitat cleanup");
        observation.current = None;
        return;
    };
    let definitions = records.active_definitions.get(&records.definitions);
    let boundary_observation_available =
        definitions.is_some() && records.roots.single().ok() == Some(run.root);
    let mut total = 0;
    let mut published = 0_usize;
    let mut unavailable = 0_usize;
    let mut mismatched = 0_usize;
    let mut gates = 0_usize;
    let mut staff_gates = 0_usize;
    let mut unclassified_gates = 0_usize;
    let mut boundary_edges = 0_usize;
    let mut staff_boundary_edges = 0_usize;
    let mut first_published_cell = None;
    let mut last_published_cell = None;
    for snapshot in &edit.created {
        let TopologySnapshot::Fence {
            id,
            definition,
            a,
            b,
            ..
        } = snapshot
        else {
            continue;
        };
        let ordinal = total;
        total += 1;
        let cells = records
            .snapshot_node_cell(edit, *a, run.root)
            .zip(records.snapshot_node_cell(edit, *b, run.root));
        let entity = cells
            .and_then(|(from, to)| EdgeKey::new(from, to))
            .and_then(|key| records.index.edges.get(&key).copied());
        let live = entity.and_then(|entity| records.edges.get(entity).ok());
        let Some((live_id, edge, gate, member)) = live else {
            unavailable += 1;
            if ordinal < SEGMENT_SAMPLE_LIMIT {
                info!(target: "openzt2_verification", transaction = ?run.transaction,
                    ordinal, id = ?id, definition = ?definition, cells = ?cells, indexed_entity = ?entity,
                    "staff gate topology segment not published or no longer available");
            }
            continue;
        };
        let live_nodes = records
            .nodes
            .get(edge.a)
            .ok()
            .zip(records.nodes.get(edge.b).ok());
        let live_cells = live_nodes.map(|((_, a, _), (_, b, _))| (a.cell, b.cell));
        let endpoints_match =
            live_nodes.is_some_and(|((a_id, _, a_member), (b_id, _, b_member))| {
                *a_id == *a && *b_id == *b && a_member.root == run.root && b_member.root == run.root
            });
        if *live_id != *id
            || edge.definition != *definition
            || member.root != run.root
            || !endpoints_match
        {
            mismatched += 1;
            if ordinal < SEGMENT_SAMPLE_LIMIT {
                info!(target: "openzt2_verification", transaction = ?run.transaction,
                    ordinal, expected_id = ?id, live_id = ?live_id, indexed_entity = ?entity,
                    expected_definition = ?definition, live_edge = ?edge, live_root = ?member.root,
                    endpoints_match, "staff gate topology indexed segment identity mismatch");
            }
            continue;
        }
        published += 1;
        if let Some((from, to)) = live_cells {
            let _ = first_published_cell.get_or_insert(from);
            last_published_cell = Some(to);
        }
        gates += usize::from(gate.is_some());
        let staff_policy = definitions
            .and_then(|definitions| definitions.find_gate_fence(edge.definition))
            .map(|fence| {
                fence.gate_policy.open_for_entity_type == Some(AssetId::from_key("staff"))
            });
        let staff_gate = gate.is_some() && staff_policy == Some(true);
        staff_gates += usize::from(staff_gate);
        unclassified_gates += usize::from(gate.is_some() && staff_policy.is_none());
        let entity = entity.expect("queried indexed edge");
        let mut memberships = records.regions.iter().filter(|(_, region, member)| {
            boundary_observation_available
                && member.root == run.root
                && region.boundary_fence_entities.contains(&entity)
        });
        let first_habitat = memberships.next().map(|(entity, _, _)| entity);
        let membership_count = usize::from(first_habitat.is_some()) + memberships.count();
        boundary_edges += usize::from(membership_count != 0);
        staff_boundary_edges += usize::from(staff_gate && membership_count != 0);
        if ordinal < SEGMENT_SAMPLE_LIMIT {
            info!(target: "openzt2_verification", transaction = ?run.transaction,
                ordinal, entity = ?entity, id = ?live_id, definition = ?edge.definition,
                a_entity = ?edge.a, b_entity = ?edge.b, expected_cells = ?cells, live_cells = ?live_cells,
                gate = ?gate, staff_opener_eligible = ?staff_policy, first_habitat = ?first_habitat,
                membership_count = ?boundary_observation_available.then_some(membership_count),
                "staff gate topology published segment after habitat cleanup");
        }
    }
    info!(target: "openzt2_verification", transaction = ?run.transaction, root = ?run.root,
        total, published, unavailable, mismatched, gates, staff_gates, unclassified_gates,
        first_published_cell = ?first_published_cell, last_published_cell = ?last_published_cell,
        boundary_edges = ?boundary_observation_available.then_some(boundary_edges),
        staff_boundary_edges = ?boundary_observation_available.then_some(staff_boundary_edges),
        boundary_observation_available, definitions_available = definitions.is_some(),
        habitat_change_messages_context = habitat_change_count,
        sampled_segments = total.min(SEGMENT_SAMPLE_LIMIT), omitted_segments = total.saturating_sub(SEGMENT_SAMPLE_LIMIT),
        "staff gate topology run membership after next habitat cleanup");
    run.boundary_observed = true;
    if run.restart_observed {
        observation.current = None;
    }
}

fn observe_fence_preview_after_commit(
    previews: Query<(&FencePreview, &ConstructionPreview)>,
    mut observation: ResMut<StaffGateTopologyObservation>,
) {
    let Some(run) = observation
        .current
        .as_mut()
        .filter(|run| run.committed && !run.restart_observed)
    else {
        return;
    };
    match previews.get(run.preview) {
        Ok((fence, construction)) => {
            info!(target: "openzt2_verification", transaction = ?run.transaction,
                preview = ?run.preview, definition = ?fence.definition,
                from = ?fence.from, to = ?fence.to, empty_anchor = fence.from == fence.to,
                validity = ?construction.validity,
                "staff gate topology post-commit preview after intent, not submitted endpoints");
        }
        Err(_) => {
            info!(target: "openzt2_verification", transaction = ?run.transaction,
                preview = ?run.preview, "staff gate topology post-commit preview unavailable");
        }
    }
    run.restart_observed = true;
    if run.boundary_observed {
        observation.current = None;
    }
}
