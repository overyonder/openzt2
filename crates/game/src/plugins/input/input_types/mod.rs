use std::time::Duration;

use bevy::prelude::*;

/// Device-independent actions understood by the shared application input
/// layer. Domain-specific actions remain typed messages owned by their feature.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GameAction {
    NavigateUp,
    NavigateDown,
    NavigateLeft,
    NavigateRight,
    Confirm,
    Cancel,
    Pause,
    OpenMenu,
    Undo,
    Redo,
    RotateLeft,
    RotateRight,
    ZoomIn,
    ZoomOut,
    PrimaryPointer,
    SecondaryPointer,
    UseObject,
    OverheadView,
    OverviewMap,
    DecreaseBrushSize,
    IncreaseBrushSize,
    RotateObjectLeft,
    RotateObjectRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ActionSource {
    KeyboardMouse,
    Controller(Entity),
    System,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActionRequest {
    pub(crate) action: GameAction,
    pub(crate) source: ActionSource,
}

/// One digital input without a device-specific string representation.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum InputChord {
    Unbound,
    Key(KeyCode),
    KeyPair(KeyCode, KeyCode),
    ModifiedKey {
        key: KeyCode,
        shift: bool,
        control: bool,
        alt: bool,
    },
    Mouse(MouseButton),
    Gamepad(GamepadButton),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GameActionInputBinding {
    pub(crate) action: GameAction,
    pub(crate) primary: InputChord,
    pub(crate) alternate: Option<InputChord>,
}

/// User-configurable action bindings. The boxed array is replaced only when a
/// profile is loaded.
#[derive(Resource, serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct GameActionInputBindings {
    pub(crate) entries: Box<[GameActionInputBinding]>,
}

impl Default for GameActionInputBindings {
    fn default() -> Self {
        use GameAction::*;
        use GamepadButton as Pad;
        use InputChord::*;

        Self {
            entries: Box::new([
                GameActionInputBinding {
                    action: NavigateUp,
                    primary: KeyPair(KeyCode::KeyW, KeyCode::ArrowUp),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: NavigateDown,
                    primary: KeyPair(KeyCode::KeyS, KeyCode::ArrowDown),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: NavigateLeft,
                    primary: KeyPair(KeyCode::KeyA, KeyCode::ArrowLeft),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: NavigateRight,
                    primary: KeyPair(KeyCode::KeyD, KeyCode::ArrowRight),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: Confirm,
                    primary: Key(KeyCode::Enter),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: Cancel,
                    primary: Key(KeyCode::Escape),
                    alternate: Some(Gamepad(Pad::Select)),
                },
                GameActionInputBinding {
                    action: Pause,
                    primary: KeyPair(KeyCode::KeyP, KeyCode::Pause),
                    alternate: Some(Gamepad(Pad::DPadRight)),
                },
                GameActionInputBinding {
                    action: OpenMenu,
                    primary: Key(KeyCode::Escape),
                    alternate: Some(Gamepad(Pad::Select)),
                },
                GameActionInputBinding {
                    action: Undo,
                    primary: ModifiedKey {
                        key: KeyCode::KeyZ,
                        shift: false,
                        control: true,
                        alt: false,
                    },
                    alternate: Some(Gamepad(Pad::DPadLeft)),
                },
                GameActionInputBinding {
                    action: Redo,
                    primary: ModifiedKey {
                        key: KeyCode::KeyY,
                        shift: false,
                        control: true,
                        alt: false,
                    },
                    alternate: None,
                },
                GameActionInputBinding {
                    action: RotateLeft,
                    primary: Key(KeyCode::KeyQ),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: RotateRight,
                    primary: Key(KeyCode::KeyE),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: ZoomIn,
                    primary: KeyPair(KeyCode::Equal, KeyCode::NumpadAdd),
                    alternate: Some(Gamepad(Pad::DPadUp)),
                },
                GameActionInputBinding {
                    action: ZoomOut,
                    primary: KeyPair(KeyCode::Minus, KeyCode::NumpadSubtract),
                    alternate: Some(Gamepad(Pad::DPadDown)),
                },
                GameActionInputBinding {
                    action: PrimaryPointer,
                    primary: Gamepad(Pad::West),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: SecondaryPointer,
                    primary: Gamepad(Pad::East),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: UseObject,
                    primary: Gamepad(Pad::South),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: OverheadView,
                    primary: Gamepad(Pad::North),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: OverviewMap,
                    primary: Gamepad(Pad::Start),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: DecreaseBrushSize,
                    primary: Gamepad(Pad::LeftTrigger2),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: IncreaseBrushSize,
                    primary: Gamepad(Pad::RightTrigger2),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: RotateObjectLeft,
                    primary: Gamepad(Pad::LeftTrigger),
                    alternate: None,
                },
                GameActionInputBinding {
                    action: RotateObjectRight,
                    primary: Gamepad(Pad::RightTrigger),
                    alternate: None,
                },
            ]),
        }
    }
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DeviceControlAxes {
    pub(crate) pan: Vec2,
    /// Forward, backward, right, and left magnitudes retained independently.
    pub(crate) pan_directions: [f32; 4],
    pub(crate) look: Vec2,
    pub(crate) zoom: f32,
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct PrimaryPointerInputState {
    pub(crate) screen: Vec2,
    pub(crate) pressed: bool,
    pub(crate) just_pressed: bool,
    pub(crate) just_released: bool,
    /// Final shared pointer movement in logical window pixels this frame.
    pub(crate) delta: Vec2,
    /// Signed vertical wheel travel reported for the current input frame.
    pub(crate) wheel_y: f32,
    /// True after the windowing backend has supplied an actual cursor
    /// position. A zero-initialized resource is not a pointer at the top-left
    /// edge and must not drive edge scrolling or picking.
    pub(crate) available: bool,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActiveInputDevice {
    pub(crate) source: ActionSource,
    /// Real elapsed time of the last meaningful device input. This is
    /// deliberately independent of simulation pause and speed.
    pub(crate) changed_at: Duration,
}

impl Default for ActiveInputDevice {
    fn default() -> Self {
        Self {
            source: ActionSource::KeyboardMouse,
            changed_at: Duration::ZERO,
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RebindGameActionInput {
    pub(crate) action: GameAction,
    pub(crate) slot: u8,
    pub(crate) chord: InputChord,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GameActionInputRebindingRejection {
    InvalidSlot,
    Duplicate { conflicting_action: GameAction },
}
