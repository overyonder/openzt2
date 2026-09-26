use super::authored_placement_footprint_types::AuthoredPlacementFootprint;
use super::object_and_placeable_source_lowering::moving_footprint;
use super::source_element_tree_search::{
    authored_type_family_component_attribute, authored_type_family_components, find_descendant,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    element_number, id, simple_error,
};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::object_placement::{
    FootprintCell, FootprintCellFlags, PlaceableDefinition, PlacementConstraints,
};
use openzt2_game_data::AssetId;

#[cfg(test)]
mod tests;

fn authored_headroom_policy(record: &RecordView<'_, '_>) -> Result<(f32, bool), BindError> {
    let height =
        authored_type_family_component_attribute(record, "ZTPlacementData", "minimumHeadroom")
            .map(|value| {
                value
                    .parse::<f32>()
                    .map_err(|_| BindError::record(record, "invalid minimumHeadroom"))
            })
            .transpose()?
            .unwrap_or(-1.0);
    if !height.is_finite() {
        return Err(BindError::record(record, "non-finite minimumHeadroom"));
    }
    let apply =
        authored_type_family_component_attribute(record, "ZTPlacementData", "applyHeightModifier")
            .map(
                |value| match canonicalize_source_document_record_key(value).as_str() {
                    "true" | "yes" | "1" => Ok(true),
                    "false" | "no" | "0" => Ok(false),
                    _ => Err(BindError::record(record, "invalid applyHeightModifier")),
                },
            )
            .transpose()?
            .unwrap_or(true);
    Ok((if height == -1.0 { 0.0 } else { height }, apply))
}

/// A ground path under the footprint is an object collision when the placed
/// object's nearest authored `stompData` prevents the path's type ancestry.
/// Types without a delete, prevent or allow rule are prevented. Prevention only
/// collides for `gridSnap` objects, which defaults to false, because path
/// decals have no collision component for other objects to overlap.
fn authored_ground_paths_block_placement(record: &RecordView<'_, '_>) -> Result<bool, BindError> {
    let grid_snap = authored_type_family_component_attribute(record, "ZTPlacementData", "gridSnap")
        .map(
            |value| match canonicalize_source_document_record_key(value).as_str() {
                "true" | "yes" | "1" => Ok(true),
                "false" | "no" | "0" => Ok(false),
                _ => Err(BindError::record(
                    record,
                    format!("invalid ZTPlacementData gridSnap value {value:?}"),
                )),
            },
        )
        .transpose()?
        .unwrap_or(false);
    if !grid_snap {
        return Ok(false);
    }
    let stomp_data = authored_type_family_components(record, "ZTPlacementData")
        .into_iter()
        .find_map(|placement_data| find_descendant(placement_data, "stompData"));
    let action_is_named = |action: &OrderedSourceDocumentNode, name: &str| {
        source_document_names_are_semantically_equal(action.name.as_str(), name)
    };
    // Ground path types are `entity/path/<surface>`. Shipped rules name only
    // the shared `path` type, so the surface-specific token is not retained.
    // Later rules for the same type replace earlier ones. Rules with nested
    // children match entity-data property values rather than entity types.
    let path_is_allowed = ["path", "entity"]
        .into_iter()
        .find_map(|target_type| {
            stomp_data?
                .element_children()
                .rfind(|action| {
                    ["delete", "prevent", "allow"]
                        .into_iter()
                        .any(|name| action_is_named(action, name))
                        && action.element_children().any(|target| {
                            target.element_children().next().is_none()
                                && source_document_names_are_semantically_equal(
                                    target.name.as_str(),
                                    target_type,
                                )
                        })
                })
                .map(|action| !action_is_named(action, "prevent"))
        })
        .unwrap_or(false);
    Ok(!path_is_allowed)
}

pub(super) fn bind_authored_placeable(
    record: &RecordView<'_, '_>,
    bounds: Option<AuthoredPlacementFootprint>,
    automatic_footprint: bool,
    price_cents: i64,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<u32, BindError> {
    let placement_data = record.descendant_named("ZTPlacementData");
    let authored_cardinal = authored_footprint_dimensions(placement_data, "cfootprint")?;
    let authored_diagonal = authored_footprint_dimensions(placement_data, "dfootprint")?;
    let authored_grid = |dimensions: [f32; 2]| dimensions.map(|value| value.ceil() as i16);
    let authored_pivot =
        |dimensions: [f32; 2]| dimensions.map(|value| (value * 50.0).round() as i16);
    let (derived_dimensions, derived_pivot_cm) = if let Some(bounds) = bounds {
        let dimensions = [
            (bounds.maximum_xz[0] - bounds.minimum_xz[0])
                .ceil()
                .max(1.0),
            (bounds.maximum_xz[1] - bounds.minimum_xz[1])
                .ceil()
                .max(1.0),
        ];
        let pivot_cm = bounds.minimum_xz.map(|minimum| -minimum * 100.0);
        if dimensions
            .iter()
            .any(|value| !value.is_finite() || !(1.0..=f32::from(i16::MAX)).contains(value))
            || pivot_cm.iter().any(|value| {
                !value.is_finite() || *value < f32::from(i16::MIN) || *value > f32::from(i16::MAX)
            })
        {
            return Err(BindError::record(
                record,
                "authored footprint bounds do not fit the cell grid",
            ));
        }
        (
            dimensions.map(|value| value as i16),
            pivot_cm.map(|value| value.round() as i16),
        )
    } else {
        ([0; 2], [0; 2])
    };
    // An authored 0x0 cfootprint, used by lamps, bins and toys, occupies the
    // one grid cell containing the object's position.
    let authored_cardinal = authored_cardinal.map(|dimensions| {
        let grid = authored_grid(dimensions);
        if grid == [0; 2] {
            ([1; 2], [50; 2])
        } else {
            (grid, authored_pivot(dimensions))
        }
    });
    let (dimensions, pivot_cm) =
        authored_cardinal.unwrap_or((derived_dimensions, derived_pivot_cm));
    let (diagonal_dimensions, diagonal_pivot_cm) = authored_diagonal
        .map_or((dimensions, pivot_cm), |authored| {
            (authored_grid(authored), authored_pivot(authored))
        });
    let mut footprint = Vec::new();
    for z in 0..dimensions[1] {
        for x in 0..dimensions[0] {
            footprint.push(FootprintCell {
                offset: [x, z],
                flags: FootprintCellFlags::OCCUPIED,
            });
        }
    }
    let mut diagonal_footprint = Vec::new();
    for z in 0..diagonal_dimensions[1] {
        for x in 0..diagonal_dimensions[0] {
            diagonal_footprint.push(FootprintCell {
                offset: [x, z],
                flags: FootprintCellFlags::OCCUPIED,
            });
        }
    }
    let terrain_flatten =
        authored_type_family_component_attribute(record, "ZTPlacementData", "terrainFlatten")
            .map(
                |value| match canonicalize_source_document_record_key(value).as_str() {
                    "true" | "yes" | "1" => Ok(true),
                    "false" | "no" | "0" => Ok(false),
                    _ => Err(BindError::record(
                        record,
                        format!("invalid ZTPlacementData terrainFlatten value {value:?}"),
                    )),
                },
            )
            .transpose()?
            .unwrap_or(false);
    let (minimum_headroom_metres, apply_height_modifier) = authored_headroom_policy(record)?;
    output.document.placeables.push(PlaceableDefinition {
        id: id(record.key),
        weight: placement_data
            .map(|data| element_number(&data, &["weight"], 1.0))
            .transpose()?
            .unwrap_or(1.0),
        footprint,
        pivot_cm,
        diagonal_footprint,
        diagonal_pivot_cm,
        // The shipped placement mode owns cardinal and diagonal preview grids;
        // objects with both authored footprints therefore rotate by an eighth
        // turn and select the matching exact footprint.
        rotation_increment_degrees: if authored_diagonal.is_some() { 45 } else { 90 },
        constraints: if terrain_flatten {
            PlacementConstraints::FLATTEN_TERRAIN_TO_PLACEMENT_HEIGHT
        } else {
            PlacementConstraints::EMPTY
        },
        // Shipped placement has no slope or terrain-contact rejection.
        // terrainFlatten and ground fitting own the placed height.
        max_slope_permille: u16::MAX,
        minimum_headroom_metres,
        apply_height_modifier,
        price_cents,
        unlock: AssetId::default(),
        entrances: Vec::new(),
        moving_footprint: moving_footprint(record)?,
        automatic_footprint,
        ground_paths_block_placement: authored_ground_paths_block_placement(record)?,
    });
    Ok(0)
}

pub(super) fn authored_footprint_dimensions(
    placement: Option<&'_ OrderedSourceDocumentNode>,
    name: &str,
) -> Result<Option<[f32; 2]>, BindError> {
    let Some(footprint) = placement.and_then(|placement| find_descendant(placement, name)) else {
        return Ok(None);
    };
    let dimension = |attribute: &str| {
        footprint
            .attribute_named_any(&[attribute])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|value| {
                value.is_finite()
                    && *value >= 0.0
                    && value.ceil() <= f32::from(i16::MAX)
                    && value * 50.0 <= f32::from(i16::MAX)
            })
            .ok_or_else(|| simple_error(format!("{name} has invalid authored {attribute}")))
    };
    Ok(Some([dimension("width")?, dimension("height")?]))
}

pub(super) fn authored_placement_bounds_from_shadow_collision_or_dimensions(
    record: &RecordView<'_, '_>,
    dimensions: Option<[f32; 2]>,
) -> Option<AuthoredPlacementFootprint> {
    let shadow = record
        .descendant_named("BFMovingBlobShadowComponent")
        .or_else(|| {
            record
                .type_tokens()
                .iter()
                .rev()
                .filter_map(|ancestor| record.find_resolved_source_record_by_reference(ancestor))
                .find_map(|ancestor| ancestor.descendant_named("BFMovingBlobShadowComponent"))
        });
    let shadow_bounds = shadow.and_then(|shadow| {
        let width =
            parse_blue_fang_source_numeric_lexeme::<f32>(shadow.attribute_named_any(&["width"])?)?;
        let length = shadow
            .attribute_named_any(&["length"])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .unwrap_or(width);
        (width.is_finite() && width > 0.0 && length.is_finite() && length > 0.0).then_some(
            AuthoredPlacementFootprint {
                minimum_xz: [-width * 0.5, -length * 0.5],
                maximum_xz: [width * 0.5, length * 0.5],
            },
        )
    });
    shadow_bounds
        .or_else(|| {
            record
                .descendant_named("BFCollisionComponent")
                .and_then(|collision| collision.attribute_named_any(&["grid"]))
                .is_some_and(|grid| source_document_names_are_semantically_equal(grid, "footprint"))
                .then_some(AuthoredPlacementFootprint {
                    minimum_xz: [-0.125; 2],
                    maximum_xz: [0.125; 2],
                })
        })
        .or_else(|| {
            dimensions.map(|dimensions| AuthoredPlacementFootprint {
                minimum_xz: dimensions.map(|value| -value * 0.5),
                maximum_xz: dimensions.map(|value| value * 0.5),
            })
        })
}
