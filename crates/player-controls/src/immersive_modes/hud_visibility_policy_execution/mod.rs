use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use super::immersive_mode_policy_types::{HideHudWhileActive, ModeHiddenHud};
use super::immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode};

/// Records a member of the composed HUD hidden for an immersive mode.
#[derive(Component)]
pub(super) struct ModeHiddenHudChild {
    previous: Visibility,
}

const ALL_GAME_MODE_HUD_LAYOUTS: [&str; 3] = ["Camera Set", "Message bar layout", "pause layout"];

/// Hides the HUD during immersive mode and restores its previous visibility on exit.
pub(super) fn hide_ingame_hud_while_requested_immersive_mode_is_active_and_restore_visibility(
    controllers_requesting_hidden_hud: Query<(), With<HideHudWhileActive>>,
    active_modes: Query<&ActiveImmersiveMode>,
    children: Query<&Children>,
    mut hud_children: Query<
        (Entity, &Name, &mut Visibility, Option<&ModeHiddenHudChild>),
        Without<UiDocumentRoot>,
    >,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    mut ui_document_roots: Query<(
        Entity,
        &UiDocumentRoot,
        &mut Visibility,
        Option<&ModeHiddenHud>,
    )>,
    mut commands: Commands,
) {
    let immersive_mode_requests_hidden_hud = !controllers_requesting_hidden_hud.is_empty()
        || active_modes.iter().any(|active| {
            matches!(
                active.mode,
                ImmersiveMode::FirstPerson | ImmersiveMode::SuperStaff
            )
        });
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
        // Only `shell.xml` belongs to overhead mode. The camera buttons, messages
        // and pause display composed into this document stay in every game mode.
        if role == UiDocumentRole::InGameHud {
            let members = children.get(ui_document_root_entity).into_iter().flatten();
            for member in members.copied() {
                let Ok((entity, name, mut visibility, hidden)) = hud_children.get_mut(member) else {
                    continue;
                };
                if ALL_GAME_MODE_HUD_LAYOUTS
                    .iter()
                    .any(|layout| name.as_str().eq_ignore_ascii_case(layout))
                {
                    continue;
                }
                match (immersive_mode_requests_hidden_hud, hidden) {
                    (true, None) => {
                        commands
                            .entity(entity)
                            .insert(ModeHiddenHudChild { previous: *visibility });
                        visibility.set_if_neq(Visibility::Hidden);
                    }
                    (true, Some(_)) => {
                        visibility.set_if_neq(Visibility::Hidden);
                    }
                    (false, Some(hidden)) => {
                        visibility.set_if_neq(hidden.previous);
                        commands.entity(entity).remove::<ModeHiddenHudChild>();
                    }
                    (false, None) => {}
                }
            }
            continue;
        }
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
