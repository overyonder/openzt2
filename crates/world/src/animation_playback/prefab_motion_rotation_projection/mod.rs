use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::transform_animation_lowering::PrefabMotionRotation;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabAuthoredBillboardOrientationMode;

pub(super) fn project_prefab_motion_rotations(
    mut rotations: Query<
        (
            &PrefabMotionRotation,
            &mut Transform,
            Option<&mut PrefabAuthoredBillboardOrientationMode>,
        ),
        Changed<PrefabMotionRotation>,
    >,
) {
    for (motion, transform, billboard) in &mut rotations {
        let rotation = motion
            .relative_to
            .map_or(motion.sample, |rest| motion.sample * rest);
        if let Some(billboard) = billboard {
            billboard
                .map_unchanged(|billboard| &mut billboard.authored_local_rotation)
                .set_if_neq(rotation);
        } else {
            transform
                .map_unchanged(|transform| &mut transform.rotation)
                .set_if_neq(rotation);
        }
    }
}
