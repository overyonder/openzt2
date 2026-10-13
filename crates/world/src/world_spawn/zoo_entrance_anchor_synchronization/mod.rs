use bevy::prelude::*;
use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::zoo_entrance_from_authored_gate_anchor;
use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance;

pub(super) fn synchronize_zoo_entrance_anchor_after_transform_changes(
    mut entrances: Query<(&Transform, &mut ZooEntrance), Changed<Transform>>,
) {
    for (transform, mut entrance) in &mut entrances {
        let next = zoo_entrance_from_authored_gate_anchor(transform.translation);
        if *entrance != next {
            *entrance = next;
        }
    }
}
