//! Presentation of photo-album availability, images, selection, and text.

use openzt2_interface_state::photos::photo_album_types::PhotoAlbum;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbumOrder;
use openzt2_interface_state::photos::photo_album_types::{AlbumMember, AlbumPhotos};
use openzt2_interface_state::photos::photo_capture_types::Photo;

use openzt2_game_data::ui_document::{
    action::{UiActionRecord, photography::UiPhotoAction, presentation::UiPresentationAction},
    node::*,
    node_property_binding::*,
    widget_live_collection::UiWidgetLiveCollectionSource,
};

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::RequestedUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_ui::authored_ui_image_content_binding::UiImageBinding;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldRoot},
};

use super::album_operations::{
    find_photo_camera_roll_album, ordered_photo_album_entities, photo_rank_for_album_display_slot,
};
use super::photo_album_types::{
    ActivePhotoAlbum, EnlargedAlbumPhoto, INITIAL_PHOTO_ALBUM_SPREADS, PHOTO_CAMERA_ROLL_CAPACITY,
    PhotoAlbumCapacityPages, PhotoAlbumChoiceRow, PhotoAlbumPage, PhotoCameraRoll,
    PhotoCameraRollRow, PhotoMove, PhotoMoveTarget, SelectedAlbumPhoto,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPhotoActions;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPresentationActions;

#[derive(Component, Default)]
pub(super) struct PhotoAlbumChoiceOrder(Vec<Entity>);

pub(super) fn present_photo_camera_roll_and_album_choices_in_authored_rows(
    mut commands: Commands,
    mut lists: Query<(
        Entity,
        Ref<UiListPolicy>,
        &mut RequestedUiListRowCount,
        Option<&Children>,
        Option<&mut PhotoAlbumChoiceOrder>,
    )>,
    mut rows: Query<(
        Ref<UiListRow>,
        Ref<ChildOf>,
        Option<&mut PhotoCameraRollRow>,
        Option<&mut PhotoAlbumChoiceRow>,
    )>,
    children: Query<&Children>,
    albums: Query<
        (
            Entity,
            Has<PhotoCameraRoll>,
            Option<Ref<SelectedAlbumPhoto>>,
            Option<Ref<Name>>,
            Has<ActivePhotoAlbum>,
        ),
        With<PhotoAlbum>,
    >,
    photos: Query<Ref<Photo>>,
    members: Query<Ref<AlbumPhotos>>,
    new_albums: Query<(), Added<PhotoAlbum>>,
    active_albums: Query<(), Added<ActivePhotoAlbum>>,
    mut removed: (
        RemovedComponents<PhotoAlbum>,
        RemovedComponents<ActivePhotoAlbum>,
        RemovedComponents<SelectedAlbumPhoto>,
        RemovedComponents<Name>,
        RemovedComponents<AlbumPhotos>,
    ),
    mut projected_nodes: Query<(
        &UiNodeId,
        Option<&mut ImageNode>,
        Option<&mut Text>,
        Option<&mut UiSelected>,
    )>,
) {
    let membership_changed = !new_albums.is_empty() || !removed.0.is_empty();
    let active_changed = !active_albums.is_empty() || !removed.1.is_empty();
    let selection_removed = !removed.2.is_empty();
    let name_removed = removed.3.read().collect::<std::collections::HashSet<_>>();
    removed.0.clear();
    removed.1.clear();
    removed.2.clear();

    let roll = albums.iter().find(|(_, is_roll, ..)| *is_roll);
    let roll_members = roll
        .as_ref()
        .and_then(|(entity, ..)| members.get(*entity).ok());
    let roll_order = roll_members
        .as_ref()
        .map_or(&[][..], |members| members.collection().as_slice());
    let roll_selection = roll
        .as_ref()
        .and_then(|(_, _, selection, ..)| selection.as_ref());
    let roll_members_removed = removed
        .4
        .read()
        .any(|entity| roll.as_ref().is_some_and(|(roll, ..)| *roll == entity));
    removed.4.clear();
    let roll_changed = membership_changed
        || selection_removed
        || roll_members_removed
        || roll_members.as_ref().is_some_and(Ref::is_changed)
        || roll_selection.is_some_and(Ref::is_changed);
    let mut choices = None;
    let exposure_image_node = openzt2_game_data::AssetId::from_key("ui/role/fragment/node/button");

    for (list, policy, mut count, owned_rows, mut order) in &mut lists {
        let (order, all_changed) = match policy.source {
            UiWidgetLiveCollectionSource::PhotoCameraRoll => (roll_order, roll_changed),
            UiWidgetLiveCollectionSource::PhotoAlbums => {
                if membership_changed || policy.is_changed() || order.is_none() {
                    let choices = choices.get_or_insert_with(|| {
                        ordered_photo_album_entities(
                            albums
                                .iter()
                                .filter(|(_, roll, ..)| !*roll)
                                .map(|(entity, ..)| entity),
                        )
                    });
                    if let Some(order) = &mut order {
                        order.0.clone_from(choices);
                    } else {
                        commands
                            .entity(list)
                            .insert(PhotoAlbumChoiceOrder(choices.clone()));
                    }
                    (choices.as_slice(), true)
                } else {
                    let Some(order) = order.as_deref() else {
                        continue;
                    };
                    (order.0.as_slice(), active_changed)
                }
            }
            _ => continue,
        };
        count.set_if_neq(RequestedUiListRowCount(Some(
            order.len().min(usize::from(u16::MAX)) as u16,
        )));
        for entity in owned_rows.into_iter().flat_map(RelationshipTarget::iter) {
            let Ok((row, parent, camera_row, album_row)) = rows.get_mut(entity) else {
                continue;
            };
            let Some(&subject) = order.get(usize::from(row.index)) else {
                continue;
            };
            let mut refresh =
                all_changed || row.is_changed() || parent.is_changed() || policy.is_changed();
            if policy.source == UiWidgetLiveCollectionSource::PhotoCameraRoll {
                let Ok(photo) = photos.get(subject) else {
                    continue;
                };
                let selected = roll_selection.is_some_and(|selection| selection.0 == subject);
                if let Some(mut row) = camera_row {
                    refresh |= row.set_if_neq(PhotoCameraRollRow(subject));
                } else {
                    commands.entity(entity).insert(PhotoCameraRollRow(subject));
                    refresh = true;
                }
                if refresh || photo.is_changed() {
                    present_photo_storage_row_descendants(
                        &mut commands,
                        entity,
                        Some((&photo.image, exposure_image_node)),
                        None,
                        selected,
                        &children,
                        &mut projected_nodes,
                    );
                }
            } else {
                let Ok((_, _, _, name, active)) = albums.get(subject) else {
                    continue;
                };
                if let Some(mut row) = album_row {
                    refresh |= row.set_if_neq(PhotoAlbumChoiceRow(subject));
                } else {
                    commands.entity(entity).insert(PhotoAlbumChoiceRow(subject));
                    refresh = true;
                }
                if refresh
                    || name_removed.contains(&subject)
                    || name.as_ref().is_some_and(Ref::is_changed)
                {
                    present_photo_storage_row_descendants(
                        &mut commands,
                        entity,
                        None,
                        name.as_deref().map(Name::as_str),
                        active,
                        &children,
                        &mut projected_nodes,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textureless_authored_exposure_receives_a_photo_image() {
        let mut app = App::new();
        let id = openzt2_game_data::AssetId::from_key("ui/role/fragment/node/button");
        let exposure = app.world_mut().spawn(UiNodeId { index: 0, id }).id();
        app.add_systems(
            Update,
            move |mut commands: Commands,
                  children: Query<&Children>,
                  mut nodes: Query<(
                &UiNodeId,
                Option<&mut ImageNode>,
                Option<&mut Text>,
                Option<&mut UiSelected>,
            )>| {
                present_photo_storage_row_descendants(
                    &mut commands,
                    exposure,
                    Some((&Handle::<Image>::default(), id)),
                    None,
                    false,
                    &children,
                    &mut nodes,
                );
            },
        );
        app.update();
        let image = app
            .world()
            .get::<ImageNode>(exposure)
            .expect("photo image assigned to exposure row");
        assert!(matches!(image.image_mode, NodeImageMode::Stretch));
        app.update();
        assert!(app.world().get::<ImageNode>(exposure).is_some());
    }
}

fn present_photo_storage_row_descendants(
    commands: &mut Commands,
    entity: Entity,
    image: Option<(&Handle<Image>, openzt2_game_data::AssetId)>,
    text: Option<&str>,
    selected: bool,
    children: &Query<&Children>,
    projected_nodes: &mut Query<(
        &UiNodeId,
        Option<&mut ImageNode>,
        Option<&mut Text>,
        Option<&mut UiSelected>,
    )>,
) {
    if let Ok((node_id, image_node, text_node, selected_node)) = projected_nodes.get_mut(entity) {
        if image.is_some_and(|(_, expected)| node_id.id == expected) {
            if let Some((image, _)) = image {
                if let Some(mut image_node) = image_node {
                    if image_node.image != *image {
                        image_node.image = image.clone();
                    }
                } else {
                    // exposure.xml starts with a textureless button.
                    commands.entity(entity).insert(ImageNode {
                        image: image.clone(),
                        image_mode: NodeImageMode::Stretch,
                        ..default()
                    });
                }
            }
        }
        if let (Some(text), Some(mut text_node)) = (text, text_node) {
            if text_node.0 != text {
                text_node.0.clear();
                text_node.0.push_str(text);
            }
        }
        if let Some(mut selected_node) = selected_node {
            if selected_node.0 != selected {
                selected_node.0 = selected;
            }
        }
    }
    if let Ok(entity_children) = children.get(entity) {
        for child in entity_children.iter() {
            present_photo_storage_row_descendants(
                commands,
                child,
                image,
                text,
                selected,
                children,
                projected_nodes,
            );
        }
    }
}

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct AlbumControlAvailability {
    active_has_photos: bool,
    roll_has_photos: bool,
    active_selected: bool,
    roll_selected: bool,
    previous_page: bool,
    next_page: bool,
}

pub(super) fn update_album_control_availability_from_album_changes(
    mut worlds: Query<
        (Entity, &mut AlbumControlAvailability),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    albums: Query<
        (
            &WorldMember,
            Option<Ref<ActivePhotoAlbum>>,
            Option<Ref<PhotoCameraRoll>>,
            Option<Ref<PhotoAlbumPage>>,
            Option<Ref<PhotoAlbumCapacityPages>>,
            Option<Ref<SelectedAlbumPhoto>>,
            Option<Ref<AlbumPhotos>>,
        ),
        (
            With<PhotoAlbum>,
            Or<(With<ActivePhotoAlbum>, With<PhotoCameraRoll>)>,
        ),
    >,
    mut removed: (
        RemovedComponents<ActivePhotoAlbum>,
        RemovedComponents<PhotoCameraRoll>,
        RemovedComponents<PhotoAlbumPage>,
        RemovedComponents<PhotoAlbumCapacityPages>,
        RemovedComponents<SelectedAlbumPhoto>,
        RemovedComponents<AlbumPhotos>,
    ),
) {
    let inputs_removed = !removed.0.is_empty()
        || !removed.1.is_empty()
        || !removed.2.is_empty()
        || !removed.3.is_empty()
        || !removed.4.is_empty()
        || !removed.5.is_empty();
    removed.0.clear();
    removed.1.clear();
    removed.2.clear();
    removed.3.clear();
    removed.4.clear();
    removed.5.clear();

    for (world, mut current) in &mut worlds {
        let mut next = AlbumControlAvailability::default();
        let mut changed = inputs_removed || current.is_added();
        for (member, active, roll, page, capacity, selection, photos) in &albums {
            if member.root != world {
                continue;
            }
            changed |= active.as_ref().is_some_and(Ref::is_changed)
                || roll.as_ref().is_some_and(Ref::is_changed)
                || page.as_ref().is_some_and(Ref::is_changed)
                || capacity.as_ref().is_some_and(Ref::is_changed)
                || selection.as_ref().is_some_and(Ref::is_changed)
                || photos.as_ref().is_some_and(Ref::is_changed);
            let has_photos = photos.as_ref().is_some_and(|photos| !photos.is_empty());
            let has_selection = selection.is_some_and(|selection| {
                photos
                    .as_ref()
                    .is_some_and(|photos| photos.collection().contains(&selection.0))
            });
            if active.is_some() {
                let page = page.map_or(0, |page| page.0);
                let capacity = capacity.map_or(INITIAL_PHOTO_ALBUM_SPREADS, |capacity| capacity.0);
                next.active_has_photos = has_photos;
                next.active_selected = has_selection;
                next.previous_page = page != 0;
                next.next_page = page.saturating_add(1) < capacity;
            }
            if roll.is_some() {
                next.roll_has_photos = has_photos;
                next.roll_selected = has_selection;
            }
        }
        if changed {
            current.set_if_neq(next);
        }
    }
}

/// Enables photo controls when the selected album supports their action.
pub(super) fn set_photo_album_control_interaction_availability_from_album_state(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<Ref<UiDocumentRoot>>,
    availability: Query<Ref<AlbumControlAvailability>, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    mut retired_worlds: RemovedComponents<WorldLoadCompleted>,
    mut removed_photo_actions: RemovedComponents<UiPhotoActions>,
    mut removed_presentation_actions: RemovedComponents<UiPresentationActions>,
    mut nodes: Query<
        (
            Option<Ref<UiPhotoActions>>,
            Option<Ref<UiPresentationActions>>,
            Ref<UiDocumentOwner>,
            &mut UiInteractionEnabled,
            Option<&mut UiSelected>,
        ),
        Or<(With<UiPhotoActions>, With<UiPresentationActions>)>,
    >,
) {
    let availability = availability.single().ok();
    let refresh_all = availability.as_ref().is_some_and(Ref::is_changed)
        || documents.is_changed()
        || !retired_worlds.is_empty()
        || !removed_photo_actions.is_empty()
        || !removed_presentation_actions.is_empty();
    removed_photo_actions.clear();
    removed_presentation_actions.clear();
    retired_worlds.clear();
    let default = AlbumControlAvailability::default();
    let availability = availability.as_deref().unwrap_or(&default);
    for (photo_range, presentation_range, owner, mut enabled, selected) in &mut nodes {
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        if !refresh_all
            && !root.is_changed()
            && !owner.is_changed()
            && !enabled.is_changed()
            && !selected.as_ref().is_some_and(|value| value.is_changed())
            && !photo_range.as_ref().is_some_and(|range| range.is_changed())
            && !presentation_range
                .as_ref()
                .is_some_and(|range| range.is_changed())
        {
            continue;
        }
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let direct_actions = photo_range
            .as_deref()
            .into_iter()
            .flat_map(|actions| actions.authored_action_records(document));
        let revealed_actions = presentation_range
            .as_deref()
            .into_iter()
            .flat_map(|actions| actions.authored_action_records(document))
            .filter_map(|record| match &record.action {
                UiPresentationAction::SetTargetNodeVisible {
                    target_node,
                    visible: true,
                } => Some(target_node.0),
                _ => None,
            })
            .flat_map(|target| {
                let nodes = document.canonical_ui_document().nodes.as_slice();
                let target = nodes.iter().position(|node| node.id.0 == target);
                nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, node)| {
                        // Only photo actions can affect availability. Avoid walking
                        // the parent chain for every unrelated authored UI node.
                        node.actions
                            .iter()
                            .any(|action| matches!(action, UiActionRecord::Photo(_)))
                    })
                    .filter_map(move |(index, node)| {
                        let target = target?;
                        is_ui_node_in_authored_subtree(nodes, index, target).then_some(
                            node.actions.iter().filter_map(|action| match action {
                                UiActionRecord::Photo(record) => Some(record),
                                _ => None,
                            }),
                        )
                    })
            })
            .flatten();
        // The confirmation launcher has the same availability as its operation.
        let next = direct_actions
            .chain(revealed_actions)
            .find_map(|record| match &record.action {
                UiPhotoAction::DeleteAllPhotosFromCameraRoll => Some(availability.roll_has_photos),
                UiPhotoAction::DeleteSelectedPhotoFromAnyAlbum => {
                    Some(availability.active_selected || availability.roll_selected)
                }
                UiPhotoAction::DeleteSelectedPhotoFromActiveAlbum => {
                    Some(availability.active_selected)
                }
                UiPhotoAction::DeleteSelectedPhotoFromCameraRoll => {
                    Some(availability.roll_selected)
                }
                UiPhotoAction::StartMovingSelectedPhotoToActiveAlbum => {
                    Some(availability.active_selected || availability.roll_selected)
                }
                UiPhotoAction::ExportActivePhotoAlbumAsHtml => Some(availability.active_has_photos),
                UiPhotoAction::ShowPreviousActivePhotoAlbumPage => Some(availability.previous_page),
                UiPhotoAction::ShowNextActivePhotoAlbumPage => Some(availability.next_page),
                _ => None,
            });
        let Some(next) = next else {
            continue;
        };
        if enabled.0 != next {
            enabled.0 = next;
        }
        if !next {
            if let Some(mut selected) = selected.filter(|selected| selected.0) {
                selected.0 = false;
            }
        }
    }
}

/// Photo slots share the image handle stored on each photo.
pub(super) fn present_active_photo_album_and_camera_roll_image_slots(
    mut commands: Commands,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(
        Entity,
        Option<&UiPhotoActions>,
        &UiDocumentOwner,
        &mut ImageNode,
        Option<&UiImageBinding>,
        Option<&mut Visibility>,
        Option<&mut UiSelected>,
    )>,
    roots: Query<&UiDocumentRoot>,
    albums: Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    active_albums: Query<(Entity, Ref<PhotoAlbumPage>), (With<PhotoAlbum>, With<ActivePhotoAlbum>)>,
    enlarged_albums: Query<&EnlargedAlbumPhoto, With<PhotoAlbum>>,
    move_targets: Query<&PhotoMoveTarget, With<PhotoAlbum>>,
    photos: Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
    members: Query<&AlbumPhotos>,
) {
    let active = active_albums
        .iter()
        .next()
        .map(|(entity, _)| entity)
        .or_else(|| albums.iter().next().map(|row| row.0));

    for (node, range, owner, mut image_node, image_binding, visibility, selected_state) in
        action_nodes
    {
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let actions = || {
            range
                .into_iter()
                .flat_map(|actions| actions.authored_action_records(document))
        };
        let move_target = actions().find_map(|record| match &record.action {
            UiPhotoAction::SetHoveredPhotoMoveTargetSlot {
                album_slot_index: index,
            } => u8::try_from(*index).ok(),
            _ => None,
        });
        if let Some(index) = move_target {
            let hovered = active
                .and_then(|album| move_targets.get(album).ok())
                .is_some_and(|target| target.0 == index);
            if let Some(mut state) = selected_state {
                if state.0 != hovered {
                    state.0 = hovered;
                }
            } else {
                commands.entity(node).insert(UiSelected(hovered));
            }
            continue;
        }
        let enlarged = actions()
            .any(|record| {
                matches!(
                    &record.action,
                    UiPhotoAction::StopEnlargingPhotoInActiveAlbum
                )
            })
            .then(|| {
                active
                    .and_then(|album| enlarged_albums.get(album).ok())
                    .map(|photo| photo.0)
            })
            .flatten();
        let slot = actions().find_map(|record| match &record.action {
            UiPhotoAction::SelectPhotoInActiveAlbumSlot {
                album_slot_index: index,
            }
            | UiPhotoAction::EnlargePhotoInActiveAlbumSlot {
                album_slot_index: index,
            } => Some((active, *index)),
            UiPhotoAction::SelectPhotoInCameraRollSlot {
                camera_roll_slot_index: index,
            } => Some((find_photo_camera_roll_album(&albums), *index)),
            _ => None,
        });
        let album_picture = image_binding.and_then(|binding| match &binding.0 {
            UiImagePropertyBindingSource::PhotoAlbumPicture { index } => {
                Some((active, i32::from(*index)))
            }
            _ => None,
        });
        let photo = enlarged.or_else(|| {
            let (album, index) = album_picture.or(slot)?;
            let album = album?;
            let album_state = albums.get(album).ok()?;
            let rank =
                photo_rank_for_album_display_slot(index, album_state.6.is_some(), album_state.3)?;
            members.get(album).ok()?.collection().get(rank).copied()
        });
        if slot.is_none()
            && album_picture.is_none()
            && !actions().any(|record| {
                matches!(
                    &record.action,
                    UiPhotoAction::StopEnlargingPhotoInActiveAlbum
                )
            })
        {
            continue;
        }
        let binds_photo_image = album_picture.is_some()
            || enlarged.is_some()
            || actions().any(|record| {
                matches!(
                    &record.action,
                    UiPhotoAction::SelectPhotoInCameraRollSlot { .. }
                )
            });
        if binds_photo_image {
            let next = photo
                .and_then(|photo| photos.get(photo).ok())
                .map(|row| row.1.image.clone())
                .unwrap_or_default();
            if image_node.image != next {
                image_node.image = next;
            }
        }
        if let Some(mut visibility) = visibility {
            let next = if photo.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != next {
                *visibility = next;
            }
        }
        let selected = slot
            .and_then(|(album, _)| album)
            .and_then(|album| albums.get(album).ok())
            .and_then(|row| row.4)
            .zip(photo)
            .is_some_and(|(selected, photo)| selected.0 == photo);
        if let Some(mut state) = selected_state {
            if state.0 != selected {
                state.0 = selected;
            }
        } else {
            commands.entity(node).insert(UiSelected(selected));
        }
    }
}

fn is_ui_node_in_authored_subtree(
    nodes: &[UiNodeDefinition],
    mut node: usize,
    subtree_root: usize,
) -> bool {
    loop {
        if node == subtree_root {
            return true;
        }
        let parent = nodes[node].parent as usize;
        if parent == node || parent >= nodes.len() {
            return false;
        }
        node = parent;
    }
}

/// Updates the active album's photo count.
pub(super) fn present_active_photo_album_photo_count(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    albums: Query<Option<&AlbumPhotos>, (With<PhotoAlbum>, With<ActivePhotoAlbum>)>,
    mut nodes: Query<(&UiPhotoActions, &UiDocumentOwner, &mut Text)>,
) {
    let Some(count) = albums.iter().next() else {
        return;
    };
    for (range, owner, mut text) in &mut nodes {
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        if range.authored_action_records(document).any(|record| {
            matches!(
                &record.action,
                UiPhotoAction::RefreshPresentedActivePhotoAlbumPhotoCount
            )
        }) {
            let next = count.map_or(0, RelationshipTarget::len).to_string();
            if text.0 != next {
                text.0 = next;
            }
        }
    }
}

/// Updates page numbers and camera-roll occupancy.
pub(super) fn present_photo_album_and_photo_mode_camera_roll_text(
    active_albums: Query<(&PhotoAlbumPage, &Name), (With<PhotoAlbum>, With<ActivePhotoAlbum>)>,
    camera_rolls: Query<Entity, (With<PhotoAlbum>, With<PhotoCameraRoll>)>,
    members: Query<&AlbumPhotos>,
    mut nodes: Query<(&UiTextBinding, &mut Text)>,
) {
    let active = active_albums.iter().next();
    let page = active.map_or(0, |(page, _)| page.0);
    let roll = camera_rolls.iter().next();
    let film_count = roll
        .and_then(|album| members.get(album).ok())
        .map_or(0, RelationshipTarget::len);
    for (binding, mut text) in &mut nodes {
        let next = match &binding.0 {
            UiTextPropertyBindingSource::PhotoAlbumPageNumber { offset } => {
                (page.saturating_mul(2) + u32::from(*offset)).to_string()
            }
            UiTextPropertyBindingSource::PhotoAlbumName => {
                active.map_or_else(String::new, |(_, name)| name.as_str().to_owned())
            }
            UiTextPropertyBindingSource::PhotoFilmCount => {
                format!("{film_count:02} / {PHOTO_CAMERA_ROLL_CAPACITY}")
            }
            _ => continue,
        };
        if text.0 != next {
            text.0 = next;
        }
    }
}

pub(super) fn present_stored_photo_captions(
    active_albums: Query<(Entity, Ref<PhotoAlbumPage>), (With<PhotoAlbum>, With<ActivePhotoAlbum>)>,
    albums: Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
    )>,
    photos: Query<Option<Ref<super::photo_caption::PhotoCaption>>, With<Photo>>,
    album_members: Query<Ref<AlbumPhotos>>,
    active_changes: Query<(), Added<ActivePhotoAlbum>>,
    mut removed_memberships: RemovedComponents<AlbumPhotos>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    mut caption_nodes: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    let Some(localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let active_album = active_albums.iter().next();
    let active_album_entity = active_album.as_ref().map(|(entity, _)| *entity);
    let page_changed = active_album.is_some_and(|(_, page)| page.is_changed());
    let membership = active_album_entity.and_then(|album| album_members.get(album).ok());
    let album_changed = !active_changes.is_empty()
        || membership
            .as_ref()
            .is_some_and(|members| members.is_changed())
        || removed_memberships
            .read()
            .any(|entity| Some(entity) == active_album_entity);
    removed_memberships.clear();
    let ordered_photos = membership
        .as_ref()
        .map_or(&[][..], |members| members.collection().as_slice());
    for (binding, mut caption_text) in &mut caption_nodes {
        let UiTextPropertyBindingSource::PhotoAlbumCaption { slot } = &binding.0 else {
            continue;
        };
        let photo = active_album_entity
            .and_then(|album| albums.get(album).ok())
            .and_then(|album| {
                photo_rank_for_album_display_slot(i32::from(*slot), album.6.is_some(), album.3)
            })
            .and_then(|rank| ordered_photos.get(rank).copied())
            .and_then(|photo_entity| photos.get(photo_entity).ok());
        if !binding.is_changed()
            && !page_changed
            && !album_changed
            && !localization_assets.is_changed()
            && !active_localization.is_changed()
            && !photo
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(Ref::is_changed)
        {
            continue;
        }
        let caption = match photo.as_ref() {
            Some(Some(caption)) => caption.0.as_str(),
            Some(None) => localization
                .find_plain_localized_text(super::photo_caption::default_photo_caption_key())
                .unwrap_or_default(),
            None => "",
        };
        if caption_text.0 != caption {
            caption_text.0.clear();
            caption_text.0.push_str(caption);
        }
    }
}
