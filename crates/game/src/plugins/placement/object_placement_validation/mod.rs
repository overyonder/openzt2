//! Pure geometry and authored-rule evaluation for object placement.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::{
    object_placement::{
        FootprintCell, FootprintCellFlags, PlaceableDefinition, PlacementConstraints,
    },
    world_objects::WorldObjectKind,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_interaction_types::PlacementValidity;
use crate::plugins::economy::money_types::Money;
use crate::plugins::terrain::terrain_world_sampling_types::TerrainPoint;

use super::PlacedObjectFootprintOccupancyIndex;

const POSITION_EPSILON_M: f32 = 0.01;
const ROTATION_EPSILON_RADIANS: f32 = 0.002;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ObjectPlacementAuthoritativeFacts {
    pub habitat: Option<Entity>,
    pub unlocked: bool,
    pub affordable: bool,
    pub topology_valid: bool,
    pub headroom_valid: bool,
}

pub(crate) fn object_placement_cell_is_blocked_by_existing_object(
    placement: &PlacedObjectFootprintOccupancyIndex,
    cell: IVec2,
    ignored: Option<Entity>,
    definition: &PlaceableDefinition,
    catalogue: WorldDefinitionsView<'_>,
    mut definition_for: impl FnMut(Entity) -> Option<openzt2_game_data::AssetId>,
) -> bool {
    let allows_scenery = definition
        .constraints
        .contains_all(PlacementConstraints::ALLOW_OVERLAP_SCENERY);
    placement.entities_occupying_cell(cell).any(|entity| {
        if Some(entity) == ignored {
            return false;
        }
        !allows_scenery
            || definition_for(entity)
                .and_then(|definition| catalogue.find_object(definition))
                .is_none_or(|object| !matches!(&object.kind, WorldObjectKind::Scenery))
    })
}

pub(crate) fn validate_object_placement_footprint_cells(
    definition: &PlaceableDefinition,
    footprint: &[FootprintCell],
    transform: &Transform,
    terrain: impl Fn(Vec2) -> Option<TerrainPoint>,
    occupied: impl Fn(IVec2) -> bool,
    facts: ObjectPlacementAuthoritativeFacts,
) -> PlacementValidity {
    let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, transform)
    else {
        return PlacementValidity::Invalid(PlacementFailure::AuthoredRule(definition.id));
    };
    let Some(origin) = calculate_object_placement_footprint_origin(definition, transform, turns)
    else {
        return PlacementValidity::Invalid(PlacementFailure::OutsideMap);
    };
    let cells = footprint
        .iter()
        .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
        .map(|cell| {
            let local = IVec2::new(cell.offset[0] as i32, cell.offset[1] as i32);
            origin + rotate_object_placement_cell_by_quarter_turns(local, turns / 2)
        });
    validate_object_placement_over_terrain_samples(definition, cells, terrain, occupied, facts)
}

pub(crate) fn validate_object_placement_over_terrain_samples(
    definition: &PlaceableDefinition,
    cells: impl Iterator<Item = IVec2>,
    terrain: impl Fn(Vec2) -> Option<TerrainPoint>,
    occupied: impl Fn(IVec2) -> bool,
    facts: ObjectPlacementAuthoritativeFacts,
) -> PlacementValidity {
    // The order is observable: source affordability and research checks precede
    // geometric predicates, followed by habitat/topology-specific predicates.
    if !facts.affordable {
        return PlacementValidity::Invalid(PlacementFailure::Unaffordable);
    }
    if !facts.unlocked {
        return PlacementValidity::Invalid(PlacementFailure::Locked);
    }

    let constraints = definition.constraints;
    let flattens_terrain =
        constraints.contains_all(PlacementConstraints::FLATTEN_TERRAIN_TO_PLACEMENT_HEIGHT);
    let maximum_slope = if constraints.contains_all(PlacementConstraints::REQUIRE_FLAT) {
        0
    } else {
        u32::from(definition.max_slope_permille)
    };
    let mut visited = false;
    for cell in cells {
        visited = true;
        // The callback reports a disallowed collision. A caller implementing
        // the scenery-only exception must inspect the collided entity's typed
        // definition; the flag must never permit arbitrary overlap.
        if occupied(cell) {
            return PlacementValidity::Invalid(PlacementFailure::Occupied);
        }
        let Some(point) = terrain(cell.as_vec2()) else {
            return PlacementValidity::Invalid(PlacementFailure::OutsideMap);
        };
        if !flattens_terrain && calculate_terrain_slope_permille(point.normal) > maximum_slope {
            return PlacementValidity::Invalid(PlacementFailure::TooSteep);
        }
        let in_water = point
            .water_height_m
            .is_some_and(|water| water > point.height_m + POSITION_EPSILON_M);
        if constraints.contains_all(PlacementConstraints::REQUIRE_WATER) && !in_water
            || constraints.contains_all(PlacementConstraints::REQUIRE_LAND) && in_water
        {
            return PlacementValidity::Invalid(PlacementFailure::InvalidHabitat);
        }
    }
    if !visited {
        return PlacementValidity::Invalid(PlacementFailure::AuthoredRule(definition.id));
    }
    if !facts.headroom_valid && definition.minimum_headroom_metres > 0.0 {
        return PlacementValidity::Invalid(PlacementFailure::NoHeadroom);
    }
    if constraints.contains_all(PlacementConstraints::REQUIRE_HABITAT) && facts.habitat.is_none() {
        return PlacementValidity::Invalid(PlacementFailure::InvalidHabitat);
    }
    if !facts.topology_valid
        && constraints.contains_any(
            PlacementConstraints::REQUIRE_PATH
                .with_additional_flags(PlacementConstraints::REQUIRE_WALL),
        )
    {
        return PlacementValidity::Invalid(PlacementFailure::InvalidTopology);
    }
    PlacementValidity::Valid {
        cost: Money(definition.price_cents),
    }
}

pub(crate) fn calculate_object_placement_footprint_origin(
    definition: &PlaceableDefinition,
    transform: &Transform,
    eighth_turns: u8,
) -> Option<IVec2> {
    transform.translation.is_finite().then(|| {
        let pivot_cm = if eighth_turns & 1 == 0 {
            &definition.pivot_cm
        } else {
            &definition.diagonal_pivot_cm
        };
        let pivot = Vec2::new(f32::from(pivot_cm[0]), f32::from(pivot_cm[1])) / 100.0;
        let rotated_pivot = rotate_object_placement_point_by_quarter_turns(pivot, eighth_turns / 2);
        (Vec2::new(transform.translation.x, transform.translation.z) - rotated_pivot)
            .round()
            .as_ivec2()
    })
}

pub(crate) fn snap_object_placement_transform_to_authored_grid(
    definition: &PlaceableDefinition,
    mut transform: Transform,
) -> Option<Transform> {
    let turns = calculate_authored_object_placement_eighth_turns(definition, &transform)?;
    let origin = calculate_object_placement_footprint_origin(definition, &transform, turns)?;
    let pivot_cm = if turns & 1 == 0 {
        &definition.pivot_cm
    } else {
        &definition.diagonal_pivot_cm
    };
    let pivot = Vec2::new(f32::from(pivot_cm[0]), f32::from(pivot_cm[1])) / 100.0;
    let rotated_pivot = rotate_object_placement_point_by_quarter_turns(pivot, turns / 2);
    let snapped = origin.as_vec2() + rotated_pivot;
    transform.translation.x = snapped.x;
    transform.translation.z = snapped.y;
    Some(transform)
}

pub(crate) fn calculate_authored_object_placement_eighth_turns(
    definition: &PlaceableDefinition,
    transform: &Transform,
) -> Option<u8> {
    if !transform.rotation.is_finite() {
        return None;
    }
    let (yaw, pitch, roll) = transform.rotation.to_euler(EulerRot::YXZ);
    if pitch.abs() > ROTATION_EPSILON_RADIANS || roll.abs() > ROTATION_EPSILON_RADIANS {
        return None;
    }
    let degrees = yaw.to_degrees().rem_euclid(360.0);
    let increment = definition.rotation_increment_degrees;
    if increment != 0 {
        let authored_step = degrees / f32::from(increment);
        if (authored_step - authored_step.round()).abs()
            > ROTATION_EPSILON_RADIANS.to_degrees() / f32::from(increment)
        {
            return None;
        }
    }
    let eighth = degrees / 45.0;
    ((eighth - eighth.round()).abs() <= ROTATION_EPSILON_RADIANS.to_degrees() / 45.0)
        .then_some((eighth.round() as i32).rem_euclid(8) as u8)
}

pub(crate) const fn rotate_object_placement_cell_by_quarter_turns(
    cell: IVec2,
    quarter_turns: u8,
) -> IVec2 {
    match quarter_turns & 3 {
        0 => cell,
        // Occupancy keys are the lower-left corner of unit cells, not points.
        // Rotating a cell center and flooring reverses the axis with -value-1.
        1 => IVec2::new(cell.y, -cell.x - 1),
        2 => IVec2::new(-cell.x - 1, -cell.y - 1),
        _ => IVec2::new(-cell.y - 1, cell.x),
    }
}

pub(super) fn rotate_object_placement_point_by_quarter_turns(
    point: Vec2,
    quarter_turns: u8,
) -> Vec2 {
    // Bevy's positive Y yaw maps +X toward -Z. Both footprint pivots and
    // presentation offsets use this same XZ convention as the spawned model.
    match quarter_turns & 3 {
        0 => point,
        1 => Vec2::new(point.y, -point.x),
        2 => -point,
        _ => Vec2::new(-point.y, point.x),
    }
}

pub(crate) fn calculate_terrain_slope_permille(normal: Vec3) -> u32 {
    if !normal.is_finite() || normal.y <= 0.0 {
        return u32::MAX;
    }
    ((Vec2::new(normal.x, normal.z).length() / normal.y) * 1000.0)
        .round()
        .clamp(0.0, u32::MAX as f32) as u32
}
