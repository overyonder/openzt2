use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_zoo_state::scenario::{
    scenario_script_timing_types::ScenarioScriptTiming, scenario_session_types::ScenarioDocument,
};

use super::{
    challenge_offer_types::{
        CreateScenarioChallengeOfferFromLuaSourcePathRequest, RespondToScenarioChallengeRequest,
        ScenarioChallengeAcceptanceRejected, ScenarioChallengeAcceptanceRejectionReason,
        ScenarioChallengeAccepted, ScenarioChallengeLuaCandidate, ScenarioChallengeOffer,
        ScenarioChallengeOfferState, ScenarioChallengeRuleContext,
    },
    scenario_instantiation::spawn_authored_challenge_objectives,
    scenario_lua_types::ScenarioLuaContext,
};

pub(super) fn create_challenge_offers_requested_by_scenario_scripts(
    mut commands: Commands,
    mut requests: MessageReader<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
    mut roots: Query<(Entity, &ZooClock, &mut PersistentIdAllocator), With<WorldRoot>>,
    existing: Query<(), With<ScenarioChallengeOffer>>,
    contexts: Query<(Entity, &ScenarioChallengeLuaCandidate, &WorldMember)>,
    scenarios: Res<WorldScenarios>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut runtime: Query<&mut ScenarioLuaContext>,
) {
    let Ok((root, clock, mut allocator)) = roots.single_mut() else {
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
        // Archive paths are case-insensitive and scenarios are keyed by their
        // lowercase stem; scripts name `BelugaPhoto.xml` and `mantaray.xml`
        // alike.
        let source_id = AssetId::from_key(&stem.to_ascii_lowercase());
        let Some(record) = scenarios.scenario(source_id) else {
            error!(path = %request.source_path, "Lua challenge offer references an unloaded scenario");
            continue;
        };
        let Some(handle) = scenarios.handle_for_scenario(source_id) else {
            continue;
        };
        let Some(asset) = assets.get(handle) else {
            continue;
        };
        let required_ids = u64::try_from(record.objectives.len())
            .ok()
            .and_then(|count| count.checked_add(1));
        if required_ids
            .and_then(|count| allocator.next().checked_add(count))
            .is_none()
        {
            error!(path = %request.source_path, "Lua challenge offer cannot allocate objective identities");
            continue;
        }
        let owner = contexts
            .iter()
            .find(|(_, candidate, member)| candidate.0 == record.id && member.root == root)
            .map(|(entity, _, _)| entity)
            .unwrap_or_else(|| {
                commands
                    .spawn((
                        WorldMember { root },
                        ScenarioChallengeLuaCandidate(record.id),
                        ScenarioDocument(handle.clone()),
                    ))
                    .id()
            });
        if let Err(error) = runtime.get_mut(owner).map_or(Ok(()), |mut context| {
            context.clear_finished_challenge_rule_state()
        }) {
            error!(%error, "new challenge could not initialize its rule state");
            continue;
        }
        let Ok(id) = allocator.allocate() else {
            continue;
        };
        let offer = commands
            .spawn((
                WorldMember { root },
                id,
                ScenarioDocument(handle.clone()),
                ScenarioChallengeRuleContext(owner),
                ScenarioScriptTiming::default(),
                ScenarioChallengeOffer {
                    definition: record.id,
                    expires_tick: u64::MAX,
                    state: ScenarioChallengeOfferState::Offered,
                },
            ))
            .id();
        // BFS_ADDSCENARIO starts its rules now. Their authored Evaluate Lua
        // opens the offer and waits for the player's response; deferring rules
        // until acceptance prevents that dialog from ever being requested.
        if !spawn_authored_challenge_objectives(
            &mut commands,
            &mut allocator,
            root,
            offer,
            record.id,
            asset,
            clock.tick,
        ) {
            commands.entity(offer).despawn();
            continue;
        }
        info!(path = %request.source_path, tick = clock.tick, "challenge offered");
        break;
    }
}

pub(super) fn validate_and_apply_requested_challenge_responses(
    mut requests: MessageReader<RespondToScenarioChallengeRequest>,
    clocks: Query<&ZooClock>,
    mut runtime: Query<&mut ScenarioLuaContext>,
    mut offers: Query<(
        &mut ScenarioChallengeOffer,
        &ScenarioChallengeRuleContext,
        &WorldMember,
    )>,
    mut accepted: MessageWriter<ScenarioChallengeAccepted>,
    mut rejected: MessageWriter<ScenarioChallengeAcceptanceRejected>,
) {
    for request in requests.read() {
        let reason = match offers.get_mut(request.offer) {
            Err(_) => Some(ScenarioChallengeAcceptanceRejectionReason::Missing),
            Ok((offer, _, _)) if offer.state != ScenarioChallengeOfferState::Offered => {
                Some(ScenarioChallengeAcceptanceRejectionReason::AlreadyActive)
            }
            Ok((offer, _, member))
                if clocks
                    .get(member.root)
                    .is_ok_and(|clock| clock.tick >= offer.expires_tick) =>
            {
                Some(ScenarioChallengeAcceptanceRejectionReason::Expired)
            }
            Ok((mut offer, owner, _)) => {
                let response = runtime
                    .get_mut(owner.0)
                    .map_err(|_| mlua::Error::runtime("challenge has no running Lua context"))
                    .and_then(|mut context| context.set_challenge_response(request.accept));
                match response {
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

pub(super) fn project_award_program_suspension(
    mut commands: Commands,
    worlds: Query<
        (
            Entity,
            Has<openzt2_progression::progression::AwardProgramSuspended>,
        ),
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
    offers: Query<&WorldMember, With<ScenarioChallengeOffer>>,
    results: Query<
        &InheritedVisibility,
        With<super::scenario_lua_ui::presentation::ChallengeResultPanel>,
    >,
) {
    use openzt2_progression::progression::AwardProgramSuspended;
    let photo_or_scenario_result_visible = results.iter().any(|visibility| visibility.get());
    for (root, suspended) in &worlds {
        let needed =
            photo_or_scenario_result_visible || offers.iter().any(|offer| offer.root == root);
        match (suspended, needed) {
            (false, true) => {
                commands.entity(root).insert(AwardProgramSuspended);
            }
            (true, false) => {
                commands.entity(root).remove::<AwardProgramSuspended>();
            }
            _ => {}
        }
    }
}
