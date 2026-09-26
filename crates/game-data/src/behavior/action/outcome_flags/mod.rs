use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct BehaviorOutcomeFlags(pub u32);

impl BehaviorOutcomeFlags {
    pub const LOOP: u32 = 1 << 0;
    pub const INTERRUPT: u32 = 1 << 1;
    pub const GROUND_FIT: u32 = 1 << 2;
    pub const AVOID_WATER: u32 = 1 << 3;
    pub const AVOID_LAND: u32 = 1 << 4;
    pub const REDOCK: u32 = 1 << 5;
    pub const PARENT_TO_TARGET: u32 = 1 << 6;
    pub const UNPARENT: u32 = 1 << 7;
    pub const USE_TARGET_NAME: u32 = 1 << 8;
    pub const RECONSIDER: u32 = 1 << 9;
    pub const IMMEDIATE: u32 = 1 << 10;
    pub const CLOSEST_APPROACH: u32 = 1 << 11;
    pub const NAME_RESULT: u32 = 1 << 12;
    pub const SPAWN_IN_TARGET_AREA: u32 = 1 << 13;
    pub const MORPH_FADE: u32 = 1 << 14;
    pub const ALL: u32 = (1 << 15) - 1;
}
