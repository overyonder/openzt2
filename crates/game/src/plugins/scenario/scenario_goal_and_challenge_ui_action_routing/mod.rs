use bevy::prelude::*;
use openzt2_game_data::{ui_document::action::scenarios::UiScenarioAction, AssetId};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

use super::{
    challenge_offer_types::{
        RespondToScenarioChallengeRequest, ScenarioChallengeOffer, ScenarioChallengeOfferState,
    },
    scenario_ui_types::{
        AuthoredScenarioUiActionDocumentQueries, ScenarioObjectiveCategoryFilter,
        ScenarioObjectiveStatusFilter,
    },
};
pub(super) fn route_scenario_goal_and_challenge_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: AuthoredScenarioUiActionDocumentQueries,
    offers: Query<&ScenarioChallengeOffer>,
    parents: Query<&ChildOf>,
    mut challenge_acceptance: MessageWriter<RespondToScenarioChallengeRequest>,
) {
    for activation in activations.read() {
        let Ok((authored_actions, owner)) = nodes.action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = nodes.document_roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        for record in authored_actions.authored_action_records(document) {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiScenarioAction::SetObjectiveFilter { filter } => {
                    commands
                        .entity(owner.0)
                        .insert(ScenarioObjectiveCategoryFilter(AssetId(filter.0)));
                }
                UiScenarioAction::SetObjectiveStatusFilter { filter } => {
                    commands
                        .entity(owner.0)
                        .insert(ScenarioObjectiveStatusFilter::from(filter));
                }
                UiScenarioAction::AcceptChallenge | UiScenarioAction::DeclineChallenge => {
                    let Ok(parent) = parents.get(owner.0) else {
                        continue;
                    };
                    let offer_entity = parent.parent();
                    if offers
                        .get(offer_entity)
                        .is_ok_and(|offer| offer.state == ScenarioChallengeOfferState::Offered)
                    {
                        challenge_acceptance.write(RespondToScenarioChallengeRequest {
                            offer: offer_entity,
                            accept: matches!(&record.action, UiScenarioAction::AcceptChallenge),
                        });
                    }
                }
                _ => {}
            }
        }
    }
}
