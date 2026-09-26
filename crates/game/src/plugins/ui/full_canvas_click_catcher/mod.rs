use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;

use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Entity-local lifecycle for an authored `ZTUIFullscreenButton`.
///
/// Despite its source name, this is the full-canvas click catcher behind a
/// transient control such as the show-mixer droplist. It does not own or
/// mirror the application's window mode.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct UiFullCanvasClickCatcher {
    delay_seconds: f32,
    elapsed_seconds: f32,
}

impl UiFullCanvasClickCatcher {
    pub(super) const fn new(delay_seconds: f32) -> Self {
        Self {
            delay_seconds,
            elapsed_seconds: 0.0,
        }
    }
}

/// Hides an authored full-canvas click catcher when it is pressed or when its
/// authored visible lifetime elapses. The same activation remains available
/// to the catcher's typed action consumers.
pub(super) fn hide_activated_or_elapsed_full_canvas_click_catchers(
    time: Res<Time<Real>>,
    mut activations: MessageReader<UiNodeActivated>,
    mut click_catchers: Query<(&mut UiFullCanvasClickCatcher, &mut Visibility)>,
) {
    for activation in activations.read() {
        if activation.trigger != UiTrigger::Press {
            continue;
        }
        let Ok((mut click_catcher, mut visibility)) = click_catchers.get_mut(activation.node)
        else {
            continue;
        };
        *visibility = Visibility::Hidden;
        click_catcher.elapsed_seconds = 0.0;
    }

    let delta_seconds = time.delta_secs();
    for (mut click_catcher, mut visibility) in &mut click_catchers {
        if *visibility == Visibility::Hidden {
            click_catcher.elapsed_seconds = 0.0;
            continue;
        }

        click_catcher.elapsed_seconds += delta_seconds;
        if click_catcher.elapsed_seconds < click_catcher.delay_seconds.max(0.0) {
            continue;
        }
        click_catcher.elapsed_seconds = 0.0;
        *visibility = Visibility::Hidden;
    }
}
