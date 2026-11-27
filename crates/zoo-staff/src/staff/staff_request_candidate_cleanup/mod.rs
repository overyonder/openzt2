use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::timing::TimekeeperEvent;
use openzt2_world_state::simulation_time::simulation_clock_types::{ZooClock, ZooTimekeeperElapsed};
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::{WorldMember, WorldRoot},
};

use super::staff_job_types::{
    StaffJob, StaffJobTarget, StaffRequestResourceCoverage, UnsuccessfulStaffJobCandidates,
};
use super::staff_lifecycle_messages::CancelStaffJobRequest;
use openzt2_game_data::AssetId;

#[derive(Component)]
pub(super) struct StaffCandidateCleanupDeadline {
    next_tick: u64,
    last_tick: Option<u64>,
}

pub(super) fn project_staff_request_resource_coverage(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    areas: Query<&openzt2_zoo_state::habitat::habitat_types::HabitatMembers>,
    objects: Query<(
        &openzt2_world_state::world_spawn::world_membership_types::DefinitionId,
        Option<&openzt2_zoo_state::authored_float_attributes::AuthoredFloatAttributes>,
    )>,
    mut animals: Query<
        (
            Entity,
            Option<&openzt2_zoo_state::habitat::habitat_types::HabitatMember>,
            Option<&mut StaffRequestResourceCoverage>,
        ),
        With<openzt2_zoo_state::animal_lifecycle::types::Animal>,
    >,
    mut commands: Commands,
) {
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    let food = AssetId::from_key("b_food");
    let water = AssetId::from_key("b_water");
    for (animal, habitat, current) in &mut animals {
        let mut next = StaffRequestResourceCoverage::default();
        if let Some(members) = habitat.and_then(|habitat| areas.get(habitat.habitat_entity).ok()) {
            use bevy::ecs::relationship::RelationshipTarget;
            for (definition, attributes) in objects.iter_many(members.iter()).matched() {
                let authored = definitions.find_object(definition.0);
                let matches = |property| {
                    attributes
                        .and_then(|attributes| attributes.get(property))
                        .map_or_else(
                            || {
                                authored.is_some_and(|object| {
                                    object.behavior_type_identifiers.contains(&property)
                                })
                            },
                            |attribute| attribute != 0.0,
                        )
                };
                next.food |= matches(food);
                next.water |= matches(water);
                if next.food && next.water {
                    break;
                }
            }
        }
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else if next.food || next.water {
            commands.entity(animal).insert(next);
        }
    }
}

pub(super) fn clear_unsuccessful_candidates_at_authored_manager_interval(
    mut commands: Commands,
    mut elapsed_events: MessageReader<ZooTimekeeperElapsed>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut requests: Query<(
        Entity,
        &mut StaffJob,
        &WorldMember,
        Option<&mut UnsuccessfulStaffJobCandidates>,
        Option<&StaffJobTarget>,
    )>,
    coverage: Query<&StaffRequestResourceCoverage>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
    mut roots: Query<
        (&ZooClock, Option<&mut StaffCandidateCleanupDeadline>),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
) {
    let refresh = active_definitions.is_changed() || definitions.is_changed();
    let Some(catalogue) = active_definitions.get(&definitions) else {
        elapsed_events.clear();
        return;
    };
    let (Some(policy), Some(timing)) = (catalogue.staff_manager(), catalogue.simulation_timestep())
    else {
        elapsed_events.clear();
        return;
    };
    if refresh {
        for (_, deadline) in &mut roots {
            if let Some(mut deadline) = deadline {
                deadline.last_tick = None;
            }
        }
    }
    let interval_ticks =
        (f64::from(policy.bad_entity_cleanup_interval) / timing.as_secs_f64()).max(0.0) as u64;
    let mut elapsed_by_world = bevy::ecs::entity::EntityHashMap::<f32>::default();
    for event in elapsed_events
        .read()
        .filter(|event| event.event == TimekeeperEvent::Slow)
    {
        let Ok((clock, deadline)) = roots.get_mut(event.root) else {
            continue;
        };
        if clock.tick <= deadline.as_deref().map_or(0, |deadline| deadline.next_tick) {
            continue;
        }
        let last_tick = deadline.as_deref().and_then(|deadline| deadline.last_tick);
        let elapsed = last_tick.map_or(0.0, |last_tick| {
            (clock.tick.saturating_sub(last_tick) as f64 * timing.as_secs_f64()) as f32
        });
        let next = StaffCandidateCleanupDeadline {
            next_tick: clock.tick.saturating_add(interval_ticks),
            last_tick: Some(clock.tick),
        };
        if let Some(mut deadline) = deadline {
            *deadline = next;
        } else {
            commands.entity(event.root).insert(next);
        }
        elapsed_by_world.insert(event.root, elapsed);
    }
    if elapsed_by_world.is_empty() {
        return;
    }
    let feed = Some(AssetId::from_key("t_feedanimal"));
    let water = Some(AssetId::from_key("t_wateranimal"));
    for (entity, mut job, member, attempts, target) in &mut requests {
        let Some(&elapsed) = elapsed_by_world.get(&member.root) else {
            continue;
        };
        if elapsed > 0.0 && job.remaining_wait_seconds > 0.0 {
            job.advance_wait(elapsed);
        }
        if target
            .and_then(|target| coverage.get(target.0).ok())
            .is_some_and(|coverage| {
                (job.token == feed && coverage.food) || (job.token == water && coverage.water)
            })
        {
            cancellations.write(CancelStaffJobRequest { job: entity });
        } else if let Some(mut attempts) = attempts {
            attempts.clear();
        }
    }
}
