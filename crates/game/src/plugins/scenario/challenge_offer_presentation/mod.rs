use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::{
    assets::localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    plugins::ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        localized_ui_text_writing::write_localized_ui_text,
        ui_document_lifecycle_contracts::ShowUiRole,
    },
};

use super::challenge_offer_types::{
    ScenarioChallengeOffer, ScenarioChallengeOfferState, ScenarioChallengePanelRequest,
    ScenarioChallengePanelText,
};

pub(super) fn apply_authored_challenge_panel_requests(
    mut commands: Commands,
    mut requests: MessageReader<ScenarioChallengePanelRequest>,
    offers: Query<(Entity, &ScenarioChallengeOffer)>,
    mut show: MessageWriter<ShowUiRole>,
) {
    for request in requests.read() {
        let Some((offer, _)) = offers.iter().find(|(_, offer)| {
            offer.definition == request.scenario
                && offer.state == ScenarioChallengeOfferState::Offered
        }) else {
            continue;
        };
        if let Some(text) = request.text {
            commands
                .entity(offer)
                .insert(ScenarioChallengePanelText(text));
        }
        if request.show {
            // Each dialog belongs to the live offer. This relation also scopes
            // native accept/decline and releases the dialog with its offer.
            show.write(ShowUiRole {
                role: UiDocumentRole::ChallengeOffer,
                owner: offer,
            });
        }
    }
}

pub(super) fn project_authored_challenge_panel_text(
    offers: Query<&ScenarioChallengePanelText>,
    parents: Query<&ChildOf>,
    localization: Res<LocalizationPrecedenceIndex>,
    assets: Res<Assets<LocalizationAsset>>,
    mut texts: Query<(&Name, &UiDocumentOwner, &mut Text)>,
) {
    let Some(localization) = localization.borrow_loaded_localization_view(&assets) else {
        return;
    };
    for (name, owner, mut text) in &mut texts {
        if name.as_str() != "challenge text" {
            continue;
        }
        let Ok(parent) = parents.get(owner.0) else {
            continue;
        };
        let Ok(key) = offers.get(parent.parent()) else {
            continue;
        };
        let _ = write_localized_ui_text(localization, key.0, &[], &mut text.0);
    }
}
