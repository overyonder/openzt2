use bevy::{ecs::system::Command, prelude::*};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_game_data::behavior::action::token::BehaviorTokenRecipient;
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorTaskReplaced, RunBehaviorSendSelfTokens, RunningBehaviorInstruction,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
    animal_health::types::Dead,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, BehaviorTaskFailed, PendingBehaviorTaskFailure,
        abandon_behavior_task_for_reconsideration,
    },
    guests::guest_simulation_types::GuestRng,
    staff::staff_job_types::CurrentJob,
};

use openzt2_zoo_state::animal_behavior::behavior_token_types::{
    BehaviorSelfToken, BehaviorTokenExpires, ClaimedBehaviorSelfToken,
};

#[derive(Component)]
struct EndedBehaviorTokenClaim;

use super::behavior_spatial_candidates::BehaviorSpatialCandidates;

fn release_claimed_token_when_task_is_discarded(
    discarded: On<Discard<BehaviorTaskExecutionState>>,
    tasks: Query<&BehaviorTaskExecutionState>,
    claims: Query<(&ClaimedBehaviorSelfToken, &ChildOf)>,
    mut commands: Commands,
) {
    let Ok(task) = tasks.get(discarded.entity) else {
        return;
    };
    let Some(token) = task.object else {
        return;
    };
    if claims.get(token).is_ok_and(|(claim, owner)| {
        owner.parent() == discarded.entity && claim.execution_id == task.execution_id
    }) {
        commands.entity(token).try_insert(EndedBehaviorTokenClaim);
    }
}

fn release_claimed_token_when_task_is_replaced(
    replaced: On<BehaviorTaskReplaced>,
    claims: Query<(&ClaimedBehaviorSelfToken, &ChildOf)>,
    mut commands: Commands,
) {
    let Some(token) = replaced.previous_object else {
        return;
    };
    if claims.get(token).is_ok_and(|(claim, owner)| {
        owner.parent() == replaced.actor && claim.execution_id == replaced.previous_execution
    }) {
        commands.entity(token).try_insert(EndedBehaviorTokenClaim);
    }
}

fn discard_expired_and_consumed_tokens(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    tokens: Query<&BehaviorSelfToken>,
    expiring: Query<
        (Entity, &BehaviorSelfToken),
        (
            With<BehaviorTokenExpires>,
            Without<ClaimedBehaviorSelfToken>,
        ),
    >,
    released: Query<
        (
            Entity,
            &BehaviorSelfToken,
            &ClaimedBehaviorSelfToken,
            &ChildOf,
        ),
        With<EndedBehaviorTokenClaim>,
    >,
    dead_recipients: Query<&Children, Added<Dead>>,
    tasks: Query<&BehaviorTaskExecutionState>,
    mut failures: MessageReader<BehaviorTaskFailed>,
) {
    let failures = failures.read().copied().collect::<Vec<_>>();
    for (entity, token, claim, recipient) in &released {
        if token.retain_on_failure
            && failures.iter().any(|failure| {
                failure.actor == recipient.parent() && failure.execution_id == claim.execution_id
            })
        {
            commands
                .entity(entity)
                .try_remove::<(ClaimedBehaviorSelfToken, EndedBehaviorTokenClaim)>();
        } else if tasks
            .get(recipient.parent())
            .is_ok_and(|task| task.execution_id == claim.execution_id)
        {
            commands
                .entity(entity)
                .try_remove::<EndedBehaviorTokenClaim>();
        } else {
            commands.entity(entity).try_despawn();
        }
    }
    for (entity, token) in &expiring {
        if clock.tick >= token.expires_at {
            commands.entity(entity).try_despawn();
        }
    }
    for children in &dead_recipients {
        for token in children
            .into_iter()
            .copied()
            .filter(|&entity| tokens.contains(entity))
        {
            commands.entity(token).try_despawn();
        }
    }
}

pub(super) struct DeliverBehaviorSelfToken {
    pub(super) actor: Entity,
    pub(super) recipient: Entity,
    pub(super) only_one: bool,
    pub(super) reconsider: bool,
    pub(super) tick: u64,
    pub(super) token: BehaviorSelfToken,
}

impl Command for DeliverBehaviorSelfToken {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Ok(recipient) = world.get_entity(self.recipient) else {
            return;
        };
        if recipient.contains::<Dead>() {
            return;
        }
        if self.only_one
            && recipient
                .get::<Children>()
                .into_iter()
                .flatten()
                .any(|&entity| {
                    world.get::<BehaviorSelfToken>(entity).is_some_and(|held| {
                        held.name == self.token.name && self.tick < held.expires_at
                    })
                })
        {
            return;
        }
        // A sender continues its task. Staff finish their claimed jobs first.
        if self.reconsider
            && self.actor != self.recipient
            && !recipient.contains::<CurrentJob>()
            && recipient.contains::<BehaviorTaskExecutionState>()
        {
            abandon_behavior_task_for_reconsideration(self.recipient, &mut world.commands());
        }
        world.spawn((self.token, ChildOf(self.recipient)));
    }
}

pub(super) fn register_token_execution(application: &mut App) {
    application
        .add_observer(release_claimed_token_when_task_is_discarded)
        .add_observer(release_claimed_token_when_task_is_replaced)
        .add_systems(
            FixedUpdate,
            discard_expired_and_consumed_tokens
                .before(super::autonomous_task_selection::select_idle_animal_tasks)
                .in_set(FixedGameSet::Think)
                .run_if(in_state(GamePhase::InGame)),
        )
        .add_observer(execute_current_self_token_deliveries);
}

fn execute_current_self_token_deliveries(
    activation: On<WhenBehaviorAction<RunBehaviorSendSelfTokens>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut actors: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorSendSelfTokens,
            ),
            Option<&mut BehaviorRandomStream>,
            Option<&mut GuestRng>,
        ),
        (Without<Dead>, Without<PendingBehaviorTaskFailure>),
    >,
    recipients: Query<(), Without<Dead>>,
    spatial: BehaviorSpatialCandidates,
    mut candidates: Local<Vec<Entity>>,
    animals: Query<(Entity, &GlobalTransform, &SpeciesHandle), (With<Animal>, Without<Dead>)>,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    world_members: Query<&openzt2_world_state::world_spawn::world_membership_types::WorldMember>,
    staff: Query<
        (Entity, &GlobalTransform),
        With<openzt2_zoo_state::staff::staff_employment_types::Staff>,
    >,
) {
    let Some(definitions) = definitions.get(&definition_assets) else {
        return;
    };
    for (actor, (task, instruction, running_action), mut random, mut guest_random) in
        std::iter::once(actors.get_mut(activation.entity)).flatten()
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let deliveries = &running_action.0;
        if deliveries
            .iter()
            .flat_map(|delivery| {
                [
                    delivery.chance_percent,
                    delivery
                        .nearby
                        .map_or(100.0, |nearby| nearby.chance_percent),
                ]
            })
            .any(|chance| chance > 0.0 && chance < 100.0)
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
                let ticks = duration.as_nanos().div_ceil(
                    definitions
                        .simulation_timestep()
                        .expect("simulation timing is loaded")
                        .as_nanos(),
                );
                clock
                    .tick
                    .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX))
            };
            let mut delivered_to = Vec::new();
            if let Some(nearby) = delivery.nearby {
                let Ok((_, origin, _)) = animals.get(actor) else {
                    continue;
                };
                let Ok(world) = world_members.get(actor) else {
                    continue;
                };
                let position = origin.translation();
                let margin = Vec2::splat(nearby.radius_m.abs());
                spatial.in_bounds(
                    world.root,
                    position.xz() - margin,
                    position.xz() + margin,
                    &mut candidates,
                );
                spatial.preserve_query_order(&mut candidates);
                for (animal, transform, species) in
                    animals.iter_many(candidates.iter().copied()).flatten()
                {
                    let is_type = animal_types
                        .borrow_for(animal)
                        .contains(&nearby.entity_type)
                        || species.species == nearby.entity_type
                        || nearby.entity_type == AssetId::from_key("animal");
                    if animal == actor
                        || !is_type
                        || transform.translation().distance(origin.translation()) > nearby.radius_m
                    {
                        continue;
                    }
                    let drawn = nearby.chance_percent >= 100.0
                        || random.as_deref_mut().is_some_and(|random| {
                            random.0.unit_f32() * 100.0 < nearby.chance_percent
                        });
                    if drawn {
                        delivered_to.push(animal);
                    }
                }
                // An attacking animal warns staff nearby (`GiveTo="Staff"`).
                if nearby.entity_type == AssetId::from_key("staff") {
                    for (member, transform) in staff.iter_many(candidates.iter().copied()).flatten()
                    {
                        if transform.translation().distance(origin.translation()) > nearby.radius_m
                        {
                            continue;
                        }
                        let drawn = nearby.chance_percent >= 100.0
                            || random.as_deref_mut().is_some_and(|random| {
                                random.0.unit_f32() * 100.0 < nearby.chance_percent
                            });
                        if drawn {
                            delivered_to.push(member);
                        }
                    }
                }
            } else {
                delivered_to.push(match delivery.recipient {
                    BehaviorTokenRecipient::Subject => actor,
                    BehaviorTokenRecipient::Target => {
                        let Some(target) =
                            task.target().filter(|target| recipients.contains(*target))
                        else {
                            continue;
                        };
                        target
                    }
                });
            }
            let payload = match delivery.payload {
                None => None,
                Some(BehaviorTokenRecipient::Subject) => Some(actor),
                Some(BehaviorTokenRecipient::Target) => {
                    let Some(target) = task.target() else {
                        continue;
                    };
                    Some(target)
                }
            };
            for recipient in delivered_to {
                commands.queue(DeliverBehaviorSelfToken {
                    actor,
                    recipient,
                    only_one: delivery.only_one,
                    reconsider: delivery.reconsider,
                    tick: clock.tick,
                    token: BehaviorSelfToken {
                        name: delivery.token,
                        available_at: at_tick(delivery.delay),
                        expires_at: delivery.lifetime.map_or(u64::MAX, at_tick),
                        payload,
                        retain_on_failure: delivery.retain_on_failure,
                    },
                });
            }
        }
        instruction.complete(actor, clock.tick, &mut commands);
    }
}
