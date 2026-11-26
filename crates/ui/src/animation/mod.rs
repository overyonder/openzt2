use bevy::prelude::*;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::node_presentation::UiNodeAnimationInterpolation;
use openzt2_interface_state::input::input_types::ActionSource;
use std::time::Duration;

use super::authored_ui_visual_types::UiVisualLayer;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component, Debug, Clone, Copy)]
pub struct UiShowHideAnimation {
    pub duration_ms: f32,
    pub exit_rate: f32,
    pub delay_ms: f32,
    pub bob_factor: f32,
    pub interpolation: UiNodeAnimationInterpolation,
    pub base_rect: [f32; 4],
    pub rect_intervals: [Option<[f32; 2]>; 4],
    pub animates_color: bool,
    pub affects_text_color: bool,
    pub start_color: [u8; 4],
    pub end_color: [u8; 4],
}

#[derive(Component, Debug, Default, PartialEq, Eq)]
pub struct UiAnimationDirection(pub bool);

#[derive(Component, Clone)]
pub struct UiShowHidePlayback {
    delay: Option<Timer>,
    progress: Timer,
}

impl UiShowHidePlayback {
    pub(super) fn at_elapsed(
        policy: &UiShowHideAnimation,
        forward: bool,
        elapsed_ms: f32,
        delay_ms: f32,
    ) -> Self {
        let duration = duration_from_ms(policy.duration_ms);
        let elapsed = duration_from_ms(elapsed_ms).min(duration);
        let mut progress = Timer::new(duration, TimerMode::Once);
        progress.set_elapsed(if forward {
            elapsed
        } else {
            duration.saturating_sub(elapsed)
        });
        Self {
            delay: (delay_ms > 0.0)
                .then(|| Timer::new(duration_from_ms(delay_ms), TimerMode::Once)),
            progress,
        }
    }

    fn fraction(&self, forward: bool) -> f32 {
        if forward {
            self.progress.fraction()
        } else if self.progress.duration().is_zero() {
            0.0
        } else {
            self.progress.remaining_secs() / self.progress.duration().as_secs_f32()
        }
    }
}

fn duration_from_ms(milliseconds: f32) -> Duration {
    Duration::try_from_secs_f64(f64::from(milliseconds.max(0.0)) * 0.001).unwrap_or(Duration::MAX)
}

#[derive(Message, Clone, Copy)]
pub struct UiAnimationPlaybackRequest {
    pub entity: Entity,
    pub operation: UiAnimationPlaybackOperation,
}

#[derive(Clone, Copy)]
pub enum UiAnimationPlaybackOperation {
    Transition(bool),
    Finish,
    Reset(bool),
}

pub(super) fn apply_authored_animation_playback_requests(
    mut commands: Commands,
    mut requests: MessageReader<UiAnimationPlaybackRequest>,
    mut nodes: Query<(
        &UiShowHideAnimation,
        &mut UiAnimationDirection,
        Option<&mut UiShowHidePlayback>,
    )>,
) {
    use bevy::ecs::entity::EntityHashMap;
    let mut by_node = EntityHashMap::<Vec<UiAnimationPlaybackOperation>>::default();
    for request in requests.read() {
        by_node
            .entry(request.entity)
            .or_default()
            .push(request.operation);
    }
    for (entity, operations) in by_node {
        let Ok((policy, mut direction, playback)) = nodes.get_mut(entity) else {
            continue;
        };
        let mut next = playback.as_deref().cloned();
        let mut forward = direction.0;
        for operation in operations {
            match operation {
                UiAnimationPlaybackOperation::Transition(show) => {
                    if let Some(current) = next.as_mut().filter(|_| forward != show) {
                        let reversed = current
                            .progress
                            .duration()
                            .saturating_sub(current.progress.elapsed());
                        current.progress.reset();
                        current.progress.set_elapsed(reversed);
                        current.delay = None;
                    } else {
                        next = Some(UiShowHidePlayback::at_elapsed(
                            policy,
                            show,
                            if show { 0.0 } else { policy.duration_ms },
                            if show { policy.delay_ms } else { 0.0 },
                        ));
                    }
                    forward = show;
                }
                UiAnimationPlaybackOperation::Finish => {
                    let active = next.get_or_insert_with(|| {
                        UiShowHidePlayback::at_elapsed(policy, forward, 0.0, 0.0)
                    });
                    active.progress.finish();
                }
                UiAnimationPlaybackOperation::Reset(next_direction) => {
                    forward = next_direction;
                    next = None;
                }
            }
        }
        direction.set_if_neq(UiAnimationDirection(forward));
        match (playback, next) {
            (Some(mut playback), Some(next)) => *playback = next,
            (None, Some(next)) => {
                commands.entity(entity).insert(next);
            }
            (Some(_), None) => {
                commands.entity(entity).remove::<UiShowHidePlayback>();
            }
            (None, None) => {}
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotifyOnUiAnimationComplete;

/// Runtime origin supplied to an authored show/hide animation whose source
/// rectangle is relative to a moving receiver. Cursor money is inserted under
/// a fixed 1024x768 container, then the placement mode supplies this one
/// primitive pointer coordinate exactly as the original receiver did.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiAnimationOrigin(pub Vec2);

pub(super) fn tick_authored_ui_animations(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut nodes: Query<(
        Entity,
        &UiShowHideAnimation,
        &UiAnimationDirection,
        &mut UiShowHidePlayback,
        &mut Node,
        &mut Visibility,
        Option<&Children>,
        Option<&mut TextColor>,
        Has<NotifyOnUiAnimationComplete>,
        Option<&UiAnimationOrigin>,
        Has<super::authored_ui_node_projection_components::UiNodeId>,
    )>,
    mut images: Query<&mut ImageNode, With<UiVisualLayer>>,
    mut backgrounds: Query<&mut BackgroundColor, (With<UiVisualLayer>, Without<ImageNode>)>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (
        entity,
        animation,
        forward,
        mut playback,
        mut node,
        mut visibility,
        children,
        text,
        notify,
        origin,
        authored,
    ) in &mut nodes
    {
        if let Some(delay) = playback.delay.as_mut() {
            if delay.tick(time.delta()).is_finished() {
                playback.delay = None;
            }
            continue;
        }
        let forward = forward.0;
        let delta = if forward {
            time.delta()
        } else {
            Duration::try_from_secs_f64(
                time.delta_secs_f64() * f64::from(animation.exit_rate.max(0.0)),
            )
            .unwrap_or(Duration::MAX)
        };
        playback.progress.tick(delta);
        let linear = playback.fraction(forward);
        let amount = match animation.interpolation {
            UiNodeAnimationInterpolation::Linear => linear,
            UiNodeAnimationInterpolation::Sinusoidal if animation.bob_factor == 0.0 => {
                (1.0 - (linear * std::f32::consts::PI).cos()) * 0.5
            }
            UiNodeAnimationInterpolation::Sinusoidal => {
                let extent = animation.bob_factor * std::f32::consts::FRAC_PI_2;
                (extent * linear).sin() / extent.sin()
            }
        };
        apply_rect(&mut node, animation, amount, origin.map(|origin| origin.0));
        if animation.animates_color {
            let color = lerp_color(animation.start_color, animation.end_color, amount);
            if animation.affects_text_color {
                if let Some(mut text) = text {
                    text.set_if_neq(TextColor(color));
                }
            } else {
                for child in children.into_iter().flatten().copied() {
                    if let Ok(image) = images.get_mut(child) {
                        image
                            .map_unchanged(|image| &mut image.color)
                            .set_if_neq(color);
                    } else if let Ok(mut background) = backgrounds.get_mut(child) {
                        background.set_if_neq(BackgroundColor(color));
                    }
                }
            }
        }
        if playback.progress.is_finished() || playback.progress.elapsed().is_zero() {
            commands.entity(entity).remove::<UiShowHidePlayback>();
            if !forward {
                super::authored_ui_change_activation_dispatch::apply_visibility_transition(
                    entity,
                    &mut visibility,
                    Visibility::Hidden,
                    authored,
                    &mut activated,
                );
            }
            if notify {
                activated.write(UiNodeActivated {
                    source: ActionSource::System,
                    node: entity,
                    trigger: UiTrigger::AnimationCompleted,
                });
            }
        }
    }
}

fn apply_rect(node: &mut Node, animation: &UiShowHideAnimation, amount: f32, origin: Option<Vec2>) {
    let animated = |index: usize| {
        animation.rect_intervals[index].map(|[start, end]| start + (end - start) * amount)
    };
    let components = [
        &mut node.left,
        &mut node.top,
        &mut node.width,
        &mut node.height,
    ];
    for (index, component) in components.into_iter().enumerate() {
        let value = match origin {
            // A moving receiver positions the authored region itself. An
            // animated coordinate replaces the region's own offset from the
            // receiver: the cursor price, authored at x=-80 y=-30 with an
            // animation ending at 0,0, sits at the pointer in the original.
            Some(origin) => {
                let value = animated(index).unwrap_or(animation.base_rect[index]);
                if index < 2 {
                    origin[index] + value
                } else {
                    value
                }
            }
            // Preserved components keep the projected layout, which may be
            // aligned or a percentage of the parent, such as a hover-revealed
            // text button that only animates its colour.
            None => match animated(index) {
                Some(value) => value,
                None => continue,
            },
        };
        *component = px(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_bob_overshoots_then_keeps_the_shown_endpoint_until_hidden() {
        let mut application = App::new();
        application
            .init_resource::<Time<Real>>()
            .add_message::<UiNodeActivated>()
            .add_message::<UiAnimationPlaybackRequest>()
            .add_systems(
                Update,
                (
                    apply_authored_animation_playback_requests,
                    tick_authored_ui_animations,
                )
                    .chain(),
            );
        let child = application.world_mut().spawn_empty().id();
        let entity = application
            .world_mut()
            .spawn((
                UiAnimationDirection(true),
                Node::default(),
                Visibility::Inherited,
                UiShowHideAnimation {
                    duration_ms: 500.0,
                    exit_rate: 1.0,
                    delay_ms: 0.0,
                    bob_factor: 1.3,
                    interpolation: UiNodeAnimationInterpolation::Sinusoidal,
                    base_rect: [0.0; 4],
                    rect_intervals: [Some([0.0, 100.0]), None, None, None],
                    animates_color: false,
                    affects_text_color: false,
                    start_color: [255; 4],
                    end_color: [255; 4],
                },
            ))
            .add_child(child)
            .id();
        application
            .world_mut()
            .write_message(UiAnimationPlaybackRequest {
                entity,
                operation: UiAnimationPlaybackOperation::Transition(true),
            });
        application
            .world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_millis(400));
        application.update();
        let Val::Px(position) = application.world().get::<Node>(entity).unwrap().left else {
            panic!("animation did not position its node");
        };
        assert!(position > 100.0);
        application
            .world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_millis(100));
        application.update();
        assert_eq!(
            application.world().get::<Node>(entity).unwrap().left,
            px(100.0)
        );
        application
            .world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_secs(2));
        application.update();
        assert_eq!(
            application.world().get::<Visibility>(entity),
            Some(&Visibility::Inherited)
        );
        assert!(application
            .world()
            .get::<UiShowHidePlayback>(entity)
            .is_none());
        application
            .world_mut()
            .write_message(UiAnimationPlaybackRequest {
                entity,
                operation: UiAnimationPlaybackOperation::Transition(false),
            });
        application.update();
        assert_eq!(
            application.world().get::<Visibility>(entity),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn show_hide_rect_animates_negative_position_and_keeps_unanimated_extent() {
        let animation = UiShowHideAnimation {
            duration_ms: 300.0,
            exit_rate: 1.0,
            delay_ms: 0.0,
            bob_factor: 0.0,
            interpolation: UiNodeAnimationInterpolation::Linear,
            base_rect: [0.0, 540.0, 327.0, 228.0],
            rect_intervals: [Some([-311.0, 0.0]), Some([540.0, 540.0]), None, None],
            animates_color: false,
            affects_text_color: false,
            start_color: [255; 4],
            end_color: [255; 4],
        };
        let mut node = Node {
            width: px(327.0),
            height: px(228.0),
            ..default()
        };

        apply_rect(&mut node, &animation, 0.5, None);

        assert_eq!(node.left, px(-155.5));
        assert_eq!(node.top, px(540.0));
        assert_eq!(node.width, px(327.0));
        assert_eq!(node.height, px(228.0));
    }

    #[test]
    fn moving_receiver_animation_replaces_the_authored_region_offset() {
        let animation = UiShowHideAnimation {
            duration_ms: 800.0,
            exit_rate: 1.0,
            delay_ms: 0.0,
            bob_factor: 0.0,
            interpolation: UiNodeAnimationInterpolation::Linear,
            base_rect: [-80.0, -30.0, 100.0, 30.0],
            rect_intervals: [Some([0.0, 0.0]), Some([-100.0, 0.0]), None, None],
            animates_color: true,
            affects_text_color: true,
            start_color: [255, 255, 64, 0],
            end_color: [255, 255, 64, 192],
        };
        let mut node = Node::default();

        apply_rect(&mut node, &animation, 0.5, Some(Vec2::new(600.0, 400.0)));

        assert_eq!(node.left, px(600.0));
        assert_eq!(node.top, px(350.0));
        assert_eq!(node.width, px(100.0));
        assert_eq!(node.height, px(30.0));
    }

    #[test]
    fn reversing_a_fade_preserves_its_current_color_and_skips_the_show_delay() {
        let animation = UiShowHideAnimation {
            duration_ms: 10.0,
            exit_rate: 0.02,
            delay_ms: 50.0,
            bob_factor: 0.0,
            interpolation: UiNodeAnimationInterpolation::Linear,
            base_rect: [0.0; 4],
            rect_intervals: [None; 4],
            animates_color: true,
            affects_text_color: true,
            start_color: [255, 197, 78, 255],
            end_color: [255, 255, 201, 255],
        };
        let mut application = App::new();
        application
            .add_message::<UiAnimationPlaybackRequest>()
            .add_systems(Update, apply_authored_animation_playback_requests);
        let playback = UiShowHidePlayback::at_elapsed(&animation, false, 4.0, 0.0);
        let before = lerp_color(
            animation.start_color,
            animation.end_color,
            playback.fraction(false),
        );
        let entity = application
            .world_mut()
            .spawn((animation, UiAnimationDirection(false), playback))
            .id();
        application
            .world_mut()
            .write_message(UiAnimationPlaybackRequest {
                entity,
                operation: UiAnimationPlaybackOperation::Transition(true),
            });
        application.update();
        let playback = application
            .world()
            .get::<UiShowHidePlayback>(entity)
            .unwrap();
        assert!(
            application
                .world()
                .get::<UiAnimationDirection>(entity)
                .unwrap()
                .0
        );
        assert!(playback.delay.is_none());
        assert_eq!(
            lerp_color(
                animation.start_color,
                animation.end_color,
                playback.fraction(true)
            ),
            before
        );
        application
            .world_mut()
            .write_message(UiAnimationPlaybackRequest {
                entity,
                operation: UiAnimationPlaybackOperation::Transition(false),
            });
        application.update();
        let playback = application
            .world()
            .get::<UiShowHidePlayback>(entity)
            .unwrap();
        assert!(
            !application
                .world()
                .get::<UiAnimationDirection>(entity)
                .unwrap()
                .0
        );
        assert_eq!(playback.progress.elapsed(), Duration::from_millis(6));
    }
}

fn lerp_color(start: [u8; 4], end: [u8; 4], amount: f32) -> Color {
    let channel = |index: usize| {
        (f32::from(start[index]) + (f32::from(end[index]) - f32::from(start[index])) * amount)
            / 255.0
    };
    Color::srgba(channel(0), channel(1), channel(2), channel(3))
}
