//! Direct UI document camera controls for the one live Bevy camera.

use openzt2_game_data::ui_document::action::camera::{
    UiCameraAction, UiCameraPanDirection, UiCameraSignedAxisDirection,
};

use bevy::picking::pointer::{PointerId, PointerLocation};
use bevy::prelude::*;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        immersive_modes::{
            immersive_mode_message_types::{EnterImmersiveMode, ExitImmersiveMode, ModeExitReason},
            immersive_mode_state_types::{ActiveImmersiveMode, PendingImmersiveEntry},
        },
        information::entity_selection_types::SelectedEntity,
        staff::staff_assignment_types::StaffAssignmentMode,
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
        world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds,
    },
};

use super::{
    camera_control_message_types::{FocusCamera, RestoreCameraMode, SetCameraMode},
    camera_runtime_state_types::{
        CameraDefinition, CameraIntent, CameraMode, CameraMouseLook, ZooCamera,
    },
};
use crate::plugins::ui::authored_ui_action_projection_components::UiCameraActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_camera_ui_actions(
    (mut activations, mut direct_actions): (
        MessageReader<UiNodeActivated>,
        MessageReader<crate::plugins::input::input_types::ActionRequest>,
    ),
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiCameraActions, &UiDocumentOwner)>,
    action_geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    pointers: Query<(&PointerId, &PointerLocation)>,
    world_bounds: Query<&WorldTerrainHorizontalBounds>,
    roots: Query<&UiDocumentRoot>,
    selected: Res<SelectedEntity>,
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
        (Entity, Has<ActiveImmersiveMode>),
        Or<(
            With<ActiveImmersiveMode>,
            With<PendingImmersiveEntry>,
            With<StaffAssignmentMode>,
        )>,
    >,
    (mut pending_entries, mut exit_modes): (
        ResMut<Messages<EnterImmersiveMode>>,
        MessageWriter<ExitImmersiveMode>,
    ),
) {
    let mut return_to_overhead = direct_actions.read().any(|request| {
        request.action == crate::plugins::input::input_types::GameAction::OverheadView
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
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiCameraAction::RestorePreviouslySavedCameraMode => {
                    return_to_overhead = true;
                }
                UiCameraAction::StartFollowingSelectedEntity => {
                    if let Some(subject) = selected.0 {
                        for (_, definition, _, _) in &mut cameras {
                            set_mode.write(SetCameraMode {
                                mode: CameraMode::Follow(subject),
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
                    cameras
                        .iter_mut()
                        .for_each(|(_, _, mut intent, _)| intent.ui_pan = axis);
                }
                UiCameraAction::FocusCameraOnSelectedEntity => {
                    if let Some(world) = selected
                        .0
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
                    if let Some(subject) = selected.0 {
                        for (_, definition, _, _) in &mut cameras {
                            set_mode.write(SetCameraMode {
                                mode: CameraMode::FirstPerson(subject),
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
                    cameras
                        .iter_mut()
                        .for_each(|(_, _, mut intent, _)| intent.ui_pan = Vec2::ZERO);
                }
            }
        }
    }
    if return_to_overhead {
        pending_entries.clear();
        // mode_overhead leaves the first-person/staff branch of
        // modes.xml, including an entry that has not committed yet.
        // Active tools still release their state through their owner.
        for (controller, active) in &mode_owners {
            commands
                .entity(controller)
                .remove::<(PendingImmersiveEntry, StaffAssignmentMode)>();
            if active {
                exit_modes.write(ExitImmersiveMode {
                    controller,
                    reason: ModeExitReason::Cancelled,
                });
            }
        }
        restore_mode.write(RestoreCameraMode {
            transition_seconds: None,
        });
        for (camera, _, mut intent, _) in &mut cameras {
            *intent = CameraIntent::default();
            commands.entity(camera).remove::<CameraMouseLook>();
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
