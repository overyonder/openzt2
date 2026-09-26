//! Presentation of photo-album availability, images, selection, and text.

use openzt2_game_data::ui_document::{
    action::{photography::UiPhotoAction, presentation::UiPresentationAction, UiActionRecord},
    node::*,
    node_property_binding::*,
    widget_live_collection::UiWidgetLiveCollectionSource,
};

use bevy::prelude::*;

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use crate::plugins::ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use crate::plugins::ui::authored_ui_image_content_binding::UiImageBinding;
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiNodeId;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::album_operations::{
    find_photo_camera_roll_album, ordered_photo_album_entities, ordered_photo_entities_in_album,
    photo_rank_for_album_display_slot,
};
use super::photo_album_types::{
    ActivePhotoAlbum, AlbumMember, EnlargedAlbumPhoto, PhotoAlbum, PhotoAlbumCapacityPages,
    PhotoAlbumChoiceRow, PhotoAlbumOrder, PhotoAlbumPage, PhotoCameraRoll, PhotoCameraRollRow,
    PhotoMove, PhotoMoveTarget, PresentedPhotoCount, SelectedAlbumPhoto,
    INITIAL_PHOTO_ALBUM_SPREADS, PHOTO_CAMERA_ROLL_CAPACITY,
};
use super::photo_capture_types::{Photo, PhotoCaptionSubject};
use crate::plugins::ui::authored_ui_action_projection_components::{
    UiPhotoActions, UiPresentationActions,
};

/// Sizes the camera-roll and album-choice lists.
pub(super) fn request_authored_photo_camera_roll_and_album_choice_row_counts(
    lists: Query<(Entity, &UiListPolicy)>,
    rows: Query<&UiListRow>,
    albums: Query<(Entity, Has<PhotoCameraRoll>), With<PhotoAlbum>>,
    photos: Query<&AlbumMember, With<Photo>>,
    mut row_counts: MessageWriter<SetUiListRowCount>,
) {
    let camera_roll = albums.iter().find(|row| row.1).map(|row| row.0);
    for (list, policy) in &lists {
        let expected = match policy.source {
            UiWidgetLiveCollectionSource::PhotoCameraRoll => camera_roll.map_or(0, |album| {
                photos.iter().filter(|member| member.0 == album).count()
            }),
            UiWidgetLiveCollectionSource::PhotoAlbums => albums.iter().filter(|row| !row.1).count(),
            _ => continue,
        }
        .min(usize::from(u16::MAX)) as u16;
        let current = rows.iter().filter(|row| row.list == list).count() as u16;
        if current != expected {
            row_counts.write(SetUiListRowCount {
                list,
                count: expected,
            });
        }
    }
}

/// Binds each camera-roll or album-choice row to its photo or album.
#[allow(clippy::too_many_arguments)]
pub(super) fn present_photo_camera_roll_and_album_choices_in_authored_rows(
    mut commands: Commands,
    lists: Query<&UiListPolicy>,
    rows: Query<(
        Entity,
        &UiListRow,
        Option<&PhotoCameraRollRow>,
        Option<&PhotoAlbumChoiceRow>,
    )>,
    children: Query<&Children>,
    albums: Query<(
        Entity,
        &PhotoAlbum,
        Option<&WorldMember>,
        Option<&PhotoAlbumPage>,
        Option<&SelectedAlbumPhoto>,
        Option<&PhotoMove>,
        Option<&PhotoCameraRoll>,
        Option<&Name>,
        Has<ActivePhotoAlbum>,
    )>,
    photos: Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
    mut projected_nodes: Query<(
        &UiNodeId,
        Option<&mut ImageNode>,
        Option<&mut Text>,
        Option<&mut UiSelected>,
        Option<&mut UiPreviousSelection>,
    )>,
) {
    let camera_roll = albums.iter().find(|row| row.6.is_some()).map(|row| row.0);
    let camera_roll_photos = camera_roll
        .map(|album| ordered_photo_entities_in_album(album, &photos))
        .unwrap_or_default();
    let album_choices =
        ordered_photo_album_entities(albums.iter().filter(|row| row.6.is_none()).map(|row| row.0));
    let exposure_image_node = openzt2_game_data::AssetId::from_key("ui/role/fragment/node/button");
    for (row_entity, list_row, camera_row, album_row) in &rows {
        let Ok(policy) = lists.get(list_row.list) else {
            continue;
        };
        match policy.source {
            UiWidgetLiveCollectionSource::PhotoCameraRoll => {
                let Some(photo_entity) =
                    camera_roll_photos.get(usize::from(list_row.index)).copied()
                else {
                    continue;
                };
                let Ok((_, photo, member, _)) = photos.get(photo_entity) else {
                    continue;
                };
                let selected = albums
                    .get(member.0)
                    .ok()
                    .and_then(|album| album.4)
                    .is_some_and(|selected| selected.0 == photo_entity);
                if camera_row.is_none_or(|row| row.0 != photo_entity) {
                    commands
                        .entity(row_entity)
                        .insert(PhotoCameraRollRow(photo_entity));
                }
                present_photo_storage_row_descendants(
                    &mut commands,
                    row_entity,
                    Some((&photo.image, exposure_image_node)),
                    None,
                    selected,
                    &children,
                    &mut projected_nodes,
                );
            }
            UiWidgetLiveCollectionSource::PhotoAlbums => {
                let Some(&album_entity) = album_choices.get(usize::from(list_row.index)) else {
                    continue;
                };
                let Ok((_, _, _, _, _, _, _, name, active)) = albums.get(album_entity) else {
                    continue;
                };
                if album_row.is_none_or(|row| row.0 != album_entity) {
                    commands
                        .entity(row_entity)
                        .insert(PhotoAlbumChoiceRow(album_entity));
                }
                present_photo_storage_row_descendants(
                    &mut commands,
                    row_entity,
                    None,
                    name.map(Name::as_str),
                    active,
                    &children,
                    &mut projected_nodes,
                );
            }
            _ => {}
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
                Option<&mut UiPreviousSelection>,
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
        Option<&mut UiPreviousSelection>,
    )>,
) {
    if let Ok((node_id, image_node, text_node, selected_node, previous_selection)) =
        projected_nodes.get_mut(entity)
    {
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
                if let Some(mut previous) = previous_selection {
                    previous.synchronize_with_non_authored_selection_change(selected);
                }
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

/// Enables photo controls when the selected album supports their action.
pub(super) fn set_photo_album_control_interaction_availability_from_album_state(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    albums: Query<
        (
            Entity,
            Option<&PhotoAlbumPage>,
            Option<&SelectedAlbumPhoto>,
            Option<&PhotoAlbumCapacityPages>,
            Has<ActivePhotoAlbum>,
            Has<PhotoCameraRoll>,
        ),
        With<PhotoAlbum>,
    >,
    photos: Query<&AlbumMember, With<Photo>>,
    mut nodes: Query<
        (
            Option<&UiPhotoActions>,
            Option<&UiPresentationActions>,
            &UiDocumentOwner,
            &mut UiInteractionEnabled,
            Option<&mut UiSelected>,
        ),
        Or<(With<UiPhotoActions>, With<UiPresentationActions>)>,
    >,
) {
    let active = albums.iter().find(|row| row.4);
    let camera = albums.iter().find(|row| row.5);
    let photo_count = |album: Option<Entity>| {
        album.map_or(0, |album| {
            photos.iter().filter(|member| member.0 == album).count()
        })
    };
    let active_count = photo_count(active.map(|row| row.0));
    let camera_count = photo_count(camera.map(|row| row.0));
    let active_selected = active.and_then(|row| row.2).is_some();
    let camera_selected = camera.and_then(|row| row.2).is_some();

    for (photo_range, presentation_range, owner, mut enabled, selected) in &mut nodes {
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let direct_actions = photo_range
            .into_iter()
            .flat_map(|actions| actions.authored_action_records(document));
        let revealed_actions = presentation_range
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
                UiPhotoAction::DeleteAllPhotosFromCameraRoll => Some(camera_count != 0),
                UiPhotoAction::DeleteSelectedPhotoFromAnyAlbum => {
                    Some(active_selected || camera_selected)
                }
                UiPhotoAction::DeleteSelectedPhotoFromActiveAlbum => Some(active_selected),
                UiPhotoAction::DeleteSelectedPhotoFromCameraRoll => Some(camera_selected),
                UiPhotoAction::StartMovingSelectedPhotoToActiveAlbum => {
                    Some(active_selected || camera_selected)
                }
                UiPhotoAction::ExportActivePhotoAlbumAsHtml => Some(active_count != 0),
                UiPhotoAction::ShowPreviousActivePhotoAlbumPage => {
                    Some(active.and_then(|row| row.1).is_some_and(|page| page.0 != 0))
                }
                UiPhotoAction::ShowNextActivePhotoAlbumPage => Some(active.is_some_and(|row| {
                    let page = row.1.map_or(0, |page| page.0);
                    let capacity = row.3.map_or(INITIAL_PHOTO_ALBUM_SPREADS, |pages| pages.0);
                    page.saturating_add(1) < capacity
                })),
                _ => None,
            });
        let Some(next) = next else {
            continue;
        };
        if enabled.0 != next {
            enabled.0 = next;
        }
        if !next {
            if let Some(mut selected) = selected {
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
) {
    let active = active_albums
        .iter()
        .next()
        .map(|(entity, _)| entity)
        .or_else(|| albums.iter().next().map(|row| row.0));
    let ordered_photos_by_album: std::collections::HashMap<_, _> = albums
        .iter()
        .map(|row| (row.0, ordered_photo_entities_in_album(row.0, &photos)))
        .collect();
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
            ordered_photos_by_album.get(&album)?.get(rank).copied()
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
    albums: Query<&PresentedPhotoCount, (With<PhotoAlbum>, With<ActivePhotoAlbum>)>,
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
            let next = count.0.to_string();
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
    photos: Query<&AlbumMember, With<Photo>>,
    mut nodes: Query<(&UiTextBinding, &mut Text)>,
) {
    let active = active_albums.iter().next();
    let page = active.map_or(0, |(page, _)| page.0);
    let roll = camera_rolls.iter().next();
    let film_count = roll.map_or(0, |roll| {
        photos.iter().filter(|member| member.0 == roll).count() as u32
    });
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

/// Uses the most prominent subject's localized name as the default caption.
pub(super) fn present_default_photo_captions_from_canonical_subjects(
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
    photos: Query<(
        Entity,
        &Photo,
        &AlbumMember,
        Option<&PhotoAlbumOrder>,
        Option<&PhotoCaptionSubject>,
    )>,
    photo_ordering_view: Query<(Entity, &Photo, &AlbumMember, Option<&PhotoAlbumOrder>)>,
    changed_photo_memberships: Query<(), Changed<AlbumMember>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    mut caption_nodes: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let active_album = active_albums.iter().next();
    let active_album_entity = active_album.as_ref().map(|(entity, _)| *entity);
    let page_changed = active_album.is_some_and(|(_, page)| page.is_changed());
    let ordered_photos = active_album_entity
        .map(|album| ordered_photo_entities_in_album(album, &photo_ordering_view))
        .unwrap_or_default();
    for (binding, mut caption_text) in &mut caption_nodes {
        let UiTextPropertyBindingSource::PhotoAlbumCaption { slot } = &binding.0 else {
            continue;
        };
        if !binding.is_added()
            && !page_changed
            && !world_definition_assets.is_changed()
            && !localization_assets.is_changed()
            && !active_localization.is_changed()
            && changed_photo_memberships.is_empty()
        {
            continue;
        }
        let caption = active_album_entity
            .and_then(|album| albums.get(album).ok())
            .and_then(|album| {
                photo_rank_for_album_display_slot(i32::from(*slot), album.6.is_some(), album.3)
            })
            .and_then(|rank| ordered_photos.get(rank).copied())
            .and_then(|photo_entity| photos.get(photo_entity).ok())
            .and_then(|photo| photo.4)
            .and_then(|subject| world_definitions.find_object(subject.0))
            .and_then(|definition| {
                localization
                    .find_plain_localized_text(openzt2_game_data::AssetId(definition.name_key.0))
            })
            .unwrap_or("");
        if caption_text.0 != caption {
            caption_text.0.clear();
            caption_text.0.push_str(caption);
        }
    }
}
