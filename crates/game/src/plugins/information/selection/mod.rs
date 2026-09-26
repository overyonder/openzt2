use super::information_view_types::InformationViewClass;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::InformationViewCategory;
use openzt2_game_data::ui_document::document::*;

use super::entity_selection_types::{
    InfoPanel, Inspectable, SelectedEntity, SelectionChanged, SelectionRequest,
};
use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::{
        authored_ui_node_projection_components::UiDocumentRoot,
        ui_document_lifecycle_contracts::ShowUiRole,
    },
};

/// Applies the last valid request in message order and reports at most one
/// observable change for the update.
pub(super) fn apply_selection_requests(
    mut requests: MessageReader<SelectionRequest>,
    inspectable: Query<(), With<Inspectable>>,
    mut selected: ResMut<SelectedEntity>,
    mut changed: MessageWriter<SelectionChanged>,
) {
    let mut requested = selected.0;
    for request in requests.read() {
        if request
            .entity
            .is_none_or(|entity| inspectable.get(entity).is_ok())
        {
            requested = request.entity;
        }
    }
    set_selection(&mut selected, requested, &mut changed);
}

/// Clears a selected entity in the same update in which its inspectability is
/// removed. Despawning an unrelated inspectable does no work.
pub(super) fn clear_removed_selection(
    inspectable: Query<(), With<Inspectable>>,
    mut selected: ResMut<SelectedEntity>,
    mut changed: MessageWriter<SelectionChanged>,
) {
    let Some(current) = selected.0 else {
        return;
    };
    if inspectable.get(current).is_err() {
        set_selection(&mut selected, None, &mut changed);
    }
}

/// Keeps the original bottom-right entity-information document attached to
/// the in-game HUD lifecycle. Selection owns only the subject; UI document still owns
/// the complete authored panel and Zoopedia link.
pub(super) fn reconcile_selected_entity_information(
    mut commands: Commands,
    selected: Res<SelectedEntity>,
    view_classes: Query<&InformationViewClass>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut roots: Query<(
        Entity,
        &UiDocumentRoot,
        &ChildOf,
        Option<&InfoPanel>,
        &mut Visibility,
    )>,
    mut show: MessageWriter<ShowUiRole>,
) {
    let selected_subject = selected.0.filter(|subject| {
        !view_classes
            .get(*subject)
            .is_ok_and(|class| class.0 == InformationViewCategory::Entrances)
    });
    let mut hud_owner = None;
    let mut has_entity_info = false;
    for (entity, root, parent, panel, mut visibility) in &mut roots {
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        match &&document.canonical_ui_document().role {
            UiDocumentRole::InGameHud => hud_owner = Some(parent.parent()),
            UiDocumentRole::EntityInfo => {
                has_entity_info = true;
                if let Some(subject) = selected_subject {
                    if panel.is_none() {
                        commands.entity(entity).insert(InfoPanel { subject });
                    }
                    *visibility = Visibility::Inherited;
                } else {
                    *visibility = Visibility::Hidden;
                }
            }
            _ => {}
        }
    }
    if selected_subject.is_some() && !has_entity_info {
        if let Some(owner) = hud_owner {
            show.write(ShowUiRole {
                role: UiDocumentRole::EntityInfo,
                owner,
            });
        }
    }
}

pub(super) fn bind_info_panel_subject(
    selected: Res<SelectedEntity>,
    view_classes: Query<&InformationViewClass>,
    mut panels: Query<(&mut InfoPanel, &mut Visibility)>,
) {
    if !selected.is_changed() {
        return;
    }
    for (mut panel, mut visibility) in &mut panels {
        match selected.0.filter(|subject| {
            !view_classes
                .get(*subject)
                .is_ok_and(|class| class.0 == InformationViewCategory::Entrances)
        }) {
            Some(subject) => {
                panel.subject = subject;
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
}

fn set_selection(
    selected: &mut SelectedEntity,
    current: Option<Entity>,
    changed: &mut MessageWriter<SelectionChanged>,
) {
    let previous = selected.0;
    if previous == current {
        return;
    }
    selected.0 = current;
    changed.write(SelectionChanged { previous, current });
}
