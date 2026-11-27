use bevy::{
    input::{mouse::MouseButtonInput, ButtonState, InputPlugin, InputSystems},
    picking::{
        backend::{HitData, PointerHits},
        events::{PointerClick, PointerPress},
        pointer::{PointerButton, PointerId, PointerPressState},
        InteractionPlugin, PickingPlugin, PickingSystems,
    },
    prelude::*,
    time::TimePlugin,
    window::PrimaryWindow,
};

use openzt2_interface_state::input::input_types::GameAction;

use super::gamepad_input_translation::radial_dead_zone;
use super::input_types::GameActionInputBindings;
use super::input_types::InputChord;
use super::shared_device_input::publish_active_device_axes_and_pointer;
use super::{input_execution_types::DeviceControlAxes, input_types::ActiveInputDevice};

#[test]
fn original_wasd_and_arrow_pan_aliases_are_both_bound() {
    let bindings = GameActionInputBindings::default();
    let up = bindings
        .entries
        .iter()
        .find(|binding| binding.action == GameAction::NavigateUp)
        .expect("up binding");
    assert_eq!(
        up.primary,
        InputChord::KeyPair(KeyCode::KeyW, KeyCode::ArrowUp)
    );
}

#[test]
fn dead_zone_rejects_noise_and_preserves_analog_magnitude() {
    assert_eq!(radial_dead_zone(Vec2::new(0.05, -0.05)), Vec2::ZERO);
    let output = radial_dead_zone(Vec2::new(0.6, 0.0));
    assert!(output.x > 0.0 && output.x < 1.0);
    assert_eq!(output.y, 0.0);
}

#[test]
fn mouse_press_and_release_in_one_update_pick_the_hovered_control() {
    let mut app = App::new();
    app.add_plugins((TimePlugin, InputPlugin, PickingPlugin, InteractionPlugin))
        .init_resource::<ActiveInputDevice>()
        .init_resource::<DeviceControlAxes>()
        .add_systems(
            PreUpdate,
            publish_active_device_axes_and_pointer
                .after(InputSystems)
                .before(PickingSystems::ProcessInput),
        );

    let mut window = Window::default();
    window.set_cursor_position(Some(Vec2::splat(16.0)));
    let window = app.world_mut().spawn((PrimaryWindow, window)).id();
    let pointer = app.world_mut().spawn(PointerId::Mouse).id();
    let camera = app.world_mut().spawn_empty().id();
    let control = app.world_mut().spawn(Pickable::default()).id();
    app.add_systems(
        PreUpdate,
        (move |mut hits: MessageWriter<PointerHits>| {
            hits.write(PointerHits::new(
                PointerId::Mouse,
                vec![(control, HitData::new(camera, 0.0, None, None))],
                0.0,
            ));
        })
        .in_set(PickingSystems::Backend),
    );
    app.update();

    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window,
        });
    }
    app.update();

    let world = app.world_mut();
    let presses = world
        .resource_mut::<Messages<PointerPress>>()
        .drain()
        .map(|event| (event.entity, event.button))
        .collect::<Vec<_>>();
    let clicks = world
        .resource_mut::<Messages<PointerClick>>()
        .drain()
        .map(|event| (event.entity, event.button))
        .collect::<Vec<_>>();
    assert_eq!(presses, [(control, PointerButton::Primary)]);
    assert_eq!(clicks, [(control, PointerButton::Primary)]);
    assert!(!world
        .get::<PointerPressState>(pointer)
        .expect("mouse pointer button state")
        .is_primary_pressed());
}
