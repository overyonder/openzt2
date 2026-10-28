use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::behavior::{
    action::token::BehaviorTokenRecipient, action_record::BehaviorAction,
};
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
    animal_health::types::Dead,
    behavior_task_execution_types::{
        abandon_behavior_task_for_reconsideration, advance_behavior_task_to_next_action,
        find_current_behavior_task_action, BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    guests::guest_simulation_types::GuestRng,
};

use openzt2_zoo_state::animal_behavior::behavior_token_types::{
    BehaviorSelfToken, ClaimedBehaviorSelfToken,
};

pub(super) fn register_token_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        (
            discard_expired_and_consumed_tokens,
            execute_current_self_token_deliveries,
        )
            .chain()
            .before(super::autonomous_task_selection::select_idle_animal_tasks)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn discard_expired_and_consumed_tokens(
    mut commands: Commands,
    clock: Res<ZooClock>,
    tokens: Query<(
        Entity,
        &BehaviorSelfToken,
        &ChildOf,
        Option<&ClaimedBehaviorSelfToken>,
    )>,
    recipients: Query<Option<&BehaviorTaskExecutionState>, Without<Dead>>,
) {
    for (entity, token, recipient, claim) in &tokens {
        let Ok(task) = recipients.get(recipient.parent()) else {
            commands.entity(entity).despawn();
            continue;
        };
        // A selected token remains the task's object until that execution ends.
        // Both completion and failure consume it; retain-on-failure deliveries
        // are a distinct policy which the loader does not accept here.
        let expired_or_consumed = claim.map_or(clock.tick >= token.expires_at, |claim| {
            task.is_none_or(|task| task.execution_id != claim.execution_id)
        });
        if expired_or_consumed {
            commands.entity(entity).despawn();
        }
    }
}

fn execute_current_self_token_deliveries(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&mut BehaviorRandomStream>,
            Option<&mut GuestRng>,
        ),
        (Without<Dead>, Without<PendingBehaviorTaskFailure>),
    >,
    recipients: Query<Has<BehaviorTaskExecutionState>, Without<Dead>>,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    for (actor, mut task, mut random, mut guest_random) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::SendSelfTokens(deliveries)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        if deliveries
            .iter()
            .any(|delivery| delivery.chance_percent > 0.0 && delivery.chance_percent < 100.0)
            && random.is_none()
            && guest_random.is_none()
        {
            continue;
        }
        for delivery in deliveries {
            if delivery.chance_percent <= 0.0
                || (delivery.chance_percent < 100.0
                    && random
                        .as_deref_mut()
                        .map(|random| random.0.unit_f32())
                        .or_else(|| {
                            guest_random
                                .as_deref_mut()
                                .map(|random| random.0.unit_f32())
                        })
                        .is_none_or(|draw| draw * 100.0 >= delivery.chance_percent))
            {
                continue;
            }
            let at_tick = |duration: std::time::Duration| {
                let ticks = duration
                    .as_nanos()
                    .saturating_mul(u128::from(definitions.timing().fixed_hz))
                    .div_ceil(1_000_000_000);
                clock
                    .tick
                    .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX))
            };
            let recipient = match delivery.recipient {
                BehaviorTokenRecipient::Subject => actor,
                BehaviorTokenRecipient::Target => {
                    let Some(target) = task.target().filter(|target| recipients.contains(*target))
                    else {
                        continue;
                    };
                    target
                }
            };
            // Reconsideration drops the recipient's current task before it
            // chooses again with the delivered token.
            if delivery.reconsider && recipients.get(recipient).is_ok_and(|busy| busy) {
                abandon_behavior_task_for_reconsideration(recipient, &mut commands);
            }
            commands.spawn((
                BehaviorSelfToken {
                    name: delivery.token,
                    available_at: at_tick(delivery.delay),
                    expires_at: delivery.lifetime.map_or(u64::MAX, at_tick),
                },
                ChildOf(recipient),
            ));
        }
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}
