//! Routing of authored photography actions to direct album operations.
use openzt2_interface_state::photos::photo_album_types::CameraRollPhoto;
use openzt2_interface_state::photos::photo_album_types::ExportPhotoAlbum;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbum;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbumOrder;
use openzt2_interface_state::photos::photo_album_types::{AlbumMember, AlbumPhotos};
use openzt2_interface_state::photos::photo_capture_types::CapturePhotoRequest;

use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_game_data::ui_document::action::photography::UiPhotoAction;

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::immersive_modes::immersive_mode_control_types::PhotoControl;
use openzt2_interface_state::immersive_modes::immersive_mode_message_types::ExitImmersiveMode;
use openzt2_interface_state::immersive_modes::immersive_mode_message_types::ModeExitReason;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ImmersiveCamera;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldRoot},
};

use super::album_operations::{
    calculate_active_photo_album_page_offset, deselect_photo_in_album_display_slot,
    find_photo_in_album_display_slot, reorder_photos_for_insertion_at_album_position,
    request_deletion_of_selected_photo_from_album, select_photo_album_by_list_index,
    select_photo_in_album_display_slot,
};
use super::photo_album_types::{
    ActivePhotoAlbum, DeletePhotoRequest, EnlargedAlbumPhoto, INITIAL_PHOTO_ALBUM_SPREADS,
    MAX_PHOTO_ALBUM_SPREADS, PHOTO_ALBUM_PAGE_SIZE, PhotoAlbumCapacityPages, PhotoAlbumChoiceRow,
    PhotoAlbumPage, PhotoCameraRoll, PhotoCameraRollRow, PhotoHelpShown, PhotoMove,
    PhotoMoveTarget, PhotoPointerDrag, SelectedAlbumPhoto, WorldActivePhotoAlbum,
    WorldPhotoCameraRoll,
};
use super::photo_configuration::{PhotoConfiguration, PhotoConfigurationSource};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPhotoActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Applies selection emitted by the shipped reusable exposure and album-entry
/// rows to their canonical album entities. The row components contain only
/// entity references; photo images, names, membership, and active selection
/// remain owned by the photo/album entities themselves.
pub(super) fn select_photo_storage_entity_from_authored_row_activation(
    mut activations: MessageReader<UiNodeActivated>,
    parents: Query<&ChildOf>,
    camera_roll_rows: Query<&PhotoCameraRollRow>,
    album_choice_rows: Query<&PhotoAlbumChoiceRow>,
    mut world: Query<
        (Entity, &WorldPhotoCameraRoll, &mut SelectedAlbumPhoto),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    photo_memberships: Query<&AlbumMember>,
    albums: Query<&WorldMember, (With<PhotoAlbum>, Without<PhotoCameraRoll>)>,
    mut commands: Commands,
) {
    let Ok((root, roll, mut selection)) = world.single_mut() else {
        return;
    };
    let camera_roll = *roll.collection();
    for activation in activations.read() {
        let mut row = std::iter::successors(Some(activation.node), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        });
        if let Some(photo) = row
            .clone()
            .find_map(|entity| camera_roll_rows.get(entity).ok())
        {
            if !photo_memberships
                .get(photo.0)
                .is_ok_and(|member| member.0 == camera_roll)
            {
                continue;
            }
            match activation.trigger {
                UiTrigger::On => {
                    selection.set_if_neq(SelectedAlbumPhoto(Some((camera_roll, photo.0))));
                }
                UiTrigger::Off if selection.0 == Some((camera_roll, photo.0)) => {
                    selection.set_if_neq(SelectedAlbumPhoto(None));
                }
                _ => {}
            }
            continue;
        }
        if activation.trigger != UiTrigger::On {
            continue;
        }
        if let Some(choice) = row.find_map(|entity| album_choice_rows.get(entity).ok()) {
            if albums.get(choice.0).is_ok_and(|member| member.root == root) {
                commands.entity(choice.0).insert(ActivePhotoAlbum(root));
            }
        }
    }
}

pub(super) fn route_photo_ui_actions(
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiPhotoActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    mut album_queries: ParamSet<(
        Query<(
            Entity,
            &PhotoAlbum,
            Option<&WorldMember>,
            Option<&PhotoAlbumPage>,
            Option<&PhotoMove>,
            Option<&PhotoCameraRoll>,
        )>,
        Query<&mut PhotoAlbumPage>,
    )>,
    names: Query<&Name, With<PhotoAlbum>>,
    mut storage: Query<
        (
            Entity,
            Option<&WorldActivePhotoAlbum>,
            Option<&WorldPhotoCameraRoll>,
            &mut SelectedAlbumPhoto,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    (mut capacities, mut hover_slots): (
        Query<&mut PhotoAlbumCapacityPages, With<PhotoAlbum>>,
        Query<&mut PhotoMoveTarget>,
    ),
    (members, mut orders, configuration, configurations, album_options): (
        Query<&AlbumPhotos>,
        Query<&mut PhotoAlbumOrder>,
        Res<PhotoConfigurationSource>,
        Res<Assets<PhotoConfiguration>>,
        Option<Res<super::PhotoAlbumProfileOptions>>,
    ),
    mut allocators: Query<&mut PersistentIdAllocator>,
    mut deletions: MessageWriter<DeletePhotoRequest>,
    mut captures: MessageWriter<CapturePhotoRequest>,
    mut immersive_mode_exits: MessageWriter<ExitImmersiveMode>,
    mut exports: MessageWriter<ExportPhotoAlbum>,
    active_photo_modes: Query<(Entity, &ImmersiveCamera), With<PhotoControl>>,
    mut commands: Commands,
) {
    let default_album_spreads = album_options
        .as_ref()
        .and_then(|options| options.get_default_spreads())
        .or_else(|| {
            configurations
                .get(&configuration.0)
                .map(|configuration| configuration.album_spreads)
        })
        .unwrap_or(INITIAL_PHOTO_ALBUM_SPREADS);
    let Ok((world, active, roll, mut selection)) = storage.single_mut() else {
        return;
    };
    let active = active.and_then(|album| album.iter().next());
    let roll = roll.and_then(|album| album.iter().next());
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
            let albums = album_queries.p0();
            match &record.action {
                UiPhotoAction::CapturePhotoFromActivePhotoModeCamera => {
                    if let Ok((_, active_photo_mode)) = active_photo_modes.single() {
                        captures.write(CapturePhotoRequest {
                            camera: active_photo_mode.0,
                        });
                    }
                }
                UiPhotoAction::ExitActivePhotoMode => {
                    if let Ok((controller, _)) = active_photo_modes.single() {
                        immersive_mode_exits.write(ExitImmersiveMode {
                            controller,
                            reason: ModeExitReason::Cancelled,
                        });
                    }
                }
                UiPhotoAction::RefreshPresentedActivePhotoAlbumPhotoCount => {}
                UiPhotoAction::RefreshActivePhotoAlbumPage => {
                    if let Some(album) =
                        active.filter(|&album| albums.get(album).is_ok_and(|row| row.3.is_none()))
                    {
                        commands.entity(album).insert(PhotoAlbumPage::default());
                    }
                }
                UiPhotoAction::EnlargeSelectedPhotoInActiveAlbum => {
                    if let Some(album) = active {
                        if let Some(selected) = selection.in_album(album) {
                            commands.entity(album).insert(EnlargedAlbumPhoto(selected));
                        }
                    }
                }
                UiPhotoAction::StopEnlargingPhotoInActiveAlbum => {
                    if let Some(album) = active {
                        commands.entity(album).remove::<EnlargedAlbumPhoto>();
                    }
                }
                UiPhotoAction::ShowPreviousActivePhotoAlbumPage
                | UiPhotoAction::ShowNextActivePhotoAlbumPage => {
                    let delta = if matches!(
                        &record.action,
                        UiPhotoAction::ShowPreviousActivePhotoAlbumPage
                    ) {
                        -1
                    } else {
                        1
                    };
                    if let Some((album, page)) = calculate_active_photo_album_page_offset(
                        active,
                        delta,
                        &albums,
                        &capacities.as_readonly(),
                        &members,
                    ) {
                        if let Ok(mut current) = album_queries.p1().get_mut(album) {
                            current.set_if_neq(page);
                        } else {
                            commands.entity(album).insert(page);
                        }
                    }
                }
                UiPhotoAction::CreateNewPhotoAlbumFromActiveAlbumProfile => {
                    if let Some((album_entity, source, member, ..)) =
                        active.and_then(|id| albums.get(id).ok())
                    {
                        if let (Some(member), Ok(name)) = (member, names.get(album_entity)) {
                            if let Some(id) = allocators
                                .get_mut(member.root)
                                .ok()
                                .and_then(|mut allocator| allocator.allocate().ok())
                            {
                                let album = commands
                                    .spawn((
                                        PhotoAlbum {
                                            profile: source.profile,
                                        },
                                        Name::new(name.as_str().to_owned()),
                                        *member,
                                        id,
                                        ActivePhotoAlbum(world),
                                        PhotoAlbumPage::default(),
                                    ))
                                    .id();
                                commands
                                    .entity(album)
                                    .insert(PhotoAlbumCapacityPages(default_album_spreads));
                            }
                        }
                    }
                }
                UiPhotoAction::MarkPhotoAlbumHelpAsShown => {
                    if let Some(album) = active {
                        commands.entity(album).insert(PhotoHelpShown);
                    }
                }
                UiPhotoAction::DeleteActivePhotoAlbumAndContainedPhotos => {
                    if let Some(album) = active {
                        members
                            .get(album)
                            .into_iter()
                            .flat_map(RelationshipTarget::iter)
                            .for_each(|photo| {
                                deletions.write(DeletePhotoRequest { photo });
                            });
                        commands.entity(album).despawn();
                        if selection.in_album(album).is_some() {
                            selection.set_if_neq(SelectedAlbumPhoto(None));
                        }
                        if let Some((candidate, ..)) = albums
                            .iter()
                            .filter(|(candidate, _, member, _, _, roll)| {
                                *candidate != album
                                    && member.is_some_and(|member| member.root == world)
                                    && roll.is_none()
                            })
                            .min_by_key(|(candidate, ..)| candidate.to_bits())
                        {
                            commands.entity(candidate).insert(ActivePhotoAlbum(world));
                        }
                    }
                }
                UiPhotoAction::StartMovingSelectedPhotoToActiveAlbum => {
                    if let Some(album) = active {
                        if let Some((from, selected)) = selection.0.filter(|(from, photo)| {
                            members
                                .get(*from)
                                .is_ok_and(|members| members.collection().contains(photo))
                        }) {
                            commands.entity(album).insert(PhotoMove {
                                photo: selected,
                                from,
                            });
                        }
                    }
                }
                UiPhotoAction::CancelMovingSelectedPhoto => {
                    if let Some(album) = active {
                        commands
                            .entity(album)
                            .remove::<PhotoMove>()
                            .remove::<PhotoPointerDrag>()
                            .remove::<PhotoMoveTarget>();
                    }
                }
                UiPhotoAction::ExportActivePhotoAlbumAsHtml => {
                    if let Some(album) = active {
                        exports.write(ExportPhotoAlbum { album });
                    }
                }
                UiPhotoAction::SelectPhotoInActiveAlbumSlot {
                    album_slot_index: index,
                } => select_photo_in_album_display_slot(
                    active,
                    *index,
                    &albums,
                    &members,
                    &mut selection,
                ),
                UiPhotoAction::DeselectPhotoInActiveAlbumSlot {
                    album_slot_index: index,
                } => deselect_photo_in_album_display_slot(
                    active,
                    *index,
                    &albums,
                    &members,
                    &mut selection,
                ),
                UiPhotoAction::EnlargePhotoInActiveAlbumSlot {
                    album_slot_index: index,
                } => {
                    if let Some(photo) =
                        find_photo_in_album_display_slot(active, *index, &albums, &members)
                    {
                        if let Some(album) = active {
                            commands.entity(album).insert(EnlargedAlbumPhoto(photo));
                        }
                    }
                }
                UiPhotoAction::SelectPhotoInCameraRollSlot {
                    camera_roll_slot_index: index,
                } => select_photo_in_album_display_slot(
                    roll,
                    *index,
                    &albums,
                    &members,
                    &mut selection,
                ),
                UiPhotoAction::DeselectPhotoInCameraRollSlot {
                    camera_roll_slot_index: index,
                } => deselect_photo_in_album_display_slot(
                    roll,
                    *index,
                    &albums,
                    &members,
                    &mut selection,
                ),
                UiPhotoAction::SelectPhotoAlbumByListIndex {
                    album_list_index: index,
                } => select_photo_album_by_list_index(*index, world, &albums, &mut commands),
                UiPhotoAction::AddOnePageToActivePhotoAlbumCapacity => {
                    if let Some(album) = active {
                        if let Ok(mut capacity) = capacities.get_mut(album) {
                            let pages = capacity.0.saturating_add(1).min(MAX_PHOTO_ALBUM_SPREADS);
                            capacity.set_if_neq(PhotoAlbumCapacityPages(pages));
                        } else {
                            commands.entity(album).insert(PhotoAlbumCapacityPages(
                                default_album_spreads
                                    .saturating_add(1)
                                    .min(MAX_PHOTO_ALBUM_SPREADS),
                            ));
                        }
                    }
                }
                UiPhotoAction::SetHoveredPhotoMoveTargetSlot {
                    album_slot_index: index,
                } => {
                    if let Some(album) = active {
                        if let Ok(slot) = u8::try_from(*index) {
                            let next = PhotoMoveTarget(slot);
                            if let Ok(mut current) = hover_slots.get_mut(album) {
                                current.set_if_neq(next);
                            } else {
                                commands.entity(album).insert(next);
                            }
                        }
                    }
                }
                UiPhotoAction::ClearHoveredPhotoMoveTargetSlot {
                    album_slot_index: index,
                } => {
                    if let Some(album) = active {
                        if u8::try_from(*index).ok().is_some_and(|slot| {
                            hover_slots
                                .get(album)
                                .is_ok_and(|current| current.0 == slot)
                        }) {
                            commands.entity(album).remove::<PhotoMoveTarget>();
                        }
                    }
                }
                UiPhotoAction::MoveSelectedPhotoToActiveAlbumSlot {
                    album_slot_index: index,
                } => {
                    if let Some(album) = active {
                        if let Ok((_, _, _, _, Some(moving), _)) = albums.get(album) {
                            if !members
                                .get(moving.from)
                                .is_ok_and(|members| members.collection().contains(&moving.photo))
                            {
                                commands.entity(album).remove::<(
                                    PhotoMove,
                                    PhotoPointerDrag,
                                    PhotoMoveTarget,
                                )>();
                                continue;
                            }
                            let target = albums
                                .get(album)
                                .ok()
                                .and_then(|row| row.3)
                                .map_or(0, |page| page.0)
                                * PHOTO_ALBUM_PAGE_SIZE
                                + u32::try_from(*index).unwrap_or_default();
                            reorder_photos_for_insertion_at_album_position(
                                album,
                                moving.photo,
                                target,
                                &members,
                                &mut orders,
                                &mut commands,
                            );
                            let order = PhotoAlbumOrder(u64::from(target));
                            if let Ok(mut current) = orders.get_mut(moving.photo) {
                                current.set_if_neq(order);
                            } else {
                                commands.entity(moving.photo).insert(order);
                            }
                            commands
                                .entity(moving.photo)
                                .insert(AlbumMember(album))
                                .remove::<CameraRollPhoto>();
                            commands
                                .entity(album)
                                .remove::<PhotoMove>()
                                .remove::<PhotoPointerDrag>()
                                .remove::<PhotoMoveTarget>();
                            selection.set_if_neq(SelectedAlbumPhoto(Some((album, moving.photo))));
                        }
                    }
                }
                UiPhotoAction::DeleteAllPhotosFromCameraRoll => {
                    if let Some(roll) = roll {
                        members
                            .get(roll)
                            .into_iter()
                            .flat_map(RelationshipTarget::iter)
                            .for_each(|photo| {
                                deletions.write(DeletePhotoRequest { photo });
                            });
                    }
                }
                UiPhotoAction::DeleteSelectedPhotoFromAnyAlbum => {
                    let album = selection.0.map(|(album, _)| album);
                    request_deletion_of_selected_photo_from_album(
                        album,
                        &mut selection,
                        &mut deletions,
                    )
                }
                UiPhotoAction::DeleteSelectedPhotoFromActiveAlbum => {
                    request_deletion_of_selected_photo_from_album(
                        active,
                        &mut selection,
                        &mut deletions,
                    )
                }
                UiPhotoAction::DeleteSelectedPhotoFromCameraRoll => {
                    request_deletion_of_selected_photo_from_album(
                        roll,
                        &mut selection,
                        &mut deletions,
                    )
                }
            }
        }
    }
}
