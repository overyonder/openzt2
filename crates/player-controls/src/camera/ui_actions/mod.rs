//! Direct UI document camera controls for the one live Bevy camera.

use openzt2_game_data::ui_document::action::camera::UiCameraAction;
use openzt2_game_data::ui_document::action::camera::UiCameraPanDirection;
use openzt2_game_data::ui_document::action::camera::UiCameraSignedAxisDirection;

use bevy::picking::pointer::{PointerId, PointerLocation};
use bevy::prelude::*;

use crate::immersive_modes::admission::ModeReservation;
use crate::immersive_modes::immersive_mode_entry_reservation::DeferredImmersiveSwitch;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::immersive_modes::immersive_mode_message_types::EnterImmersiveMode;
use openzt2_interface_state::immersive_modes::immersive_mode_message_types::ExitImmersiveMode;
use openzt2_interface_state::immersive_modes::immersive_mode_message_types::ModeExitReason;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiEdgePanSurface;
use openzt2_world_state::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_zoo_state::staff::staff_assignment_types::StaffAssignmentMode;

use super::camera_control_message_types::FocusCamera;
use super::camera_control_message_types::RestoreCameraMode;
use super::camera_control_message_types::SetCameraMode;
use super::camera_runtime_state_types::CameraDefinition;
use super::camera_runtime_state_types::CameraMode;
use super::camera_runtime_state_types::CameraMouseLook;
use super::camera_runtime_state_types::ZooCamera;
use crate::camera::camera_execution_types::{CameraIntent, CameraTuning};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiCameraActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_camera_ui_actions(
    (mut activations, mut direct_actions): (
        MessageReader<UiNodeActivated>,
        MessageReader<openzt2_interface_state::input::input_types::ActionRequest>,
    ),
    documents: Res<Assets<UiDocumentAsset>>,
    (parents, edges, camera_policies): (
        Query<&ChildOf>,
        Query<&UiEdgePanSurface>,
        Query<(&CameraMode, &CameraTuning), With<ZooCamera>>,
    ),
    action_nodes: Query<(&UiCameraActions, &UiDocumentOwner)>,
    action_geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    pointers: Query<(&PointerId, &PointerLocation)>,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    roots: Query<&UiDocumentRoot>,
    selected: WorldUiSelection,
    selected_transforms: Query<&GlobalTransform>,
    mut cameras: Query<
        (
            Entity,
            &CameraDefinition,
            &mut CameraIntent,
            Has<CameraMouseLook>,
        ),
        With<ZooCamera>,
    >,
    mut focus: MessageWriter<FocusCamera>,
    mut set_mode: MessageWriter<SetCameraMode>,
    mut restore_mode: MessageWriter<RestoreCameraMode>,
    mut commands: Commands,
    mode_owners: Query<
        (
            Entity,
            Option<&ActiveImmersiveMode>,
            Has<StaffAssignmentMode>,
        ),
        Or<(
            With<ActiveImmersiveMode>,
            With<ModeReservation>,
            With<DeferredImmersiveSwitch>,
            With<StaffAssignmentMode>,
        )>,
    >,
    (mut pending_entries, mut exit_modes): (
        ResMut<Messages<EnterImmersiveMode>>,
        MessageWriter<ExitImmersiveMode>,
    ),
) {
    let mut return_to_overhead = direct_actions.read().any(|request| {
        request.action == openzt2_interface_state::input::input_types::GameAction::OverheadView
    });
    for activation in activations.read() {
        let Ok((range, owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let records = range.authored_action_records(document);
        let edge_surface = std::iter::once(activation.node)
            .chain(parents.iter_ancestors::<ChildOf>(activation.node))
            .find(|node| edges.contains(*node));
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiCameraAction::RestorePreviouslySavedCameraMode => {
                    return_to_overhead = true;
                }
                UiCameraAction::StartFollowingSelectedEntity => {
                    if let Some(subject) = selected.subject() {
                        for (_, definition, _, _) in &mut cameras {
                            set_mode.write(SetCameraMode {
                                mode: CameraMode::Follow,
                                subject: Some(subject),
                                definition: definition.0,
                                transition_seconds: None,
                            });
                        }
                    }
                }
                UiCameraAction::SetCameraPanDirection { direction } => {
                    let axis = match direction {
                        UiCameraPanDirection::North => Vec2::Y,
                        UiCameraPanDirection::NorthEast => Vec2::new(
                            std::f32::consts::FRAC_1_SQRT_2,
                            std::f32::consts::FRAC_1_SQRT_2,
                        ),
                        UiCameraPanDirection::East => Vec2::X,
                        UiCameraPanDirection::SouthEast => Vec2::new(
                            std::f32::consts::FRAC_1_SQRT_2,
                            -std::f32::consts::FRAC_1_SQRT_2,
                        ),
                        UiCameraPanDirection::South => Vec2::NEG_Y,
                        UiCameraPanDirection::SouthWest => Vec2::new(
                            -std::f32::consts::FRAC_1_SQRT_2,
                            -std::f32::consts::FRAC_1_SQRT_2,
                        ),
                        UiCameraPanDirection::West => Vec2::NEG_X,
                        UiCameraPanDirection::NorthWest => Vec2::new(
                            -std::f32::consts::FRAC_1_SQRT_2,
                            std::f32::consts::FRAC_1_SQRT_2,
                        ),
                    };
                    for (camera, _, intent, _) in &mut cameras {
                        if let Some(surface) = edge_surface {
                            if camera_policies.get(camera).is_ok_and(|(mode, tuning)| {
                                *mode == CameraMode::Overhead && tuning.edge_scroll
                            }) && edges.get(surface).is_ok_and(|surface| surface.0)
                            {
                                intent
                                    .map_unchanged(|intent| &mut intent.edge_pan)
                                    .set_if_neq(Some((surface, axis)));
                            }
                        } else {
                            intent
                                .map_unchanged(|intent| &mut intent.ui_pan)
                                .set_if_neq(axis);
                        }
                    }
                }
                UiCameraAction::FocusCameraOnSelectedEntity => {
                    if let Some(world) = selected
                        .subject()
                        .and_then(|entity| selected_transforms.get(entity).ok())
                    {
                        focus.write(FocusCamera {
                            world: world.translation(),
                            transition_seconds: None,
                        });
                    }
                }
                UiCameraAction::FocusCameraOnActivatedOverviewMapPoint => {
                    let point = pointers.iter().find_map(|(id, location)| {
                        (*id == PointerId::Mouse)
                            .then(|| location.location().map(|location| location.position))
                            .flatten()
                    });
                    let world = point
                        .zip(action_geometry.get(activation.node).ok())
                        .zip(world_bounds.single().ok())
                        .and_then(|((point, (node, transform)), bounds)| {
                            node.normalize_point(*transform, point).map(|local| {
                                let normalized = Vec2::new(local.x + 0.5, 0.5 - local.y)
                                    .clamp(Vec2::ZERO, Vec2::ONE);
                                let target = bounds.min + normalized * (bounds.max - bounds.min);
                                Vec3::new(target.x, 0.0, target.y)
                            })
                        });
                    if let Some(world) = world {
                        focus.write(FocusCamera {
                            world,
                            transition_seconds: None,
                        });
                    }
                }
                UiCameraAction::SetCameraMouseLookEnabled { enabled } => {
                    cameras.iter_mut().for_each(|(camera, _, _, _)| {
                        set_camera_mouse_look_enabled(&mut commands, camera, *enabled);
                    });
                }
                UiCameraAction::ToggleCameraMouseLookEnabled => {
                    cameras.iter_mut().for_each(|(camera, _, _, enabled)| {
                        set_camera_mouse_look_enabled(&mut commands, camera, !enabled);
                    });
                }
                UiCameraAction::EnterFirstPersonCameraForSelectedEntity => {
                    if let Some(subject) = selected.subject() {
                        for (_, definition, _, _) in &mut cameras {
                            set_mode.write(SetCameraMode {
                                mode: CameraMode::FirstPerson,
                                subject: Some(subject),
                                definition: definition.0,
                                transition_seconds: None,
                            });
                        }
                    }
                }
                UiCameraAction::SetPhotoZoomCommandState { direction, pressed } => {
                    for (_, _, mut intent, _) in &mut cameras {
                        match direction {
                            UiCameraSignedAxisDirection::Positive => {
                                intent.photo_zoom_in_command = *pressed
                            }
                            UiCameraSignedAxisDirection::Negative => {
                                intent.photo_zoom_out_command = *pressed
                            }
                        }
                    }
                }
                UiCameraAction::SetCameraZoomDirection { direction } => {
                    let axis = match direction {
                        UiCameraSignedAxisDirection::Positive => 1.0,
                        UiCameraSignedAxisDirection::Negative => -1.0,
                    };
                    cameras
                        .iter_mut()
                        .for_each(|(_, _, mut intent, _)| intent.ui_zoom = axis);
                }
                UiCameraAction::SetCameraRotationDirection { direction } => {
                    let axis = match direction {
                        UiCameraSignedAxisDirection::Positive => 1.0,
                        UiCameraSignedAxisDirection::Negative => -1.0,
                    };
                    cameras
                        .iter_mut()
                        .for_each(|(_, _, mut intent, _)| intent.ui_turn = axis);
                }
                UiCameraAction::StopCameraZoom => {
                    cameras
                        .iter_mut()
                        .for_each(|(_, _, mut intent, _)| intent.ui_zoom = 0.0);
                }
                UiCameraAction::StopCameraRotation => {
                    cameras
                        .iter_mut()
                        .for_each(|(_, _, mut intent, _)| intent.ui_turn = 0.0);
                }
                UiCameraAction::StopCameraPan => {
                    for (_, _, intent, _) in &mut cameras {
                        if edge_surface.is_some() {
                            intent
                                .map_unchanged(|intent| &mut intent.edge_pan)
                                .set_if_neq(None);
                        } else {
                            intent
                                .map_unchanged(|intent| &mut intent.ui_pan)
                                .set_if_neq(Vec2::ZERO);
                        }
                    }
                }
            }
        }
    }
    if return_to_overhead {
        pending_entries.clear();
        let mut session_restores = false;
        for (controller, active, staff_assignment) in &mode_owners {
            if let Some(active) = active {
                session_restores |= active.restore_camera_on_exit;
                exit_modes.write(ExitImmersiveMode {
                    controller,
                    reason: ModeExitReason::Cancelled,
                });
            } else if staff_assignment {
                commands.entity(controller).remove::<StaffAssignmentMode>();
            } else {
                commands.entity(controller).despawn();
            }
        }
        if !session_restores {
            restore_mode.write(RestoreCameraMode {
                transition_seconds: None,
            });
        }
    }
}

fn set_camera_mouse_look_enabled(commands: &mut Commands, camera: Entity, enabled: bool) {
    if enabled {
        commands.entity(camera).insert(CameraMouseLook);
    } else {
        commands.entity(camera).remove::<CameraMouseLook>();
    }
}
