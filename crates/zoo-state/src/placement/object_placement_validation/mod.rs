//! Pure geometry and authored-rule evaluation for object placement.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::object_placement::placement_stomp_rules::PlacementStompAction;
use openzt2_game_data::world_definitions::object_placement::FootprintCell;
use openzt2_game_data::world_definitions::object_placement::FootprintCellFlags;
use openzt2_game_data::world_definitions::object_placement::PlaceableDefinition;
use openzt2_game_data::world_definitions::object_placement::PlacementConstraints;
use openzt2_game_data::world_definitions::world_objects::WorldObjectKind;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::construction::construction_interaction_types::PlacementFailure;
use crate::construction::construction_interaction_types::PlacementValidity;
use crate::economy::money_types::Money;
use crate::terrain::terrain_world_sampling_types::TerrainPoint;

use crate::placement::object_placement_geometry::ObjectPlacementGeometry;
use crate::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;

const POSITION_EPSILON_M: f32 = 0.01;
const ROTATION_EPSILON_RADIANS: f32 = 0.002;

#[derive(Debug, Clone, Copy)]
pub struct ObjectPlacementAuthoritativeFacts {
    pub habitat: Option<Entity>,
    pub unlocked: bool,
    pub affordable: bool,
    pub topology_valid: bool,
    pub headroom_valid: bool,
    pub surface_valid: bool,
}

pub fn object_placement_cell_is_blocked_by_existing_object(
    placement: &PlacedObjectFootprintOccupancyIndex,
    cell: IVec2,
    ignored: Option<Entity>,
    definition: &PlaceableDefinition,
    catalogue: WorldDefinitionsView<'_>,
    mut definition_for: impl FnMut(Entity) -> Option<openzt2_game_data::AssetId>,
) -> bool {
    placement.entities_occupying_cell(cell).any(|entity| {
        Some(entity) != ignored
            && object_placement_stomp_action(definition, catalogue, definition_for(entity))
                != PlacementStompAction::Allow
    })
}

pub fn object_placement_stomp_action(
    definition: &PlaceableDefinition,
    catalogue: WorldDefinitionsView<'_>,
    target: Option<openzt2_game_data::AssetId>,
) -> PlacementStompAction {
    let Some(object) = target.and_then(|target| catalogue.find_object(target)) else {
        return PlacementStompAction::Prevent;
    };
    let target = catalogue.placement_target_facts(object.id);
    let types = target.map_or(object.behavior_type_identifiers.as_slice(), |target| {
        target.types.as_slice()
    });
    if let Some(action) = definition
        .stomp
        .action_for_target(types, target.map(|target| &target.properties))
    {
        return action;
    }
    if definition
        .constraints
        .contains_all(PlacementConstraints::ALLOW_OVERLAP_SCENERY)
        && matches!(object.kind, WorldObjectKind::Scenery)
    {
        PlacementStompAction::Allow
    } else {
        PlacementStompAction::Prevent
    }
}

pub fn validate_object_placement_footprint_cells<'a>(
    geometry: impl Into<ObjectPlacementGeometry<'a>>,
    footprint: &[FootprintCell],
    transform: &Transform,
    terrain: impl FnMut(Vec2) -> Option<TerrainPoint>,
    occupied: impl FnMut(IVec2) -> bool,
    facts: ObjectPlacementAuthoritativeFacts,
) -> PlacementValidity {
    let geometry = geometry.into();
    let definition = geometry.definition;
    let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, transform)
    else {
        return PlacementValidity::Invalid(PlacementFailure::AuthoredRule(definition.id));
    };
    let Some(origin) = calculate_object_placement_footprint_origin(geometry, transform, turns)
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

pub fn validate_object_placement_over_terrain_samples(
    definition: &PlaceableDefinition,
    cells: impl Iterator<Item = IVec2>,
    mut terrain: impl FnMut(Vec2) -> Option<TerrainPoint>,
    mut occupied: impl FnMut(IVec2) -> bool,
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
        let water_depth = point
            .water_height_m
            .map(|water| water - point.height_m)
            .filter(|depth| *depth > POSITION_EPSILON_M);
        let in_water = water_depth.is_some();
        // The source refuses objects without `waterPlacement` in water ("You
        // cannot place %s in water") and water deeper than an authored
        // `maximumDepth` ("The water there is too deep for %s").
        let surface = definition.surface_policy;
        if water_depth.is_some_and(|depth| {
            !surface.water
                || surface.maximum_depth_metres > 0.0 && depth > surface.maximum_depth_metres
        }) {
            return PlacementValidity::Invalid(PlacementFailure::InvalidHabitat);
        }
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
    if !facts.surface_valid {
        return PlacementValidity::Invalid(PlacementFailure::InvalidHabitat);
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

pub fn calculate_object_placement_footprint_origin<'a>(
    geometry: impl Into<ObjectPlacementGeometry<'a>>,
    transform: &Transform,
    eighth_turns: u8,
) -> Option<IVec2> {
    transform.translation.is_finite().then(|| {
        let pivot_cm = geometry.into().pivot_cm(eighth_turns);
        let pivot = Vec2::new(f32::from(pivot_cm[0]), f32::from(pivot_cm[1])) / 100.0;
        let rotated_pivot = rotate_object_placement_point_by_quarter_turns(pivot, eighth_turns / 2);
        (Vec2::new(transform.translation.x, transform.translation.z) - rotated_pivot)
            .round()
            .as_ivec2()
    })
}

pub fn snap_object_placement_transform_to_authored_grid<'a>(
    geometry: impl Into<ObjectPlacementGeometry<'a>>,
    mut transform: Transform,
) -> Option<Transform> {
    let geometry = geometry.into();
    let turns = calculate_authored_object_placement_eighth_turns(geometry.definition, &transform)?;
    let origin = calculate_object_placement_footprint_origin(geometry, &transform, turns)?;
    let pivot_cm = geometry.pivot_cm(turns);
    let pivot = Vec2::new(f32::from(pivot_cm[0]), f32::from(pivot_cm[1])) / 100.0;
    let rotated_pivot = rotate_object_placement_point_by_quarter_turns(pivot, turns / 2);
    let snapped = origin.as_vec2() + rotated_pivot;
    transform.translation.x = snapped.x;
    transform.translation.z = snapped.y;
    Some(transform)
}

pub fn calculate_authored_object_placement_eighth_turns(
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

pub const fn rotate_object_placement_cell_by_quarter_turns(
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

pub fn rotate_object_placement_point_by_quarter_turns(point: Vec2, quarter_turns: u8) -> Vec2 {
    // Bevy's positive Y yaw maps +X toward -Z. Both footprint pivots and
    // presentation offsets use this same XZ convention as the spawned model.
    match quarter_turns & 3 {
        0 => point,
        1 => Vec2::new(point.y, -point.x),
        2 => -point,
        _ => Vec2::new(-point.y, point.x),
    }
}

pub fn calculate_terrain_slope_permille(normal: Vec3) -> u32 {
    if !normal.is_finite() || normal.y <= 0.0 {
        return u32::MAX;
    }
    ((Vec2::new(normal.x, normal.z).length() / normal.y) * 1000.0)
        .round()
        .clamp(0.0, u32::MAX as f32) as u32
}
