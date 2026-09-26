use crate::plugins::input::input_types::ActionSource;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::node_presentation::UiNodeAnimationInterpolation;

use super::authored_ui_visual_types::UiVisualLayer;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct UiShowHideAnimation {
    pub elapsed_ms: f32,
    pub duration_ms: f32,
    pub exit_rate: f32,
    pub delay_ms: f32,
    pub delay_remaining_ms: f32,
    pub bob_ms: f32,
    pub interpolation: UiNodeAnimationInterpolation,
    pub forward: bool,
    pub running: bool,
    pub base_rect: [f32; 4],
    pub start_rect: [f32; 4],
    pub end_rect: [f32; 4],
    pub animates_color: bool,
    pub affects_text_color: bool,
    pub start_color: [u8; 4],
    pub end_color: [u8; 4],
}

impl UiShowHideAnimation {
    pub(crate) fn start_authored_visibility_transition(&mut self, visible: bool) {
        self.forward = visible;
        self.elapsed_ms = if visible { 0.0 } else { self.duration_ms };
        self.delay_remaining_ms = if visible { self.delay_ms } else { 0.0 };
        self.running = true;
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotifyOnUiAnimationComplete;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAnimationCompleted {
    pub(crate) entity: Entity,
}

/// Runtime origin supplied to an authored show/hide animation whose source
/// rectangle is relative to a moving receiver. Cursor money is inserted under
/// a fixed 1024x768 container, then the placement mode supplies this one
/// primitive pointer coordinate exactly as the original receiver did.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiAnimationOrigin(pub Vec2);

pub(super) fn advance_ui_animations(
    time: Res<Time<Real>>,
    mut nodes: Query<(
        Entity,
        &mut UiShowHideAnimation,
        &mut Node,
        &mut Visibility,
        &Children,
        Option<&mut TextColor>,
        Option<&NotifyOnUiAnimationComplete>,
        Option<&UiAnimationOrigin>,
    )>,
    mut visual_images: Query<&mut ImageNode, With<UiVisualLayer>>,
    mut visual_backgrounds: Query<&mut BackgroundColor, (With<UiVisualLayer>, Without<ImageNode>)>,
    mut completed: MessageWriter<UiAnimationCompleted>,
) {
    let delta_ms = time.delta_secs() * 1000.0;
    for (entity, mut animation, mut node, mut visibility, children, text_color, notify, origin) in
        &mut nodes
    {
        if !animation.running {
            continue;
        }
        if animation.delay_remaining_ms > 0.0 {
            animation.delay_remaining_ms = (animation.delay_remaining_ms - delta_ms).max(0.0);
            continue;
        }
        let step_ms = if animation.forward {
            delta_ms
        } else {
            delta_ms * animation.exit_rate
        };
        animation.elapsed_ms = if animation.forward {
            (animation.elapsed_ms + step_ms).min(animation.duration_ms)
        } else {
            (animation.elapsed_ms - step_ms).max(0.0)
        };
        let linear_amount = if animation.duration_ms == 0.0 {
            if animation.forward {
                1.0
            } else {
                0.0
            }
        } else {
            animation.elapsed_ms / animation.duration_ms
        };
        let amount = match animation.interpolation {
            UiNodeAnimationInterpolation::Linear => linear_amount,
            UiNodeAnimationInterpolation::Sinusoidal => {
                (linear_amount * std::f32::consts::FRAC_PI_2).sin()
            }
        };
        apply_rect(&mut node, &animation, amount, origin.map(|origin| origin.0));
        if animation.animates_color {
            let color = lerp_color(animation.start_color, animation.end_color, amount);
            if animation.affects_text_color {
                if let Some(mut text_color) = text_color {
                    text_color.0 = color;
                }
            } else {
                for child in children.iter() {
                    if let Ok(mut image) = visual_images.get_mut(child) {
                        image.color = color;
                    } else if let Ok(mut background) = visual_backgrounds.get_mut(child) {
                        background.0 = color;
                    }
                }
            }
        }
        if animation.elapsed_ms == 0.0 || animation.elapsed_ms == animation.duration_ms {
            if animation.forward && animation.bob_ms > 0.0 {
                animation.forward = false;
                animation.delay_remaining_ms = animation.bob_ms;
                continue;
            }
            animation.running = false;
            if !animation.forward {
                *visibility = Visibility::Hidden;
            }
            if notify.is_some() {
                completed.write(UiAnimationCompleted { entity });
            }
        }
    }
}

pub(super) fn dispatch_ui_animation_commands(
    mut completed: MessageReader<UiAnimationCompleted>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for event in completed.read() {
        activated.write(UiNodeActivated {
            source: ActionSource::System,
            node: event.entity,
            trigger: UiTrigger::AnimationCompleted,
        });
    }
}

fn apply_rect(node: &mut Node, animation: &UiShowHideAnimation, amount: f32, origin: Option<Vec2>) {
    // `-1` preserves one authored base component. Other negative values are
    // real coordinates: the in-game controls panel slides from x=-311 while
    // retaining its width and height through `w=-1 h=-1`.
    let value = |index: usize| {
        let start = animation.start_rect[index];
        let end = animation.end_rect[index];
        if start == -1.0 || end == -1.0 {
            animation.base_rect[index]
        } else {
            start + (end - start) * amount
        }
    };
    let relative = |index: usize| {
        origin.map_or(value(index), |origin| {
            origin[index] + animation.base_rect[index] + value(index)
        })
    };
    node.left = px(relative(0));
    node.top = px(relative(1));
    node.width = px(value(2));
    node.height = px(value(3));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_hide_rect_animates_negative_position_and_preserves_sentinel_extent() {
        let animation = UiShowHideAnimation {
            elapsed_ms: 0.0,
            duration_ms: 300.0,
            exit_rate: 1.0,
            delay_ms: 0.0,
            delay_remaining_ms: 0.0,
            bob_ms: 0.0,
            interpolation: UiNodeAnimationInterpolation::Linear,
            forward: true,
            running: true,
            base_rect: [0.0, 540.0, 327.0, 228.0],
            start_rect: [-311.0, 540.0, -1.0, -1.0],
            end_rect: [0.0, 540.0, -1.0, -1.0],
            animates_color: false,
            affects_text_color: false,
            start_color: [255; 4],
            end_color: [255; 4],
        };
        let mut node = Node::default();

        apply_rect(&mut node, &animation, 0.5, None);

        assert_eq!(node.left, px(-155.5));
        assert_eq!(node.top, px(540.0));
        assert_eq!(node.width, px(327.0));
        assert_eq!(node.height, px(228.0));
    }

    #[test]
    fn moving_receiver_origin_keeps_authored_region_offset_during_rise() {
        let animation = UiShowHideAnimation {
            elapsed_ms: 400.0,
            duration_ms: 800.0,
            exit_rate: 1.0,
            delay_ms: 0.0,
            delay_remaining_ms: 0.0,
            bob_ms: 0.0,
            interpolation: UiNodeAnimationInterpolation::Linear,
            forward: false,
            running: true,
            base_rect: [-80.0, -30.0, 100.0, 30.0],
            start_rect: [0.0, -100.0, -1.0, -1.0],
            end_rect: [0.0, 0.0, -1.0, -1.0],
            animates_color: true,
            affects_text_color: true,
            start_color: [255, 255, 64, 0],
            end_color: [255, 255, 64, 192],
        };
        let mut node = Node::default();

        apply_rect(&mut node, &animation, 0.5, Some(Vec2::new(600.0, 400.0)));

        assert_eq!(node.left, px(520.0));
        assert_eq!(node.top, px(320.0));
        assert_eq!(node.width, px(100.0));
        assert_eq!(node.height, px(30.0));
    }
}

fn lerp_color(start: [u8; 4], end: [u8; 4], amount: f32) -> Color {
    let channel = |index: usize| {
        (f32::from(start[index]) + (f32::from(end[index]) - f32::from(start[index])) * amount)
            / 255.0
    };
    Color::srgba(channel(0), channel(1), channel(2), channel(3))
}
