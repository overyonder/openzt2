//! Authored task delays. A delayed task can't be selected again by the same
//! animal until its sampled delay has passed after the task ended. It also
//! waits out a delay once it becomes possible for the animal, so a rampaging
//! ankylosaur runs around before it bashes its fence, as in `zt.exe`.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;

/// Delayed tasks this animal has ended, and the tick each becomes selectable.
#[derive(Component, Debug, Default)]
pub(crate) struct AnimalTaskCooldowns {
    ready_ticks_by_task: Vec<(AssetId, u64)>,
    /// Delayed tasks possible for the animal since it waited out their delay.
    armed: Vec<AssetId>,
}

impl AnimalTaskCooldowns {
    pub(super) fn is_cooling_down(&self, task: AssetId, tick: u64) -> bool {
        self.ready_ticks_by_task
            .iter()
            .any(|(cooling_task, ready_tick)| *cooling_task == task && *ready_tick > tick)
    }

    pub(super) fn is_armed(&self, task: AssetId) -> bool {
        self.armed.contains(&task)
    }

    /// The task has just become possible: it first waits out its delay.
    pub(super) fn arm(&mut self, task: AssetId, tick: u64, delay_ticks: u64) {
        self.armed.push(task);
        self.start(task, tick, delay_ticks);
    }

    /// The task is no longer possible; its next chance waits again.
    pub(super) fn disarm(&mut self, task: AssetId) {
        if self.is_armed(task) {
            self.armed.retain(|armed| *armed != task);
            self.ready_ticks_by_task
                .retain(|(cooling_task, _)| *cooling_task != task);
        }
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
pub(crate) struct RunningDelayedAnimalTask {
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
pub(crate) fn start_animal_task_cooldowns_after_delayed_tasks_end(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_waits_out_its_delay_each_time_it_becomes_possible() {
        let task = AssetId::from_key("smashbuilding");
        let mut cooldowns = AnimalTaskCooldowns::default();
        cooldowns.arm(task, 100, 30);
        assert!(cooldowns.is_cooling_down(task, 129));
        assert!(!cooldowns.is_cooling_down(task, 130));
        cooldowns.disarm(task);
        assert!(!cooldowns.is_armed(task));
        cooldowns.arm(task, 500, 30);
        assert!(cooldowns.is_cooling_down(task, 510));
    }
}
