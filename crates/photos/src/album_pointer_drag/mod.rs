//! Native exposure dragging routed through the album's authored controls.
use bevy::picking::pointer::{PointerAction, PointerButton, PointerInput};
use openzt2_interface_state::input::input_types::MousePointer;
use openzt2_interface_state::photos::photo_album_types::AlbumMember;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
};

use openzt2_game_data::ui_document::action::UiTrigger;

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::ui_document::action::photography::UiPhotoAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::input::input_types::ActiveInputDevice;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPhotoActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;

use super::photo_album_types::{
    PhotoCameraRollRow, PhotoMove, PhotoMoveTarget, PhotoPointerDrag, PhotoPointerPress,
    SelectedAlbumPhoto, WorldActivePhotoAlbum, WorldPhotoCameraRoll,
};

/// The native album component starts a move while dragging an exposure and
/// dispatches the hovered selection's action on release. Reuse those same
/// authored controls so their enable/disable and overlay actions also execute.
pub(super) fn route_photo_exposure_pointer_drag_to_authored_album_controls(
    pointer: MousePointer,
    mut pointer_inputs: MessageReader<PointerInput>,
    active_input: Res<ActiveInputDevice>,
    capture: Res<UiPointerCapture>,
    parents: Query<&ChildOf>,
    exposures: Query<&PhotoCameraRollRow>,
    photo_memberships: Query<&AlbumMember>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    controls: Query<(
        Entity,
        &UiPhotoActions,
        &UiDocumentOwner,
        &InheritedVisibility,
    )>,
    mut storage: Query<
        (
            Option<&WorldActivePhotoAlbum>,
            Option<&WorldPhotoCameraRoll>,
            &mut SelectedAlbumPhoto,
            &mut PhotoPointerPress,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    albums: Query<(Has<PhotoPointerDrag>, Has<PhotoMove>)>,
    mut activations: MessageWriter<UiNodeActivated>,
    mut commands: Commands,
) {
    let mut primary_just_pressed = false;
    let mut primary_just_released = false;
    let mut pointer_delta = Vec2::ZERO;
    for input in pointer_inputs
        .read()
        .filter(|input| input.pointer_id.is_mouse())
    {
        primary_just_pressed |= input.button_just_pressed(PointerButton::Primary);
        primary_just_released |= input.button_just_released(PointerButton::Primary);
        if let PointerAction::Move { delta } = input.action {
            pointer_delta += delta;
        }
    }
    let primary_pressed = pointer.primary_is_pressed();
    let Ok((active, roll, mut selection, mut pressed)) = storage.single_mut() else {
        return;
    };
    let Some(camera_roll) = roll.and_then(|roll| roll.iter().next()) else {
        pressed.set_if_neq(PhotoPointerPress(None));
        return;
    };
    let Some(album) = active.and_then(|active| active.iter().next()) else {
        pressed.set_if_neq(PhotoPointerPress(None));
        return;
    };
    let Ok((dragging, moving)) = albums.get(album) else {
        return;
    };
    if primary_just_pressed {
        let photo = capture
            .target
            .and_then(|target| {
                std::iter::successors(Some(target), |entity| {
                    parents.get(*entity).ok().map(ChildOf::parent)
                })
                .find_map(|entity| exposures.get(entity).ok().map(|row| row.0))
            })
            .filter(|&photo| {
                photo_memberships
                    .get(photo)
                    .is_ok_and(|member| member.0 == camera_roll)
            });
        pressed.set_if_neq(PhotoPointerPress(photo.map(|photo| (album, photo))));
    }
    if !primary_pressed || primary_just_released {
        pressed.set_if_neq(PhotoPointerPress(None));
    }
    if dragging && primary_just_released {
        commands.entity(album).remove::<PhotoPointerDrag>();
        let target = capture.target.and_then(|target| {
            std::iter::successors(Some(target), |entity| {
                parents.get(*entity).ok().map(ChildOf::parent)
            })
            .find(|entity| {
                let Ok((_, actions, owner, visible)) = controls.get(*entity) else {
                    return false;
                };
                let Some(document) = roots
                    .get(owner.0)
                    .ok()
                    .and_then(|root| documents.get(&root.document))
                else {
                    return false;
                };
                visible.get()
                    && actions.authored_action_records(document).any(|record| {
                        matches!(
                            record.action,
                            UiPhotoAction::MoveSelectedPhotoToActiveAlbumSlot { .. }
                        )
                    })
            })
        });
        if let Some(node) = target {
            activations.write(UiNodeActivated {
                source: active_input.source,
                node,
                trigger: UiTrigger::On,
            });
        } else {
            // A release outside an album slot leaves the photo in its source.
            commands
                .entity(album)
                .remove::<PhotoMove>()
                .remove::<PhotoMoveTarget>();
            for (node, actions, owner, visible) in &controls {
                let Some(document) = roots
                    .get(owner.0)
                    .ok()
                    .and_then(|root| documents.get(&root.document))
                else {
                    continue;
                };
                if visible.get()
                    && actions.authored_action_records(document).any(|record| {
                        matches!(
                            record.action,
                            UiPhotoAction::StartMovingSelectedPhotoToActiveAlbum
                        )
                    })
                {
                    activations.write(UiNodeActivated {
                        source: active_input.source,
                        node,
                        trigger: UiTrigger::Off,
                    });
                }
            }
        }
        return;
    }
    if dragging || moving || !primary_pressed || pointer_delta == Vec2::ZERO {
        return;
    }
    let Some((_, photo)) = pressed.0.filter(|&(source, photo)| {
        source == album
            && photo_memberships
                .get(photo)
                .is_ok_and(|member| member.0 == camera_roll)
    }) else {
        return;
    };
    for (node, actions, owner, visible) in &controls {
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        if visible.get()
            && actions.authored_action_records(document).any(|record| {
                matches!(
                    record.action,
                    UiPhotoAction::StartMovingSelectedPhotoToActiveAlbum
                )
            })
        {
            commands.entity(album).insert(PhotoPointerDrag);
            selection.set_if_neq(SelectedAlbumPhoto(Some((camera_roll, photo))));
            activations.write(UiNodeActivated {
                source: active_input.source,
                node,
                trigger: UiTrigger::On,
            });
            break;
        }
    }
}
