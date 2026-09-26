use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorEntityRole {
    Subject,
    Target,
    Object,
    SelfEntity,
}
