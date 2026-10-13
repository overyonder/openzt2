use bevy::prelude::*;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;
use openzt2_world_state::world_spawn::prefab_authored_transform_animation_advancement::PrefabAuthoredTransformAnimation;

pub(super) fn advance_authored_prefab_transform_animations_from_elapsed_time(
    time: Res<Time>,
    mut animations: Query<(
        &mut PrefabAuthoredTransformAnimation,
        &mut Transform,
        Option<&mut PrefabAuthoredBillboardOrientationMode>,
    )>,
) {
    let elapsed_s = time.elapsed_secs();
    for (animated, transform, billboard) in &mut animations {
        PrefabAuthoredTransformAnimation::apply_elapsed_transform(
            animated, elapsed_s, transform, billboard,
        );
    }
}
