use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct FrozenAnimationPoseBlend(pub(crate) Timer);

pub(crate) fn advance_frozen_animation_pose_blends<
    Clock: Component,
    Domain: Default + Send + Sync + 'static,
>(
    mut commands: Commands,
    time: Res<Time<Domain>>,
    mut blends: Query<(Entity, &mut FrozenAnimationPoseBlend), With<Clock>>,
) {
    for (entity, mut blend) in &mut blends {
        if blend.0.tick(time.delta()).is_finished() {
            commands
                .entity(entity)
                .try_remove::<FrozenAnimationPoseBlend>();
        }
    }
}
