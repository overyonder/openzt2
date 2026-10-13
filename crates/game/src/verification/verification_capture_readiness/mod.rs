use std::{
    any::TypeId,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use bevy::{
    asset::AssetId,
    camera::{visibility::VisibleEntities, RenderTarget},
    color::LinearRgba,
    pbr::{PreparedMaterial, RenderMaterialInstances},
    prelude::*,
    render::{
        erased_render_asset::ErasedRenderAssets,
        mesh::RenderMesh,
        render_asset::RenderAssets,
        render_resource::{CachedPipelineState, PipelineCache, PipelineDescriptor},
        sync_world::MainEntity,
        texture::GpuImage,
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
    },
    sprite::BorderRect,
    text::TextLayoutInfo,
    ui::ResolvedBorderRadius,
    ui_render::{ExtractedUiItem, ExtractedUiNodes, NodeType},
};

use crate::plugins::{
    construction::construction_interaction_types::ConstructionPreview,
    placement::placement_preview_types::{
        ObjectPlacementPreviewDecal, ObjectPlacementPreviewOwner,
    },
    ui::animation::UiShowHideAnimation,
    world_spawn::prefab_presentation_types::PrefabModel,
};

use super::{
    verification_capture_execution::{CAPTURE_READY_FRAME_LIMIT, CAPTURE_READY_WALL_LIMIT},
    verification_journey_state_types::{
        VerificationCaptureProgress, VerificationJourneySession, VerificationSessionStage,
    },
    verification_journey_types::{VerificationCaptureMode, VerificationJourneyAction},
};

pub(crate) mod world_presentation_diagnostics;

#[derive(Resource)]
pub(crate) struct VerificationCaptureReadiness {
    mode: VerificationCaptureMode,
    step: Option<(usize, VerificationSessionStage, usize)>,
    stable_since: u64,
    // Retain the former observation only to diagnose discarded layout churn.
    node_layout: Vec<(Entity, Vec2, UiGlobalTransform)>,
    mesh_entities: Vec<(Entity, AssetId<Mesh>)>,
    placement_decals: Vec<VerificationPlacementDecalDraw>,
    reported_node_change: bool,
    reported_pending_blockers: bool,
    deadline_observed: bool,
    request: VerificationCaptureRenderRequest,
}

#[derive(Resource, Clone, Default)]
struct VerificationCaptureRenderRequest {
    capture: String,
    step: Option<(usize, VerificationSessionStage, usize)>,
    frame: u64,
    ui_ready: bool,
    log_timeout_blockers: bool,
    images: Vec<AssetId<Image>>,
    meshes: Vec<(Entity, AssetId<Mesh>)>,
    node_names: Vec<(Entity, String)>,
    submitted_frame: Arc<AtomicU64>,
}

#[derive(Debug, PartialEq)]
struct VerificationPlacementDecalDraw {
    owner: Entity,
    mesh: AssetId<Mesh>,
    material: AssetId<StandardMaterial>,
    world_from_local: [f32; 16],
}

#[derive(Debug)]
struct VerificationUiDrawLayout {
    entity: Entity,
    camera: Entity,
    z_order: f32,
    image: AssetId<Image>,
    corners: [Vec2; 4],
    clip: Option<Rect>,
    rect: Rect,
    color: LinearRgba,
    node_style: Option<VerificationUiNodeDrawStyle>,
}

#[derive(Debug, PartialEq)]
struct VerificationUiNodeDrawStyle {
    atlas_scaling: Option<Vec2>,
    flip_x: bool,
    flip_y: bool,
    border_radius: ResolvedBorderRadius,
    border: BorderRect,
    node_type: NodeType,
}

#[derive(Resource, Default)]
struct VerificationCaptureDrawReadiness {
    layout: Vec<VerificationUiDrawLayout>,
    stable_since: u64,
}

impl VerificationCaptureReadiness {
    pub(super) fn uses_presentation_readiness(&self) -> bool {
        self.mode == VerificationCaptureMode::Ready
    }

    pub(super) fn preparation_deadline_observed(
        &self,
        session: &VerificationJourneySession,
    ) -> bool {
        self.deadline_observed
            && self.step == Some((session.journey_index, session.stage, session.step_index))
    }

    pub(super) fn is_ready(&self, step_frame: u32) -> bool {
        if self.mode == VerificationCaptureMode::Full {
            return step_frame >= 30;
        }
        self.request.ui_ready
            // Draw stability is checked in the render world before acknowledging
            // submission. The frame comparison also covers pipelined extraction.
            && self.request.frame > self.stable_since
            && self.request.submitted_frame.load(Ordering::Acquire) > self.stable_since
    }
}

pub(super) fn install_verification_capture_readiness(app: &mut App, mode: VerificationCaptureMode) {
    info!(target: "openzt2_verification", ?mode, "verification capture mode");
    app.init_resource::<world_presentation_diagnostics::FenceTerrainFittingHistory>();
    app.insert_resource(VerificationCaptureReadiness {
        mode,
        step: None,
        stable_since: 0,
        node_layout: Vec::new(),
        mesh_entities: Vec::new(),
        placement_decals: Vec::new(),
        reported_node_change: false,
        reported_pending_blockers: false,
        deadline_observed: false,
        request: default(),
    });
    app.add_systems(
        Last,
        world_presentation_diagnostics::log_capture_world_presentation_dependencies,
    );
    if mode == VerificationCaptureMode::Full {
        return;
    }
    app.add_systems(Last, observe_capture_presentation_readiness);
    app.sub_app_mut(RenderApp)
        .init_resource::<VerificationCaptureRenderRequest>()
        .init_resource::<VerificationCaptureDrawReadiness>()
        .add_systems(ExtractSchedule, extract_capture_render_request)
        .add_systems(
            Render,
            observe_extracted_capture_draw_layout.before(RenderSystems::PrepareBindGroups),
        )
        .add_systems(
            Render,
            acknowledge_prepared_capture_frame.after(RenderSystems::Render),
        );
}

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn observe_capture_presentation_readiness(
    session: Res<VerificationJourneySession>,
    mut readiness: ResMut<VerificationCaptureReadiness>,
    animations: Query<(Entity, &UiShowHideAnimation, &InheritedVisibility)>,
    nodes: Query<(
        Entity,
        &ComputedNode,
        &UiGlobalTransform,
        &InheritedVisibility,
        Option<&Name>,
    )>,
    images: Query<(&ImageNode, &InheritedVisibility)>,
    texts: Query<(&TextLayoutInfo, &InheritedVisibility)>,
    meshes: Query<(Entity, &Mesh3d, &ViewVisibility)>,
    models: Query<(Entity, &PrefabModel, &InheritedVisibility)>,
    names: Query<(Entity, &Name)>,
    parents: Query<&ChildOf>,
    cameras: Query<(Entity, &Camera, &RenderTarget, &VisibleEntities)>,
    server: Res<AssetServer>,
    decals: Query<
        (
            &ObjectPlacementPreviewOwner,
            &MeshMaterial3d<StandardMaterial>,
            &GlobalTransform,
        ),
        With<ObjectPlacementPreviewDecal>,
    >,
    preview_owners: Query<(), With<ConstructionPreview>>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut material_events: MessageReader<AssetEvent<StandardMaterial>>,
) {
    let capture_name = session
        .steps
        .get(session.step_index)
        .and_then(|step| match &step.action {
            VerificationJourneyAction::Capture(name) => Some(name.as_str()),
            _ => None,
        });
    if capture_name.is_none()
        || !matches!(
            session.step_capture,
            VerificationCaptureProgress::Preparing { .. }
        )
    {
        readiness.step = None;
        readiness.request.ui_ready = false;
        readiness.request.log_timeout_blockers = false;
        readiness.request.node_names.clear();
        readiness.deadline_observed = false;
        mesh_events.clear();
        material_events.clear();
        return;
    }
    let step = (session.journey_index, session.stage, session.step_index);
    if readiness.step != Some(step) {
        readiness.node_layout.clear();
        readiness.reported_node_change = false;
        readiness.reported_pending_blockers = false;
        readiness.deadline_observed = false;
        readiness.request.capture = format!(
            "{}/{}",
            session.journeys[session.journey_index].name,
            capture_name.unwrap_or_default()
        );
        readiness.request.step = Some(step);
    }
    let mut node_layout = nodes
        .iter()
        .filter(|(_, _, _, visibility, _)| visibility.get())
        .map(|(entity, node, transform, _, _)| (entity, node.size(), *transform))
        .collect::<Vec<_>>();
    node_layout.sort_unstable_by_key(|(entity, ..)| *entity);
    let mut required_images = images
        .iter()
        .filter(|(_, visibility)| visibility.get())
        .map(|(image, _)| image.image.id())
        .chain(
            texts
                .iter()
                .filter(|(_, visibility)| visibility.get())
                .flat_map(|(text, _)| text.glyphs.iter().map(|glyph| glyph.atlas_info.texture)),
        )
        .collect::<Vec<_>>();
    required_images.sort_unstable();
    required_images.dedup();
    let mut required_meshes = meshes
        .iter()
        .filter(|(_, _, visibility)| visibility.get())
        .map(|(entity, mesh, _)| (entity, mesh.0.id()))
        .collect::<Vec<_>>();
    required_meshes.sort_unstable_by_key(|(entity, _)| *entity);
    let mut mesh_entities = Vec::new();
    let mut placement_decals = Vec::new();
    for (entity, mesh) in &required_meshes {
        if let Ok((owner, material, transform)) = decals.get(*entity) {
            if preview_owners.contains(owner.0)
                && parents
                    .get(*entity)
                    .is_ok_and(|parent| parent.parent() == owner.0)
            {
                // This producer rebuilds footprint/grid children using one cached
                // plane. Compare their actual draws, retaining count and geometry.
                placement_decals.push(VerificationPlacementDecalDraw {
                    owner: owner.0,
                    mesh: *mesh,
                    material: material.0.id(),
                    world_from_local: transform.to_matrix().to_cols_array(),
                });
                continue;
            }
        }
        mesh_entities.push((*entity, *mesh));
    }
    placement_decals.sort_unstable_by(|previous, current| {
        previous
            .owner
            .cmp(&current.owner)
            .then(previous.mesh.cmp(&current.mesh))
            .then(previous.material.cmp(&current.material))
            .then_with(|| {
                previous
                    .world_from_local
                    .iter()
                    .zip(&current.world_from_local)
                    .map(|(previous, current)| previous.total_cmp(current))
                    .find(|order| *order != std::cmp::Ordering::Equal)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    // Asset IDs alone do not distinguish an in-place geometry or material edit.
    // Drain all events, including those unrelated to these normalized draws.
    let decal_mesh_changed = mesh_events
        .read()
        .fold(false, |changed, event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => {
                changed | placement_decals.iter().any(|draw| draw.mesh == *id)
            }
            AssetEvent::Unused { .. } => changed,
        });
    let decal_material_changed = material_events
        .read()
        .fold(false, |changed, event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => {
                changed | placement_decals.iter().any(|draw| draw.material == *id)
            }
            AssetEvent::Unused { .. } => changed,
        });
    let ui_ready = !animations
        .iter()
        .any(|(_, animation, visibility)| visibility.get() && animation.running)
        && models
            .iter()
            .filter(|(_, _, visibility)| visibility.get())
            .all(|(_, model, _)| {
                model
                    .handle
                    .as_ref()
                    .is_some_and(|handle| server.is_loaded_with_dependencies(handle.id()))
            });
    let images_changed = readiness.request.images != required_images;
    let mesh_membership_changed = readiness.request.meshes != required_meshes;
    let meshes_changed = readiness.mesh_entities != mesh_entities
        || readiness.placement_decals != placement_decals
        || decal_mesh_changed
        || decal_material_changed;
    if readiness.step != Some(step) || images_changed || meshes_changed || !ui_ready {
        readiness.stable_since = session.frames_run;
    }
    readiness.step = Some(step);
    readiness.request.frame = session.frames_run;
    readiness.request.ui_ready = ui_ready;
    readiness.request.images = required_images;
    let elapsed = match session.step_capture {
        VerificationCaptureProgress::Preparing { started_at } => started_at.elapsed(),
        _ => Duration::ZERO,
    };
    let deadline_reached = elapsed >= CAPTURE_READY_WALL_LIMIT;
    // A pending checkpoint reveals blockers well before slow rendering can reach
    // the frame limit. Extract the final request before First terminates the step.
    readiness.request.log_timeout_blockers = session.step_frame == CAPTURE_READY_FRAME_LIMIT
        || (elapsed >= Duration::from_secs(5) && !readiness.reported_pending_blockers)
        || (deadline_reached && !readiness.deadline_observed);
    readiness.request.node_names.clear();
    let changed_node = readiness
        .node_layout
        .iter()
        .zip(&node_layout)
        .find(|(previous, current)| previous != current);
    if readiness.request.log_timeout_blockers
        || (!readiness.reported_node_change
            && changed_node.is_some()
            && ui_ready
            && readiness.request.frame > readiness.stable_since)
    {
        let changed_name = changed_node
            .and_then(|(_, current)| nodes.get(current.0).ok())
            .and_then(|(_, _, _, _, name)| name);
        info!(target: "openzt2_verification", capture = %readiness.request.capture, step = ?step,
            frame = readiness.request.frame,
            previous_nodes = readiness.node_layout.len(), current_nodes = node_layout.len(),
            ?changed_name, ?changed_node, "capture readiness raw node layout change");
        readiness.reported_node_change = true;
    }
    readiness.node_layout = node_layout;
    if readiness.request.log_timeout_blockers {
        if mesh_membership_changed {
            // Membership is the union of camera and shadow visibility, not an
            // asset change notification. Diagnose that delta before replacing it.
            let added = required_meshes
                .iter()
                .filter(|entry| readiness.request.meshes.binary_search(entry).is_err())
                .collect::<Vec<_>>();
            let removed = readiness
                .request
                .meshes
                .iter()
                .filter(|entry| required_meshes.binary_search(entry).is_err())
                .collect::<Vec<_>>();
            info!(target: "openzt2_verification", capture = %readiness.request.capture,
                frame = readiness.request.frame,
                previous_meshes = readiness.request.meshes.len(), current_meshes = required_meshes.len(),
                capture_target = ?session.capture_target,
                added_or_replaced = added.len(), removed_or_replaced = removed.len(),
                removed_entities_still_present = removed.iter().filter(|(entity, _)| meshes.contains(*entity)).count(),
                "capture readiness mesh membership delta");
            // Limit output to four samples per direction at existing checkpoints.
            for (change, entries) in [
                ("added_or_replaced", added),
                ("removed_or_replaced", removed),
            ] {
                for (entity, mesh_asset) in entries.into_iter().take(4) {
                    let current_mesh = meshes.get(*entity).ok();
                    let parent = parents.get(*entity).ok().map(ChildOf::parent);
                    let parent_name = parent
                        .and_then(|parent| names.get(parent).ok().map(|(_, name)| name.as_str()));
                    let model_owner = std::iter::once(*entity)
                        .chain(parents.iter_ancestors(*entity))
                        .find_map(|ancestor| models.get(ancestor).ok())
                        .map(|(owner, model, _)| {
                            (
                                owner,
                                names.get(owner).ok().map(|(_, name)| name.as_str()),
                                model.model_path.as_ref(),
                            )
                        });
                    let visible_cameras = cameras
                        .iter()
                        .filter(|(_, _, _, visible)| {
                            visible.get(TypeId::of::<Mesh3d>()).contains(entity)
                        })
                        .map(|(camera_entity, camera, target, _)| {
                            (
                                camera_entity,
                                names.get(camera_entity).ok().map(|(_, name)| name.as_str()),
                                camera.is_active,
                                target,
                            )
                        })
                        .collect::<Vec<_>>();
                    info!(target: "openzt2_verification", capture = %readiness.request.capture,
                        frame = readiness.request.frame, change, ?entity, ?mesh_asset,
                        mesh_asset_path = ?server.get_path(*mesh_asset),
                        mesh_entity_present = current_mesh.is_some(),
                        current_mesh_asset = ?current_mesh.map(|(_, mesh, _)| mesh.0.id()),
                        currently_view_visible = current_mesh.is_some_and(|(_, _, visible)| visible.get()),
                        name = ?names.get(*entity).ok().map(|(_, name)| name.as_str()),
                        ?parent, ?parent_name, ?model_owner, ?visible_cameras,
                        placement_decal_owner = ?decals.get(*entity).ok().map(|(owner, _, _)| owner.0),
                        "capture readiness changed mesh dependency");
                }
            }
        }
        readiness.request.node_names.extend(
            names
                .iter()
                .map(|(entity, name)| (entity, name.as_str().to_owned())),
        );
        let unloaded_model = models
            .iter()
            .filter(|(_, _, visibility)| visibility.get())
            .find(|(_, model, _)| {
                model
                    .handle
                    .as_ref()
                    .is_none_or(|handle| !server.is_loaded_with_dependencies(handle.id()))
            })
            .map(|(entity, model, _)| {
                (
                    entity,
                    names.get(entity).ok().map(|(_, name)| name.as_str()),
                    model.model_path.as_ref(),
                    model
                        .handle
                        .as_ref()
                        .and_then(|handle| server.get_load_states(handle.id())),
                )
            });
        let running_animation = animations
            .iter()
            .find(|(_, animation, visibility)| visibility.get() && animation.running)
            .map(|(entity, animation, _)| {
                (
                    entity,
                    names.get(entity).ok().map(|(_, name)| name.as_str()),
                    animation,
                )
            });
        info!(target: "openzt2_verification", capture = %readiness.request.capture, step = ?step,
            progress = ?session.step_capture, step_frame = session.step_frame, elapsed_seconds = elapsed.as_secs_f64(),
            frame = readiness.request.frame, deadline_reached,
            ui_ready, images_changed, meshes_changed, mesh_membership_changed,
            normalized_placement_decals = placement_decals.len(), decal_mesh_changed, decal_material_changed,
            stable_since = readiness.stable_since,
            main_stable = readiness.request.frame > readiness.stable_since,
            render_acknowledged = readiness.request.submitted_frame.load(Ordering::Acquire) > readiness.stable_since,
            submitted_frame = readiness.request.submitted_frame.load(Ordering::Acquire),
            ?running_animation, ?unloaded_model, "capture readiness main-world blockers");
        readiness.reported_pending_blockers = true;
    }
    readiness.request.meshes = required_meshes;
    readiness.mesh_entities = mesh_entities;
    readiness.placement_decals = placement_decals;
    readiness.deadline_observed |= deadline_reached;
}

#[allow(clippy::needless_pass_by_value)]
fn extract_capture_render_request(
    mut request: ResMut<VerificationCaptureRenderRequest>,
    readiness: Extract<Res<VerificationCaptureReadiness>>,
) {
    *request = readiness.request.clone();
}

#[allow(clippy::needless_pass_by_value)]
fn observe_extracted_capture_draw_layout(
    request: Res<VerificationCaptureRenderRequest>,
    extracted: Res<ExtractedUiNodes>,
    mut readiness: ResMut<VerificationCaptureDrawReadiness>,
) {
    if !request.ui_ready {
        request.submitted_frame.store(0, Ordering::Release);
        readiness.layout.clear();
        readiness.stable_since = request.frame;
        return;
    }
    // Extraction has already applied visibility, target-camera selection and
    // authored text projection. Sample before prepare_uinodes clears the data.
    // Temporary render entities and glyph-buffer ranges are recreated each frame;
    // compare the geometry and visual content they reference, keyed by the
    // persistent source entity.
    let mut layout = Vec::new();
    for draw in &extracted.uinodes {
        let mut append_quad = |rect: Rect, translation: Vec2, color: LinearRgba, node_style| {
            let corners = [
                Vec2::new(-0.5, -0.5),
                Vec2::new(0.5, -0.5),
                Vec2::new(0.5, 0.5),
                Vec2::new(-0.5, 0.5),
            ]
            // This is prepare_uinodes' pre-clip position formula for both Node
            // and Glyphs. rect.min selects UVs; it does not offset the quad.
            .map(|corner| {
                draw.transform
                    .transform_point2(translation + corner * rect.size())
            });
            let bounds = Rect {
                min: corners.iter().copied().fold(Vec2::INFINITY, Vec2::min),
                max: corners.iter().copied().fold(Vec2::NEG_INFINITY, Vec2::max),
            };
            if draw
                .clip
                .is_some_and(|clip| bounds.intersect(clip).is_empty())
            {
                return;
            }
            layout.push(VerificationUiDrawLayout {
                entity: draw.main_entity.id(),
                camera: draw.extracted_camera_entity,
                z_order: draw.z_order,
                image: draw.image,
                corners,
                clip: draw.clip,
                rect,
                color,
                node_style,
            });
        };
        match &draw.item {
            ExtractedUiItem::Node {
                rect,
                color,
                atlas_scaling,
                flip_x,
                flip_y,
                border_radius,
                border,
                node_type,
            } => append_quad(
                *rect,
                Vec2::ZERO,
                *color,
                Some(VerificationUiNodeDrawStyle {
                    atlas_scaling: *atlas_scaling,
                    flip_x: *flip_x,
                    flip_y: *flip_y,
                    border_radius: *border_radius,
                    border: *border,
                    node_type: *node_type,
                }),
            ),
            ExtractedUiItem::Glyphs { range } => {
                for glyph in &extracted.glyphs[range.clone()] {
                    append_quad(glyph.rect, glyph.translation, glyph.color, None);
                }
            }
        }
    }
    // Extraction systems append independently. Their append order is not draw
    // order; z_order is. Resolve equal-layer glyphs by position, atlas selection
    // and color so identical-position glyph groups have a stable comparison order.
    layout.sort_unstable_by(|left, right| {
        let node_type_key = |style: &Option<VerificationUiNodeDrawStyle>| match style
            .as_ref()
            .map(|style| style.node_type)
        {
            None => (0, 0),
            Some(NodeType::Rect) => (1, 0),
            Some(NodeType::Inverted) => (2, 0),
            Some(NodeType::Border(flags)) => (3, flags),
        };
        (left.entity, left.camera, left.image)
            .cmp(&(right.entity, right.camera, right.image))
            .then_with(|| left.z_order.total_cmp(&right.z_order))
            .then_with(|| node_type_key(&left.node_style).cmp(&node_type_key(&right.node_style)))
            .then_with(|| {
                left.corners
                    .iter()
                    .zip(&right.corners)
                    .find_map(|(left, right)| {
                        let order = left.x.total_cmp(&right.x).then(left.y.total_cmp(&right.y));
                        (!order.is_eq()).then_some(order)
                    })
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                [
                    left.rect.min.x,
                    left.rect.min.y,
                    left.rect.max.x,
                    left.rect.max.y,
                ]
                .into_iter()
                .chain(left.color.to_f32_array())
                .zip(
                    [
                        right.rect.min.x,
                        right.rect.min.y,
                        right.rect.max.x,
                        right.rect.max.y,
                    ]
                    .into_iter()
                    .chain(right.color.to_f32_array()),
                )
                .find_map(|(left, right)| {
                    let order = left.total_cmp(&right);
                    (!order.is_eq()).then_some(order)
                })
                .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let changed_draw = readiness
        .layout
        .iter()
        .zip(&layout)
        .find(|(previous, current)| !capture_draw_layout_matches(previous, current));
    let changed =
        layout.is_empty() || readiness.layout.len() != layout.len() || changed_draw.is_some();
    if request.log_timeout_blockers {
        let changed_entity = changed_draw.map(|(_, current)| current.entity);
        let changed_name = request
            .node_names
            .iter()
            .find(|(entity, _)| Some(*entity) == changed_entity)
            .map(|(_, name)| name);
        info!(target: "openzt2_verification", capture = %request.capture, step = ?request.step,
            frame = request.frame,
            stable_since = readiness.stable_since, previous_draws = readiness.layout.len(),
            current_draws = layout.len(), changed, ?changed_name, ?changed_draw,
            "capture readiness extracted draw layout blockers");
    }
    if changed {
        // An earlier stable submission cannot authorize a capture after new
        // geometry arrives, including while the current frame is rendering.
        request.submitted_frame.store(0, Ordering::Release);
        readiness.stable_since = request.frame;
        // Keep the last significant sample as the anchor. Replacing it on every
        // observation would let accumulated small motion appear stationary.
        readiness.layout = layout;
    }
}

fn capture_draw_layout_matches(
    previous: &VerificationUiDrawLayout,
    current: &VerificationUiDrawLayout,
) -> bool {
    // Physical-pixel geometry tolerates only floating-point reconstruction noise,
    // not rounded-pixel movement or a moving/settling authored animation.
    const PIXEL_TOLERANCE: f32 = 1.0 / 1024.0;
    previous.entity == current.entity
        && previous.camera == current.camera
        && previous.z_order == current.z_order
        && previous.image == current.image
        // Texture selection, opacity and shader styling require exact agreement.
        // Only physical positions below receive a reconstruction tolerance.
        && previous.rect == current.rect
        && previous.color == current.color
        && previous.node_style == current.node_style
        && previous
            .corners
            .iter()
            .zip(&current.corners)
            .all(|(previous, current)| previous.abs_diff_eq(*current, PIXEL_TOLERANCE))
        && match (previous.clip, current.clip) {
            (None, None) => true,
            (Some(previous), Some(current)) => {
                previous.min.abs_diff_eq(current.min, PIXEL_TOLERANCE)
                    && previous.max.abs_diff_eq(current.max, PIXEL_TOLERANCE)
            }
            _ => false,
        }
}

#[allow(clippy::needless_pass_by_value)]
fn acknowledge_prepared_capture_frame(
    request: Res<VerificationCaptureRenderRequest>,
    draw_readiness: Res<VerificationCaptureDrawReadiness>,
    images: Res<RenderAssets<GpuImage>>,
    meshes: Res<RenderAssets<RenderMesh>>,
    instances: Res<RenderMaterialInstances>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    pipelines: Res<PipelineCache>,
) {
    if request.log_timeout_blockers {
        let missing_image = request.images.iter().find(|id| images.get(**id).is_none());
        let missing_mesh = request
            .meshes
            .iter()
            .find(|(_, id)| meshes.get(*id).is_none());
        let missing_material = request.meshes.iter().find(|(entity, _)| {
            instances
                .instances
                .get(&MainEntity::from(*entity))
                .is_none_or(|instance| materials.get(instance.asset_id).is_none())
        });
        let missing_material_asset = missing_material.and_then(|(entity, _)| {
            instances
                .instances
                .get(&MainEntity::from(*entity))
                .map(|instance| instance.asset_id)
        });
        let waiting_pipeline = pipelines.waiting_pipelines().next();
        let missing_mesh_name = missing_mesh.and_then(|(entity, _)| {
            request
                .node_names
                .iter()
                .find(|(candidate, _)| candidate == entity)
                .map(|(_, name)| name)
        });
        let missing_material_name = missing_material.and_then(|(entity, _)| {
            request
                .node_names
                .iter()
                .find(|(candidate, _)| candidate == entity)
                .map(|(_, name)| name)
        });
        let blocked_pipeline = pipelines
            .pipelines()
            .enumerate()
            .find(|(_, pipeline)| !matches!(pipeline.state, CachedPipelineState::Ok(_)))
            .map(|(index, pipeline)| {
                let label = match &pipeline.descriptor {
                    PipelineDescriptor::RenderPipelineDescriptor(descriptor) => &descriptor.label,
                    PipelineDescriptor::ComputePipelineDescriptor(descriptor) => &descriptor.label,
                };
                (index, label, &pipeline.state)
            });
        info!(target: "openzt2_verification", capture = %request.capture, step = ?request.step,
            frame = request.frame, ui_ready = request.ui_ready,
            draw_count = draw_readiness.layout.len(), draw_stable_since = draw_readiness.stable_since,
            draw_stable = !draw_readiness.layout.is_empty() && request.frame > draw_readiness.stable_since,
            submitted_frame = request.submitted_frame.load(Ordering::Acquire),
            requested_images = request.images.len(), requested_meshes = request.meshes.len(),
            waiting_pipelines = pipelines.waiting_pipelines().count(),
            unfinished_pipelines = pipelines.pipelines()
                .filter(|pipeline| matches!(pipeline.state, CachedPipelineState::Queued | CachedPipelineState::Creating(_))).count(),
            failed_pipelines = pipelines.pipelines()
                .filter(|pipeline| matches!(pipeline.state, CachedPipelineState::Err(_))).count(),
            ?missing_image, ?missing_mesh, ?missing_mesh_name, ?missing_material, ?missing_material_name,
            ?missing_material_asset, ?waiting_pipeline, ?blocked_pipeline,
            "capture readiness render-world blockers");
    }
    if request.ui_ready
        && !draw_readiness.layout.is_empty()
        && request.frame > draw_readiness.stable_since
        && pipelines.waiting_pipelines().next().is_none()
        && pipelines
            .pipelines()
            .all(|pipeline| matches!(pipeline.state, CachedPipelineState::Ok(_)))
        && request.images.iter().all(|id| images.get(*id).is_some())
        && request.meshes.iter().all(|(entity, id)| {
            meshes.get(*id).is_some()
                && instances
                    .instances
                    .get(&MainEntity::from(*entity))
                    .is_some_and(|instance| materials.get(instance.asset_id).is_some())
        })
    {
        // The screenshot copy uses this same queue, so submission order is enough;
        // ScreenshotCaptured supplies the GPU/readback completion acknowledgment.
        request
            .submitted_frame
            .store(request.frame, Ordering::Release);
    } else {
        request.submitted_frame.store(0, Ordering::Release);
    }
}
