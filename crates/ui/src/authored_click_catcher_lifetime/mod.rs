use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use std::time::Duration;

use crate::authored_ui_change_activation_dispatch::{
    apply_visibility_transition, set_authored_visibility,
};

#[derive(Component)]
pub(super) struct AuthoredClickCatcherLifetime(pub(super) f32);

#[derive(Component)]
pub(super) struct ActiveClickCatcherTimer(Timer);

pub(super) fn dismiss_pressed_click_catchers(
    mut activations: ParamSet<(
        MessageReader<UiNodeActivated>,
        MessageWriter<UiNodeActivated>,
    )>,
    mut catchers: Query<&mut Visibility, With<AuthoredClickCatcherLifetime>>,
) {
    let hidden: Vec<_> = activations
        .p0()
        .read()
        .filter(|activation| activation.trigger == UiTrigger::Press)
        .filter_map(|activation| {
            let mut visibility = catchers.get_mut(activation.node).ok()?;
            set_authored_visibility(activation.node, &mut visibility, Visibility::Hidden, true)
        })
        .collect();
    let _ = activations.p1().write_batch(hidden);
}

pub(super) fn synchronize_click_catcher_timers(
    mut commands: Commands,
    catchers: Query<
        (
            Entity,
            &AuthoredClickCatcherLifetime,
            &Visibility,
            Has<ActiveClickCatcherTimer>,
        ),
        Or<(Changed<Visibility>, Added<AuthoredClickCatcherLifetime>)>,
    >,
) {
    for (entity, lifetime, visibility, active) in &catchers {
        match (*visibility != Visibility::Hidden, active) {
            (true, false) => {
                let duration =
                    Duration::try_from_secs_f32(lifetime.0.max(0.0)).unwrap_or(Duration::MAX);
                commands
                    .entity(entity)
                    .insert(ActiveClickCatcherTimer(Timer::new(
                        duration,
                        TimerMode::Once,
                    )));
            }
            (false, true) => {
                commands.entity(entity).remove::<ActiveClickCatcherTimer>();
            }
            _ => {}
        }
    }
}

pub(super) fn expire_active_click_catchers(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut catchers: Query<(Entity, &mut ActiveClickCatcherTimer, &mut Visibility)>,
    mut activations: MessageWriter<UiNodeActivated>,
) {
    for (entity, mut timer, mut visibility) in &mut catchers {
        if timer.0.tick(time.delta()).just_finished() {
            apply_visibility_transition(
                entity,
                &mut visibility,
                Visibility::Hidden,
                true,
                &mut activations,
            );
            commands.entity(entity).remove::<ActiveClickCatcherTimer>();
        }
    }
}
