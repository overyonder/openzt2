use super::action::UiActionRecord;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiDocumentHotkeyDefinition {
    pub trigger: UiDocumentHotkeyTrigger,
    pub key_code: u32,
    pub character: String,
    pub control_state: UiDocumentHotkeyControlState,
    pub allow_repeat: bool,
    pub action: UiActionRecord,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiDocumentHotkeyTrigger {
    KeyPressed,
    KeyReleased,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum UiDocumentHotkeyControlState {
    #[default]
    ControlReleased,
    ControlPressed,
    AnyControlState,
}
