use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    challenge_offer_types::{
        CreateScenarioChallengeOfferFromLuaSourcePathRequest, RespondToScenarioChallengeRequest,
        ScenarioChallengeAcceptanceRejected, ScenarioChallengeAcceptanceRejectionReason,
        ScenarioChallengeAccepted, ScenarioChallengeOffer, ScenarioChallengeOfferState,
    },
    scenario_instantiation::spawn_authored_challenge_objectives,
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
};

pub(super) fn create_challenge_offers_requested_by_scenario_scripts(
    mut commands: Commands,
    mut requests: MessageReader<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
    roots: Query<Entity, With<WorldRoot>>,
    existing: Query<(), With<ScenarioChallengeOffer>>,
    scenarios: Res<WorldScenarios>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut allocator: ResMut<PersistentIdAllocator>,
    clock: Res<ZooClock>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    let Some(scenarios) = scenarios.get(&assets) else {
        return;
    };
    for request in requests.read() {
        if !existing.is_empty() {
            continue;
        }
        let Some(stem) = std::path::Path::new(&request.source_path)
            .file_stem()
            .and_then(|stem| stem.to_str())
        else {
            error!(path = %request.source_path, "Lua challenge offer has no UTF-8 scenario stem");
            continue;
        };
        let source_id = AssetId::from_key(stem);
        let Some(record) = scenarios.scenario(source_id) else {
            error!(path = %request.source_path, "Lua challenge offer references an unloaded scenario");
            continue;
        };
        let Some(asset) = scenarios.document_for_scenario(source_id) else {
            continue;
        };
        let required_ids = u64::try_from(record.objectives.len())
            .ok()
            .and_then(|count| count.checked_add(1));
        if !allocator.owns_world(root)
            || required_ids
                .and_then(|count| allocator.next().checked_add(count))
                .is_none()
        {
            error!(path = %request.source_path, "Lua challenge offer cannot allocate objective identities");
            continue;
        }
        if let Err(error) = runtime.clear_finished_challenge_rule_state(record.id) {
            error!(%error, "new challenge could not initialize its rule state");
            continue;
        }
        let Ok(id) = allocator.allocate(root) else {
            continue;
        };
        // BFS_ADDSCENARIO starts its rules now. Their authored Evaluate Lua
        // opens the offer and waits for the player's response; deferring rules
        // until acceptance prevents that dialog from ever being requested.
        if !spawn_authored_challenge_objectives(
            &mut commands,
            &mut allocator,
            root,
            record.id,
            &asset.document,
            clock.tick,
        ) {
            continue;
        }
        commands.spawn((
            WorldMember { root },
            id,
            ScenarioChallengeOffer {
                definition: record.id,
                expires_tick: u64::MAX,
                state: ScenarioChallengeOfferState::Offered,
            },
        ));
        break;
    }
}

pub(super) fn validate_and_apply_requested_challenge_responses(
    mut requests: MessageReader<RespondToScenarioChallengeRequest>,
    clock: Res<ZooClock>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    mut offers: Query<&mut ScenarioChallengeOffer>,
    mut accepted: MessageWriter<ScenarioChallengeAccepted>,
    mut rejected: MessageWriter<ScenarioChallengeAcceptanceRejected>,
) {
    for request in requests.read() {
        let reason = match offers.get_mut(request.offer) {
            Err(_) => Some(ScenarioChallengeAcceptanceRejectionReason::Missing),
            Ok(offer) if offer.state != ScenarioChallengeOfferState::Offered => {
                Some(ScenarioChallengeAcceptanceRejectionReason::AlreadyActive)
            }
            Ok(offer) if clock.tick >= offer.expires_tick => {
                Some(ScenarioChallengeAcceptanceRejectionReason::Expired)
            }
            Ok(mut offer) => {
                match runtime.set_challenge_response(offer.definition, request.accept) {
                    Ok(()) => {
                        offer.state = if request.accept {
                            ScenarioChallengeOfferState::Active
                        } else {
                            ScenarioChallengeOfferState::Declined
                        };
                        if request.accept {
                            accepted.write(ScenarioChallengeAccepted {
                                offer: request.offer,
                                definition: offer.definition,
                            });
                        }
                        None
                    }
                    Err(error) => {
                        error!(%error, "challenge response could not reach its authored Lua");
                        Some(ScenarioChallengeAcceptanceRejectionReason::Ineligible)
                    }
                }
            }
        };
        if let Some(reason) = reason {
            rejected.write(ScenarioChallengeAcceptanceRejected {
                offer: request.offer,
                reason,
            });
        }
    }
}
