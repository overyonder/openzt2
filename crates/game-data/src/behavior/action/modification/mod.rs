use serde::{Deserialize, Serialize};

use super::entity_role::BehaviorEntityRole;
use crate::behavior::scalar::BehaviorScalarQ16;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorFactModification {
    pub affected_entity_role: BehaviorEntityRole,
    pub modified_fact: BehaviorFact,
    pub modification_operation: BehaviorFactModificationOperation,
    pub modification_value: BehaviorScalarQ16,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorFactModificationOperation {
    Add,
    Clear,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorFact {
    Amusement,
    Bathroom,
    Breath,
    Exercise,
    Happiness,
    Health,
    Hunger,
    Hygiene,
    Privacy,
    Reproduction,
    Rest,
    Social,
    Space,
    Stimulation,
    Thirst,
    WaterDirtiness,
    FoodLevel,
    BoneLevel,
    TrashLevel,
    FilterDirtiness,
    Construction,
    Damage,
    Opened,
    Moving,
    Disease,
    Pregnancy,
    Rampage,
    Escaped,
    Lifespan,
    Swimming,
    InWater,
    OnLand,
    PaintLevel,
    PaintLevel1,
    PaintLevel2,
    PaintLevel3,
    PaintLevel4,
    IceLevel,
    DeparturePoints,
    Talking,
    Dead,
    Frozen,
    InGlacier,
    NursingYoung,
    Dessert,
    Gift,
    ViewAnimals,
    AteFavoriteFood,
    AteNonFavoriteFood,
}
