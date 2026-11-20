//! An animal that jumps, climbs, flies over or tunnels under a fence ends on
//! the far side of it once its task completes.

use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_zoo_state::{
    behavior_task_execution_types::{BehaviorTaskFailed, BehaviorTaskFinished},
    topology::topology_graph_types::{FenceSegment, TopologyGrid, TopologyNode},
};

/// The fence the animal's current task crosses.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct CrossingFence {
    pub(crate) execution_id: u64,
    pub(crate) fence: Entity,
}

/// Whether a fence task, by its name, takes the animal across the fence:
/// `JumpLowFence`, `ClimbFence_*`, `ClimbUnderFence`, `FlyLowFence` and
/// `RunThroughRailFence`, rather than bashing or examining it.
// Told apart by name, as the bash tasks are; the clips that carry
// an animal over aren't root motion here.
pub(crate) fn fence_task_crosses(task_name: &str) -> bool {
    ["Jump", "Climb", "Fly", "RunThrough"]
        .iter()
        .any(|prefix| task_name.starts_with(prefix))
}

pub(super) fn register_fence_crossing_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        carry_animals_across_their_crossed_fences
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

/// The point mirrored across the fence line through `first` and `second`.
pub(crate) fn mirror_across_fence(point: Vec3, first: Vec3, second: Vec3) -> Vec3 {
    let along = (second - first).xz().normalize_or_zero();
    let normal = Vec2::new(-along.y, along.x);
    let offset = (point.xz() - first.xz()).dot(normal);
    let mirrored = point.xz() - normal * (2.0 * offset);
    Vec3::new(mirrored.x, point.y, mirrored.y)
}

fn carry_animals_across_their_crossed_fences(
    mut commands: Commands,
    mut finished: MessageReader<BehaviorTaskFinished>,
    mut failed: MessageReader<BehaviorTaskFailed>,
    mut animals: Query<(&CrossingFence, &mut Transform)>,
    fences: Query<FenceSegment>,
    nodes: Query<&TopologyNode>,
    grid: Res<TopologyGrid>,
) {
    for outcome in failed.read() {
        if animals
            .get(outcome.actor)
            .is_ok_and(|(crossing, _)| crossing.execution_id == outcome.execution_id)
        {
            commands.entity(outcome.actor).remove::<CrossingFence>();
        }
    }
    for outcome in finished.read() {
        let Ok((crossing, mut transform)) = animals.get_mut(outcome.actor) else {
            continue;
        };
        if crossing.execution_id != outcome.execution_id {
            continue;
        }
        let ends = fences.get(crossing.fence).ok().and_then(|edge| {
            Some((
                grid.cell_translation(nodes.get(edge.a()).ok()?.cell),
                grid.cell_translation(nodes.get(edge.b()).ok()?.cell),
            ))
        });
        if let Some((first, second)) = ends {
            // The animal docked short of the fence on its side; it lands as
            // far beyond it.
            transform.translation = mirror_across_fence(transform.translation, first, second);
        }
        commands.entity(outcome.actor).remove::<CrossingFence>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crossing_animal_lands_as_far_beyond_the_fence_as_it_stood_before_it() {
        // A fence along X; the animal stands 2 m to its north (-Z).
        let landed = mirror_across_fence(
            Vec3::new(1.5, 3.0, -2.0),
            Vec3::ZERO,
            Vec3::new(3.0, 0.0, 0.0),
        );
        assert!(landed.abs_diff_eq(Vec3::new(1.5, 3.0, 2.0), 1.0e-5));
        assert!(fence_task_crosses("JumpLowFence"));
        assert!(fence_task_crosses("ClimbUnderFence"));
        assert!(!fence_task_crosses("BashFence"));
        assert!(!fence_task_crosses("SniffElectricFence"));
    }
}
