use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;
use openzt2_ui::localized_ui_text_writing::write_localized_ui_text;

use super::challenge_offer_types::{
    ReopenRestoredScenarioChallengePanel, ScenarioChallengeOffer, ScenarioChallengeOfferState,
    ScenarioChallengePanelDescription, ScenarioChallengePanelRequest,
    ScenarioChallengePanelSurface, ScenarioChallengePanelText,
};

pub(super) fn apply_authored_challenge_panel_requests(
    mut commands: Commands,
    mut requests: MessageReader<ScenarioChallengePanelRequest>,
    mut offers: Query<(
        Entity,
        &ScenarioChallengeOffer,
        Option<&mut ScenarioChallengePanelText>,
        Option<&mut ScenarioChallengePanelDescription>,
        Option<&mut ScenarioChallengePanelSurface>,
    )>,
    objectives: Query<&openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveOf>,
    mut show: MessageWriter<ShowUiRole>,
) {
    for request in requests.read() {
        let source_owner = request
            .source
            .and_then(|source| objectives.get(source).ok())
            .map(|owner| owner.0);
        let Some((offer, _, existing_text, existing_description, existing_surface)) =
            offers.iter_mut().find(|(entity, offer, ..)| {
                if source_owner.is_some_and(|owner| owner != *entity) {
                    return false;
                }
                offer.definition == request.scenario
                    && offer.state == ScenarioChallengeOfferState::Offered
            })
        else {
            continue;
        };
        if let Some(text) = request.text {
            if let Some(mut current) = existing_text {
                if current.0 != text {
                    current.0 = text;
                }
            } else {
                commands
                    .entity(offer)
                    .insert(ScenarioChallengePanelText(text));
            }
        }
        if let Some(description) = request.description {
            if let Some(mut current) = existing_description {
                if current.0 != description {
                    current.0 = description;
                }
            } else {
                commands
                    .entity(offer)
                    .insert(ScenarioChallengePanelDescription(description));
            }
        }
        if let Some(surface) = request.show {
            if let Some(mut current) = existing_surface {
                current.set_if_neq(surface);
            } else {
                commands.entity(offer).insert(surface);
            }
            info!(?surface, ?offer, "challenge offer dialog shown");
            // Each dialog belongs to the live offer. This relation also scopes
            // native accept/decline and releases the dialog with its offer.
            show.write(ShowUiRole {
                role: panel_role(surface),
                owner: offer,
            });
        }
    }
}

pub(super) fn reopen_restored_challenge_panel(
    mut commands: Commands,
    offers: Query<
        (Entity, &ScenarioChallengePanelSurface),
        With<ReopenRestoredScenarioChallengePanel>,
    >,
    mut show: MessageWriter<ShowUiRole>,
) {
    for (offer, surface) in &offers {
        show.write(ShowUiRole {
            role: panel_role(*surface),
            owner: offer,
        });
        commands
            .entity(offer)
            .remove::<ReopenRestoredScenarioChallengePanel>();
    }
}

fn panel_role(surface: ScenarioChallengePanelSurface) -> UiDocumentRole {
    match surface {
        ScenarioChallengePanelSurface::Offer => UiDocumentRole::ChallengeOffer,
        ScenarioChallengePanelSurface::InstantOffer => UiDocumentRole::InstantChallengeOffer,
    }
}

pub(super) fn project_authored_challenge_panel_text(
    offers: Query<(
        Option<&ScenarioChallengePanelText>,
        Option<&ScenarioChallengePanelDescription>,
    )>,
    parents: Query<&ChildOf>,
    localization: Res<LocalizationPrecedenceIndex>,
    assets: Res<Assets<LocalizationAsset>>,
    mut texts: Query<(&Name, &UiDocumentOwner, &mut Text)>,
) {
    let Some(localization) = localization.borrow_loaded_localization_view(&assets) else {
        return;
    };
    for (name, owner, mut text) in &mut texts {
        let describes = match name.as_str() {
            "challenge text" | "instant challenge text" => false,
            "instant challenge description text" => true,
            _ => continue,
        };
        let Ok(parent) = parents.get(owner.0) else {
            continue;
        };
        let Ok((challenge_text, description)) = offers.get(parent.parent()) else {
            continue;
        };
        let key = if describes {
            description.map(|description| description.0)
        } else {
            challenge_text.map(|challenge_text| challenge_text.0)
        };
        if let Some(key) = key {
            let _ = write_localized_ui_text(localization, key, &[], &mut text.0);
        }
    }
}
