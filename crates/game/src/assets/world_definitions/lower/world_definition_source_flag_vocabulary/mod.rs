use super::object_facility_staff_and_guest_source_vocabulary::{staff_job_bit, staff_job_kind};
use openzt2_game_data::world_definitions::animal_health::TranquilizerEligibility;
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueFilterFlags;
use openzt2_game_data::world_definitions::fences_and_gates::FenceTraversalBlockingFlags;
use openzt2_game_data::world_definitions::object_placement::{
    FootprintCellFlags, PlacementConstraints,
};
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectAffordanceFlags, WorldObjectPropertyFlags,
};

pub(super) fn object_tag(v: &str) -> Option<u64> {
    Some(match v {
        "animalusable" => WorldObjectPropertyFlags::ANIMAL_USABLE.raw_flag_bits(),
        "guestusable" => WorldObjectPropertyFlags::GUEST_USABLE.raw_flag_bits(),
        "staffonly" => WorldObjectPropertyFlags::STAFF_ONLY.raw_flag_bits(),
        "indoor" => WorldObjectPropertyFlags::INDOOR.raw_flag_bits(),
        "outdoor" => WorldObjectPropertyFlags::OUTDOOR.raw_flag_bits(),
        "waterplaceable" => WorldObjectPropertyFlags::WATER_PLACEABLE.raw_flag_bits(),
        "wallplaceable" => WorldObjectPropertyFlags::WALL_PLACEABLE.raw_flag_bits(),
        "pathrequired" => WorldObjectPropertyFlags::PATH_REQUIRED.raw_flag_bits(),
        "donationacceptor" => WorldObjectPropertyFlags::DONATION_ACCEPTOR.raw_flag_bits(),
        "viewable" => WorldObjectPropertyFlags::VIEWABLE.raw_flag_bits(),
        "deletable" => WorldObjectPropertyFlags::DELETABLE.raw_flag_bits(),
        "saverelevant" => WorldObjectPropertyFlags::SAVE_RELEVANT.raw_flag_bits(),
        "waterboundary" => WorldObjectPropertyFlags::WATER_BOUNDARY.raw_flag_bits(),
        _ => return None,
    })
}
pub(super) fn affordance(v: &str) -> Option<u64> {
    Some(match v {
        "eat" => WorldObjectAffordanceFlags::EAT.raw_flag_bits(),
        "drink" => WorldObjectAffordanceFlags::DRINK.raw_flag_bits(),
        "rest" => WorldObjectAffordanceFlags::REST.raw_flag_bits(),
        "play" => WorldObjectAffordanceFlags::PLAY.raw_flag_bits(),
        "shelter" => WorldObjectAffordanceFlags::SHELTER.raw_flag_bits(),
        "view" => WorldObjectAffordanceFlags::VIEW.raw_flag_bits(),
        "buy" => WorldObjectAffordanceFlags::BUY.raw_flag_bits(),
        "donate" => WorldObjectAffordanceFlags::DONATE.raw_flag_bits(),
        "learn" => WorldObjectAffordanceFlags::LEARN.raw_flag_bits(),
        "clean" => WorldObjectAffordanceFlags::CLEAN.raw_flag_bits(),
        "repair" => WorldObjectAffordanceFlags::REPAIR.raw_flag_bits(),
        "treat" => WorldObjectAffordanceFlags::TREAT.raw_flag_bits(),
        "train" => WorldObjectAffordanceFlags::TRAIN.raw_flag_bits(),
        "board" => WorldObjectAffordanceFlags::BOARD.raw_flag_bits(),
        "disembark" => WorldObjectAffordanceFlags::DISEMBARK.raw_flag_bits(),
        "operate" => WorldObjectAffordanceFlags::OPERATE.raw_flag_bits(),
        _ => return None,
    })
}
pub(super) fn placement_flag(v: &str) -> Option<u64> {
    Some(match v {
        "requirepath" => PlacementConstraints::REQUIRE_PATH.raw_flag_bits() as u64,
        "requirehabitat" => PlacementConstraints::REQUIRE_HABITAT.raw_flag_bits() as u64,
        "requirewater" => PlacementConstraints::REQUIRE_WATER.raw_flag_bits() as u64,
        "requireland" => PlacementConstraints::REQUIRE_LAND.raw_flag_bits() as u64,
        "requirewall" => PlacementConstraints::REQUIRE_WALL.raw_flag_bits() as u64,
        "requireflat" => PlacementConstraints::REQUIRE_FLAT.raw_flag_bits() as u64,
        "allowoverlapscenery" => PlacementConstraints::ALLOW_OVERLAP_SCENERY.raw_flag_bits() as u64,
        _ => return None,
    })
}
pub(super) fn footprint_flag(v: &str) -> Option<u64> {
    Some(match v {
        "occupied" => FootprintCellFlags::OCCUPIED.raw_flag_bits() as u64,
        "walkable" => FootprintCellFlags::WALKABLE.raw_flag_bits() as u64,
        "entrance" => FootprintCellFlags::ENTRANCE.raw_flag_bits() as u64,
        "water" => FootprintCellFlags::WATER.raw_flag_bits() as u64,
        "foundationrequired" => FootprintCellFlags::FOUNDATION_REQUIRED.raw_flag_bits() as u64,
        _ => return None,
    })
}
pub(super) fn staff_job_flag(v: &str) -> Option<u64> {
    staff_job_kind(v)
        .ok()
        .map(|kind| u64::from(staff_job_bit(kind)))
}
pub(super) fn traversal_flag(v: &str) -> Option<u64> {
    Some(match v {
        "guest" => u64::from(FenceTraversalBlockingFlags::GUEST.raw_flag_bits()),
        "staff" => u64::from(FenceTraversalBlockingFlags::STAFF.raw_flag_bits()),
        "animal" => u64::from(FenceTraversalBlockingFlags::ANIMAL.raw_flag_bits()),
        "vehicle" => u64::from(FenceTraversalBlockingFlags::VEHICLE.raw_flag_bits()),
        "air" => u64::from(FenceTraversalBlockingFlags::AIR.raw_flag_bits()),
        "water" => u64::from(FenceTraversalBlockingFlags::WATER.raw_flag_bits()),
        _ => return None,
    })
}
pub(super) fn tranquilizer_flag(v: &str) -> Option<u64> {
    Some(match v {
        "escaped" => u64::from(TranquilizerEligibility::ESCAPED.raw_flag_bits()),
        "rampaging" => u64::from(TranquilizerEligibility::RAMPAGING.raw_flag_bits()),
        "contained" => u64::from(TranquilizerEligibility::CONTAINED.raw_flag_bits()),
        _ => return None,
    })
}
pub(super) fn catalogue_flag(v: &str) -> Option<u64> {
    Some(match v {
        "purchasable" => u64::from(CatalogueFilterFlags::PURCHASABLE.raw_flag_bits()),
        "buildable" => u64::from(CatalogueFilterFlags::BUILDABLE.raw_flag_bits()),
        "adoptable" => u64::from(CatalogueFilterFlags::ADOPTABLE.raw_flag_bits()),
        "hireable" => u64::from(CatalogueFilterFlags::HIREABLE.raw_flag_bits()),
        "expansion" => u64::from(CatalogueFilterFlags::EXPANSION.raw_flag_bits()),
        "modded" => u64::from(CatalogueFilterFlags::MODDED.raw_flag_bits()),
        "hiddenuntilunlocked" => {
            u64::from(CatalogueFilterFlags::HIDDEN_UNTIL_UNLOCKED.raw_flag_bits())
        }
        _ => return None,
    })
}
