use super::source_element_tree_search::find_first_authored_family_variant_with_nonempty_attribute;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_flag_vocabulary::traversal_flag;
use super::world_definition_source_value_reading_and_conversion::{
    asset, bool_or, flags, id, number_or, parse, required_number,
};
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::fences_and_gates::{
    FenceDefinition, FenceGatePolicy, FenceSegmentPrefabs, FenceTraversalBlockingFlags,
};
use openzt2_game_data::world_definitions::paths_and_tile_surfaces::{
    GuestPathDefinition, GuestPathTileSurfaceProfile,
};

pub(super) fn bind_fence(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let traversal_blocking_flag_bits =
        flags(record.value(&["blocks", "traversalMask"]), traversal_flag)? as u16;
    let traversal_blocking_flags = FenceTraversalBlockingFlags::from_raw_flag_bits(
        traversal_blocking_flag_bits,
    )
    .ok_or_else(|| {
        BindError::record(
            record,
            format!("invalid fence traversal blocking flags {traversal_blocking_flag_bits:#x}"),
        )
    })?;
    output.document.fences.push(FenceDefinition {
        id: id(record.key),
        object: asset(record, &["object", "entity"]),
        segment_length_cm: number_or(record, &["segmentLengthCm", "lengthCm"], 300)?,
        height_cm: required_number(record, &["heightCm", "height"])?,
        strength: number_or(record, &["strength"], 0)?,
        blocks: traversal_blocking_flags,
        gate: asset(record, &["gate", "gateDefinition"]),
        post_prefab: asset(record, &["postPrefab", "post"]),
        segments: FenceSegmentPrefabs {
            cardinal_straight: asset(
                record,
                &[
                    "cardinalStraightPrefab",
                    "straightPrefab",
                    "fence90",
                    "segmentPrefab",
                ],
            ),
            diagonal_straight: asset(
                record,
                &["diagonalStraightPrefab", "diagonalPrefab", "fence45"],
            ),
            cardinal_curve_90: asset(
                record,
                &["cardinalCurve90Prefab", "curve90Prefab", "fence90curve90"],
            ),
            diagonal_curve_90: asset(record, &["diagonalCurve90Prefab", "fence45curve90"]),
            cardinal_curve_135: asset(
                record,
                &[
                    "cardinalCurve135Prefab",
                    "curve135Prefab",
                    "fence90curve135",
                ],
            ),
            diagonal_curve_135: asset(record, &["diagonalCurve135Prefab", "fence45curve135"]),
        },
        gate_policy: FenceGatePolicy {
            prefab: asset(record, &["gatePrefab", "gate", "gateDefinition"]),
            open_animation: asset(record, &["gateOpenAnimation", "openAnimation"]),
            close_animation: asset(record, &["gateCloseAnimation", "closeAnimation"]),
            trigger_distance_cm: number_or(
                record,
                &["gateTriggerDistanceCm", "triggerDistanceCm"],
                0,
            )?,
            auto_close_ticks: number_or(record, &["gateAutoCloseTicks", "autoCloseTicks"], 0)?,
        },
    });
    Ok(())
}

pub(super) fn bind_path(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let path = record.descendant_named("ZTPath");
    let surface_texture = path
        .and_then(|path| path.attribute_named_any(&["texture"]))
        .map(id)
        .unwrap_or_default();
    let curb = path
        .and_then(|path| path.attribute_named_any(&["curb"]))
        .map(id)
        .unwrap_or_default();
    output.document.paths.push(GuestPathDefinition {
        id: id(record.key),
        object: asset(record, &["object", "entity"]),
        surface_texture,
        curb,
        width_cm: path_width_cm(record, resolved_source_records)?,
        capacity: number_or(record, &["capacity"], 0)?,
        speed_permille: number_or(record, &["speedPermille", "speedMultiplier"], 1000)?,
        elevated: bool_or(record, &["elevated", "isElevated"], false)?,
        support_prefab: asset(record, &["supportPrefab", "support"]),
        curve_support_prefab: asset(record, &["curveSupportPrefab", "supportCurvePrefab"]),
        support_column: None,
        curve_support_column: None,
        max_support_grade_permille: number_or(
            record,
            &["maxSupportGradePermille", "supportGradePermille"],
            1000,
        )?,
        support_headroom_cm: number_or(
            record,
            &["supportHeadroomCm", "minimumSupportClearanceCm"],
            0,
        )?,
        surface: GuestPathTileSurfaceProfile {
            height_cm: [
                number_or(record, &["surfaceNorthWestCm", "northWestHeightCm"], 0)?,
                number_or(record, &["surfaceNorthEastCm", "northEastHeightCm"], 0)?,
                number_or(record, &["surfaceSouthEastCm", "southEastHeightCm"], 0)?,
                number_or(record, &["surfaceSouthWestCm", "southWestHeightCm"], 0)?,
                number_or(record, &["surfaceCentreCm", "centerHeightCm"], 0)?,
            ],
        },
    });
    Ok(())
}

pub(super) fn path_width_cm(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
) -> Result<u16, BindError> {
    if let Some(value) = record.value(&["widthCm"]) {
        return parse(value, record, "widthCm");
    }
    let variant = find_first_authored_family_variant_with_nonempty_attribute(
        record,
        resolved_source_records,
        "BFTerrainDecalComponent",
        "dwidth",
    );
    let Some(decal) = record
        .descendant_named("BFTerrainDecalComponent")
        .or_else(|| {
            variant
                .as_ref()
                .and_then(|variant| variant.descendant_named("BFTerrainDecalComponent"))
        })
    else {
        // Ordinary ground paths omit decal dimensions because the original
        // path tool owns a fixed six-quarter-metre tile. Only authored decal
        // components override that engine width.
        return Ok(150);
    };
    let width = decal
        .attribute_named_any(&["dwidth"])
        .and_then(parse_blue_fang_source_numeric_lexeme::<f64>)
        .ok_or_else(|| BindError::record(record, "path terrain decal has no numeric dwidth"))?;
    let height = decal
        .attribute_named_any(&["dheight"])
        .and_then(parse_blue_fang_source_numeric_lexeme::<f64>)
        .ok_or_else(|| BindError::record(record, "path terrain decal has no numeric dheight"))?;
    // Terrain-decal dimensions use quarter-metre units. The small authored
    // oversize (6.01) prevents cracks and deliberately rounds to a 1.50 m
    // topology tile.
    let width_cm = (width * 25.0).round();
    let height_cm = (height * 25.0).round();
    if !width_cm.is_finite()
        || width_cm <= 0.0
        || width_cm > f64::from(u16::MAX)
        || width_cm != height_cm
    {
        return Err(BindError::record(
            record,
            "path terrain decal does not define one positive square width",
        ));
    }
    Ok(width_cm as u16)
}
