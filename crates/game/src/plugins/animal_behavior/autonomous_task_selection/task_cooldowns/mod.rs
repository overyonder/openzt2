//! Authored task delays. A delayed task can't be selected again by the same
//! animal until its sampled delay has passed after the task ended.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::{
    behavior_task_execution_types::{BehaviorTaskFailed, BehaviorTaskFinished},
    simulation_time::simulation_clock_types::ZooClock,
};

/// Delayed tasks this animal has ended, and the tick each becomes selectable.
#[derive(Component, Debug, Default)]
pub(in crate::plugins::animal_behavior) struct AnimalTaskCooldowns {
    ready_ticks_by_task: Vec<(AssetId, u64)>,
}

impl AnimalTaskCooldowns {
    pub(super) fn is_cooling_down(&self, task: AssetId, tick: u64) -> bool {
        self.ready_ticks_by_task
            .iter()
            .any(|(cooling_task, ready_tick)| *cooling_task == task && *ready_tick > tick)
    }

    pub(super) fn start(&mut self, task: AssetId, tick: u64, delay_ticks: u64) {
        self.ready_ticks_by_task
            .retain(|(cooling_task, ready_tick)| *cooling_task != task && *ready_tick > tick);
        self.ready_ticks_by_task
            .push((task, tick.saturating_add(delay_ticks)));
    }
}

/// The delayed task an animal is running, with the delay sampled when it started.
#[derive(Component, Debug, Clone, Copy)]
pub(in crate::plugins::animal_behavior) struct RunningDelayedAnimalTask {
    pub(super) execution_id: u64,
    pub(super) task: AssetId,
    pub(super) delay_ticks: u64,
}

/// Samples one authored delay range in whole fixed ticks.
pub(super) fn sample_task_delay_ticks(delay_seconds: [f32; 2], unit: f32, fixed_hz: u16) -> u64 {
    let [minimum, maximum] = delay_seconds;
    let seconds = (maximum - minimum).mul_add(unit, minimum);
    let ticks = (f64::from(seconds) * f64::from(fixed_hz)).ceil();
    // Lowering accepts only finite, non-negative ranges.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks = ticks as u64;
    ticks
}

/// Starts each delayed task's cooldown when its execution ends, either way.
pub(in crate::plugins::animal_behavior) fn start_animal_task_cooldowns_after_delayed_tasks_end(
    clock: Res<ZooClock>,
    mut finished: MessageReader<BehaviorTaskFinished>,
    mut failed: MessageReader<BehaviorTaskFailed>,
    mut animals: Query<(&RunningDelayedAnimalTask, Option<&mut AnimalTaskCooldowns>)>,
    mut commands: Commands,
) {
    let ended = finished
        .read()
        .map(|message| (message.actor, message.execution_id))
        .chain(
            failed
                .read()
                .map(|message| (message.actor, message.execution_id)),
        );
    for (actor, execution_id) in ended {
        let Ok((running, cooldowns)) = animals.get_mut(actor) else {
            continue;
        };
        if running.execution_id != execution_id {
            continue;
        }
        let running = *running;
        if let Some(mut cooldowns) = cooldowns {
            cooldowns.start(running.task, clock.tick, running.delay_ticks);
        } else {
            let mut cooldowns = AnimalTaskCooldowns::default();
            cooldowns.start(running.task, clock.tick, running.delay_ticks);
            commands.entity(actor).insert(cooldowns);
        }
        commands.entity(actor).remove::<RunningDelayedAnimalTask>();
    }
}
