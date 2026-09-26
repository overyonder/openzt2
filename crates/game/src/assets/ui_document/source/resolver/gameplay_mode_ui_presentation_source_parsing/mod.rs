//! Gameplay-mode cursor and placement-preview source parsing.

use crate::assets::{
    source_document::ordered_source_document_types::{
        OrderedSourceDocument, OrderedSourceDocumentChildren, OrderedSourceDocumentNode,
    },
    ui_document::source::lower::authored_ui_document_lowering::{
        AuthoredUiInteractionCursors, AuthoredUiPlacementPreview,
    },
};

use super::super::ui_source_document_gap::{
    UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
};

fn find_nested_ordered_source_document_node_by_name<'a>(
    children: &'a OrderedSourceDocumentChildren,
    tag: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    children.iter().find_map(|node| {
        node.name
            .eq_ignore_ascii_case(tag)
            .then_some(node)
            .or_else(|| find_nested_ordered_source_document_node_by_name(&node.children, tag))
    })
}

pub(super) fn parse_authored_interaction_cursors_from_gameplay_mode_document(
    document: &OrderedSourceDocument,
) -> Result<AuthoredUiInteractionCursors, UiSourceDocumentGap> {
    let required = |tag: &str, attribute: &str| {
        find_nested_ordered_source_document_node_by_name(&document.root.children, tag)
            .and_then(|node| node.attribute(attribute))
            .map(str::to_owned)
            .ok_or_else(|| UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: document.path.key(),
                span: document.root.span,
                message: format!("{tag} has no authored {attribute} cursor"),
            })
    };
    Ok(AuthoredUiInteractionCursors {
        overhead: required("ZTOverheadMode", "cursor")?,
        placement_default: required("ZTPlacementMode", "defaultCursor")?,
        placement_pickup: required("ZTPlacementMode", "pickupCursor")?,
        placement_rotate: required("ZTPlacementMode", "rotateCursor")?,
        fence_default: required("ZTFenceMode", "cursor")?,
        fence_gate: required("ZTFenceMode", "gateCursor")?,
        path: required("ZTPathMode", "cursor")?,
        elevated_path: required("ZTElevatedPathMode", "cursor")?,
        biome_default: required("ZTBiomeMode", "cursor")?,
        biome_paint: required("ZTBiomeMode", "paintCursor")?,
        biome_invalid: required("ZTBiomeMode", "invalidCursor")?,
        terrain: required("ZTTerrainCommandReader", "cursor")?,
        tank: required("ZTTankCommandReader", "cursor")?,
        selection_default: required("ZTSelectionMode", "defaultCursor")?,
        selection_pickup: required("ZTSelectionMode", "pickupCursor")?,
        selection_rotate: required("ZTSelectionMode", "rotateCursor")?,
        delete: required("ZTDeleteMode", "cursor")?,
    })
}

pub(super) fn parse_authored_placement_preview_from_gameplay_mode_document(
    document: &OrderedSourceDocument,
) -> Result<AuthoredUiPlacementPreview, UiSourceDocumentGap> {
    fn direct_child<'a>(
        parent: &'a OrderedSourceDocumentNode,
        name: &str,
    ) -> Option<&'a OrderedSourceDocumentNode> {
        parent
            .children
            .iter()
            .find(|node| node.name.eq_ignore_ascii_case(name))
    }
    let gap = |message: String| UiSourceDocumentGap {
        family: UiSourceDocumentFamily::Ui,
        kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
        virtual_path: document.path.key(),
        span: document.root.span,
        message,
    };
    let placement = find_nested_ordered_source_document_node_by_name(
        &document.root.children,
        "ZTPlacementMode",
    )
    .ok_or_else(|| gap("gameplay mode document has no ZTPlacementMode".into()))?;
    let fence =
        find_nested_ordered_source_document_node_by_name(&document.root.children, "ZTFenceMode")
            .ok_or_else(|| gap("gameplay mode document has no ZTFenceMode".into()))?;
    let required = |node: &OrderedSourceDocumentNode, attribute: &str| {
        node.attribute(attribute)
            .map(str::to_owned)
            .ok_or_else(|| gap(format!("{} has no authored {attribute}", node.name)))
    };
    let color =
        |owner: &OrderedSourceDocumentNode, name: &str| -> Result<[u8; 4], UiSourceDocumentGap> {
            let colors = direct_child(owner, "colors")
                .ok_or_else(|| gap(format!("{} has no authored colors child", owner.name)))?;
            let node = direct_child(colors, name)
                .ok_or_else(|| gap(format!("{} has no authored {name} child", colors.name)))?;
            let channel = |attribute: &str, fallback: u8| {
                node.attribute(attribute).map_or(Ok(fallback), |value| {
                    value.parse::<u8>().map_err(|_| {
                        gap(format!("{name} has invalid {attribute} channel {value:?}"))
                    })
                })
            };
            Ok([
                channel("r", 0)?,
                channel("g", 0)?,
                channel("b", 0)?,
                channel("a", 255)?,
            ])
        };
    let grid = direct_child(placement, "grid")
        .ok_or_else(|| gap(format!("{} has no authored grid child", placement.name)))?;
    let grid_entry = |name: &str| -> Result<(String, String, f32), UiSourceDocumentGap> {
        let node = direct_child(grid, name)
            .ok_or_else(|| gap(format!("{} has no authored {name} child", grid.name)))?;
        let radius = required(node, "radius")?
            .trim_end_matches(['f', 'F'])
            .parse::<f32>()
            .map_err(|_| gap(format!("{name} has an invalid authored radius")))?;
        if !radius.is_finite() || radius <= 0.0 {
            return Err(gap(format!(
                "{name} has an authored radius which is not finite and positive"
            )));
        }
        Ok((
            required(node, "cardinalImage")?,
            required(node, "diagonalImage")?,
            radius,
        ))
    };
    let large = grid_entry("large")?;
    let medium = grid_entry("medium")?;
    let small = grid_entry("small")?;
    Ok(AuthoredUiPlacementPreview {
        footprint_valid: required(placement, "footprintplace_yes")?,
        footprint_valid_small: required(placement, "footprintplace_yes_small")?,
        footprint_invalid: required(placement, "footprintplace_no")?,
        footprint_invalid_small: required(placement, "footprintplace_no_small")?,
        grid_large_cardinal: large.0,
        grid_large_diagonal: large.1,
        grid_large_radius: large.2,
        grid_medium_cardinal: medium.0,
        grid_medium_diagonal: medium.1,
        grid_medium_radius: medium.2,
        grid_small_cardinal: small.0,
        grid_small_diagonal: small.1,
        grid_small_radius: small.2,
        model_valid_srgba: color(placement, "place_yes")?,
        model_invalid_srgba: color(placement, "place_no")?,
        fence_valid_srgba: color(fence, "place_yes")?,
        fence_invalid_srgba: color(fence, "place_no")?,
        footprint_valid_srgba: color(placement, "footprintplace_yes")?,
        footprint_invalid_srgba: color(placement, "footprintplace_no")?,
        grid_srgba: color(placement, "grid")?,
    })
}
