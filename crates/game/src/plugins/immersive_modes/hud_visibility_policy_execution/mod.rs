use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::authored_ui_node_projection_components::UiDocumentRoot,
};

use super::immersive_mode_policy_types::{HideHudWhileActive, ModeHiddenHud};

/// Hides the HUD during immersive mode and restores its previous visibility on exit.
pub(super) fn hide_ingame_hud_while_requested_immersive_mode_is_active_and_restore_visibility(
    controllers_requesting_hidden_hud: Query<(), With<HideHudWhileActive>>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    mut ui_document_roots: Query<(
        Entity,
        &UiDocumentRoot,
        &mut Visibility,
        Option<&ModeHiddenHud>,
    )>,
    mut commands: Commands,
) {
    let immersive_mode_requests_hidden_hud = !controllers_requesting_hidden_hud.is_empty();
    for (
        ui_document_root_entity,
        ui_document_root,
        mut current_visibility,
        previous_mode_hidden_hud,
    ) in &mut ui_document_roots
    {
        let Some(ui_document_asset) = ui_document_assets.get(&ui_document_root.document) else {
            continue;
        };
        let role = ui_document_asset.canonical_ui_document().role;
        if !matches!(
            role,
            UiDocumentRole::InGameHud
                | UiDocumentRole::EntityInfo
                | UiDocumentRole::AnimalCareCatalogue
                | UiDocumentRole::PurchaseCatalogue
        ) {
            continue;
        }
        match (immersive_mode_requests_hidden_hud, previous_mode_hidden_hud) {
            (true, None) => {
                let previous_visibility = *current_visibility;
                current_visibility.set_if_neq(Visibility::Hidden);
                commands
                    .entity(ui_document_root_entity)
                    .insert(ModeHiddenHud {
                        previous: previous_visibility,
                    });
            }
            (false, Some(previous_mode_hidden_hud)) => {
                // Entity information projects its current selection every UI
                // update. Preserve that result instead of resurrecting the
                // subject that happened to be selected on mode entry.
                if role != UiDocumentRole::EntityInfo {
                    current_visibility.set_if_neq(previous_mode_hidden_hud.previous);
                }
                commands
                    .entity(ui_document_root_entity)
                    .remove::<ModeHiddenHud>();
            }
            (true, Some(_)) => {
                // Selection and catalogue projection can request visibility
                // again while the mode remains active. This presentation
                // policy runs after UI projection and remains authoritative.
                current_visibility.set_if_neq(Visibility::Hidden);
            }
            (false, None) => {}
        }
    }
}

pub(super) fn restore_ingame_hud_visibility_when_leaving_gameplay(
    mut hidden_hud_roots: Query<(Entity, &mut Visibility, &ModeHiddenHud)>,
    mut commands: Commands,
) {
    for (ui_document_root_entity, mut visibility, previous_mode_hidden_hud) in &mut hidden_hud_roots
    {
        *visibility = previous_mode_hidden_hud.previous;
        commands
            .entity(ui_document_root_entity)
            .remove::<ModeHiddenHud>();
    }
}
