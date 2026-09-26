use crate::assets::source_document::path::AssetPath;
use crate::assets::source_document::ui::model::{
    SourceUiActiveState, SourceUiAlignment, SourceUiButton, SourceUiEvent, SourceUiGrid,
    SourceUiHitPolicy, SourceUiList, SourceUiListSource, SourceUiMetric, SourceUiMetricRect,
    SourceUiNode, SourceUiRegion, SourceUiSlider, SourceUiText, SourceUiVisual, SourceUiWidgetData,
    SourceUiWidgetKind, SourceUiWindow,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid, invalid_at, number, numeric_rect, parse_bool,
};
use crate::assets::ui_document::source::lower::visuals;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::node_layout::{
    UiNodeRegionAlignment, UiNodeRegionDefinition, UiNodeRegionMetric,
};
use openzt2_game_data::ui_document::node_style::{UiSystemFont, UiTextAlignment};
use std::collections::BTreeMap;
use std::io;

pub(super) fn collect_templates(root: &SourceUiNode) -> io::Result<BTreeMap<String, SourceUiNode>> {
    fn walk(node: &SourceUiNode, output: &mut BTreeMap<String, SourceUiNode>) -> io::Result<()> {
        if let Some(name) = &node.template_name {
            let key = name.to_ascii_lowercase();
            if output.insert(key, node.clone()).is_some() {
                return Err(invalid(format!("duplicate UI template {name:?}")));
            }
        }
        node.children
            .iter()
            .try_for_each(|child| walk(child, output))
    }
    let mut output = BTreeMap::new();
    walk(root, &mut output)?;
    Ok(output)
}

pub(super) fn resolve_template(
    authored: &SourceUiNode,
    templates: &BTreeMap<String, SourceUiNode>,
    stack: &mut Vec<String>,
    input: &AuthoredUiDocument,
) -> io::Result<SourceUiNode> {
    let Some(name) = &authored.template else {
        return Ok(authored.clone());
    };
    let key = name.to_ascii_lowercase();
    if stack.contains(&key) {
        return Err(invalid_at(
            input,
            format!("UI template cycle through {name:?}"),
        ));
    }
    let Some(base) = templates.get(&key) else {
        return Err(invalid_at(
            input,
            format!("UI template {name:?} is not present in the selected overlay"),
        ));
    };
    stack.push(key);
    let mut result = resolve_template(base, templates, stack, input)?;
    stack.pop();
    result.name = authored.name.clone().or(result.name);
    result.template = None;
    result.template_name = authored.template_name.clone().or(result.template_name);
    result.cursor = authored.cursor.clone().or(result.cursor);
    result.modal = authored.modal.or(result.modal);
    result.use_list_box_color_if_in_container = authored
        .use_list_box_color_if_in_container
        .or(result.use_list_box_color_if_in_container);
    result.clip_children |= authored.clip_children;
    result.always_hit = authored.always_hit.clone().or(result.always_hit);
    result.region = authored.region.clone().or(result.region);
    result.background_layout = authored
        .background_layout
        .clone()
        .or(result.background_layout);
    result.aspect = match (result.aspect.take(), authored.aspect.as_ref()) {
        (base, Some(overlay)) => Some(
            visuals::merge_source_ui_aspect_template_and_authored_override(base.as_ref(), overlay),
        ),
        (base, None) => base,
    };
    result.show_hide_animation = authored
        .show_hide_animation
        .clone()
        .or(result.show_hide_animation);
    result.notify_animation_completed = authored
        .notify_animation_completed
        .or(result.notify_animation_completed);
    result.help = authored.help.clone().or(result.help);
    result.text_format = authored.text_format.clone().or(result.text_format);
    if !authored.children.is_empty() {
        // A template instance adds its authored children beneath the cloned
        // template subtree. It does not replace that subtree. Scalable panel
        // templates keep their nine background pieces while the instance
        // contributes labels and controls; replacing the children erases the
        // panel itself whenever it contains content.
        result.children.extend(authored.children.iter().cloned());
    }
    if !authored.events.is_empty() {
        result.events = authored.events.clone();
    }
    if !authored.hotkeys.is_empty() {
        result.hotkeys = authored.hotkeys.clone();
    }
    result.state = authored.state.clone();
    let mut widget = match (&result.widget, &authored.widget) {
        (SourceUiWidgetData::Slider(base), SourceUiWidgetData::Slider(overlay)) => {
            SourceUiWidgetData::Slider(merge_authored_slider_template(base, overlay))
        }
        (SourceUiWidgetData::TypeList(base), SourceUiWidgetData::TypeList(overlay)) => {
            let mut list = overlay.clone();
            list.grid = merge_authored_grid_template(&base.grid, &overlay.grid);
            list.filter_fields = overlay
                .filter_fields
                .clone()
                .or_else(|| base.filter_fields.clone());
            list.root_type = overlay.root_type.clone().or_else(|| base.root_type.clone());
            SourceUiWidgetData::TypeList(list)
        }
        (SourceUiWidgetData::Layout(base), SourceUiWidgetData::Layout(overlay)) => {
            SourceUiWidgetData::Layout(merge_authored_grid_template(base, overlay))
        }
        (SourceUiWidgetData::List(base), SourceUiWidgetData::List(overlay)) => {
            SourceUiWidgetData::List(merge_authored_list_template(base, overlay))
        }
        (SourceUiWidgetData::Button(base), SourceUiWidgetData::Button(overlay)) => {
            SourceUiWidgetData::Button(merge_authored_button_template(base, overlay))
        }
        (SourceUiWidgetData::Text(base), SourceUiWidgetData::Text(overlay)) => {
            SourceUiWidgetData::Text(SourceUiText {
                auto_size: overlay.auto_size.or(base.auto_size),
                min_height: overlay.min_height.or(base.min_height),
                text_type: overlay.text_type.clone().or_else(|| base.text_type.clone()),
                text_format: overlay
                    .text_format
                    .clone()
                    .or_else(|| base.text_format.clone()),
                localization_id: overlay
                    .localization_id
                    .clone()
                    .or_else(|| base.localization_id.clone()),
                authored_string: overlay
                    .authored_string
                    .clone()
                    .or_else(|| base.authored_string.clone()),
            })
        }
        (SourceUiWidgetData::Window(base), SourceUiWidgetData::Window(overlay)) => {
            SourceUiWidgetData::Window(SourceUiWindow {
                title: overlay.title.clone().or_else(|| base.title.clone()),
                modal: overlay.modal.or(base.modal),
                draggable: overlay.draggable.or(base.draggable),
                horizontal: overlay.horizontal.or(base.horizontal),
                vertical: overlay.vertical.or(base.vertical),
                wheel_scroll: overlay.wheel_scroll.or(base.wheel_scroll),
                horizontal_scroll: overlay
                    .horizontal_scroll
                    .clone()
                    .or_else(|| base.horizontal_scroll.clone()),
                vertical_scroll: overlay
                    .vertical_scroll
                    .clone()
                    .or_else(|| base.vertical_scroll.clone()),
            })
        }
        _ => authored.widget.clone(),
    };
    merge_authored_widget_grid(&result.widget, &mut widget);
    result.widget = widget;
    result.kind = authored.kind.clone();
    Ok(result)
}

fn merge_authored_widget_grid(base: &SourceUiWidgetData, overlay: &mut SourceUiWidgetData) {
    let base = match base {
        SourceUiWidgetData::Layout(grid) | SourceUiWidgetData::ToggleSet { grid, .. } => grid,
        SourceUiWidgetData::List(list) | SourceUiWidgetData::DropList { list, .. } => &list.grid,
        SourceUiWidgetData::TypeList(list) => &list.grid,
        SourceUiWidgetData::TreeElement(tree) => &tree.grid,
        _ => return,
    };
    let overlay = match overlay {
        SourceUiWidgetData::Layout(grid) | SourceUiWidgetData::ToggleSet { grid, .. } => grid,
        SourceUiWidgetData::List(list) | SourceUiWidgetData::DropList { list, .. } => {
            &mut list.grid
        }
        SourceUiWidgetData::TypeList(list) => &mut list.grid,
        SourceUiWidgetData::TreeElement(tree) => &mut tree.grid,
        _ => return,
    };
    *overlay = merge_authored_grid_template(base, overlay);
}

fn merge_authored_grid_template(base: &SourceUiGrid, overlay: &SourceUiGrid) -> SourceUiGrid {
    SourceUiGrid {
        auto_size: overlay.auto_size.or(base.auto_size),
        auto_size_parent: overlay.auto_size_parent.or(base.auto_size_parent),
        columns: overlay.columns.or(base.columns),
        rows: overlay.rows.or(base.rows),
        x_spacing: overlay.x_spacing.or(base.x_spacing),
        y_spacing: overlay.y_spacing.or(base.y_spacing),
        column_width: overlay.column_width.or(base.column_width),
        row_height: overlay.row_height.or(base.row_height),
        initial_x: overlay.initial_x.or(base.initial_x),
        initial_y: overlay.initial_y.or(base.initial_y),
    }
}

fn merge_authored_list_template(base: &SourceUiList, overlay: &SourceUiList) -> SourceUiList {
    SourceUiList {
        grid: merge_authored_grid_template(&base.grid, &overlay.grid),
        row_template: overlay
            .row_template
            .clone()
            .or_else(|| base.row_template.clone()),
        balance_sheet_layout: overlay
            .balance_sheet_layout
            .clone()
            .or_else(|| base.balance_sheet_layout.clone()),
        count_component: overlay
            .count_component
            .clone()
            .or_else(|| base.count_component.clone()),
        update_seconds: overlay.update_seconds.or(base.update_seconds),
        source: if overlay.source == SourceUiListSource::Unbound {
            base.source
        } else {
            overlay.source
        },
    }
}

fn merge_authored_button_template(
    base: &SourceUiButton,
    overlay: &SourceUiButton,
) -> SourceUiButton {
    SourceUiButton {
        auto_size: overlay.auto_size.or(base.auto_size),
        min_height: overlay.min_height.or(base.min_height),
        toggle: overlay.toggle.or(base.toggle),
        sticky: overlay.sticky.or(base.sticky),
        repress: overlay.repress.or(base.repress),
        activate_data: overlay
            .activate_data
            .clone()
            .or_else(|| base.activate_data.clone()),
        repeat_delay_seconds: overlay.repeat_delay_seconds.or(base.repeat_delay_seconds),
        hold_change: overlay.hold_change.or(base.hold_change),
        hold_interval_cap_seconds: overlay
            .hold_interval_cap_seconds
            .or(base.hold_interval_cap_seconds),
        broadcast: overlay.broadcast.or(base.broadcast),
        delayed_activation_seconds: overlay
            .delayed_activation_seconds
            .or(base.delayed_activation_seconds),
    }
}

fn merge_authored_slider_template(
    base: &SourceUiSlider,
    overlay: &SourceUiSlider,
) -> SourceUiSlider {
    let mut slider = base.clone();
    slider.value_type = overlay.value_type.clone().or(slider.value_type);
    slider.span = overlay.span.or(slider.span);
    slider.min = overlay.min.or(slider.min);
    slider.max = overlay.max.or(slider.max);
    slider.increment = overlay.increment.or(slider.increment);
    slider.initial_value = overlay.initial_value.or(slider.initial_value);
    slider.thumb_name = overlay.thumb_name.clone().or(slider.thumb_name);
    slider.thumb_region = overlay.thumb_region.clone().or(slider.thumb_region);
    slider.axis = overlay.axis.clone().or(slider.axis);
    slider.style = overlay.style.clone().or(slider.style);
    slider.field = overlay.field.clone().or(slider.field);
    slider.on_change = overlay.on_change.clone().or(slider.on_change);
    slider.minimum_thumb_size = overlay.minimum_thumb_size.or(slider.minimum_thumb_size);
    slider
}

#[cfg(test)]
mod slider_template_tests {
    use crate::assets::source_document::path::AssetPath;
    use crate::assets::source_document::ui::model::SourceUiWidgetData;
    use crate::assets::source_document::ui::parser::SourceUiDocument;
    use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
    use crate::assets::ui_document::source::lower::authored_ui_template_resolution::collect_templates;
    use crate::assets::ui_document::source::lower::authored_ui_template_resolution::resolve_template;
    use openzt2_game_data::ui_document::document::UiDocumentRole;
    use openzt2_game_data::AssetId;
    use std::collections::BTreeMap;

    #[test]
    fn catalogue_template_retains_filters_and_explicit_empty_clears_them() {
        use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
        let parsed = parse_blue_fang_source_document(
            AssetPath::new("ui/layout/filter-test.xml"),
            br#"
            <UIRoot><children>
                <ZTUITypeList templateName="buy" rows="2" columnWidth="58">
                    <TypeListFilters><s_ObjectType/><biome/></TypeListFilters>
                </ZTUITypeList>
                <ZTUITypeList template="buy" name="buildings" rows="1"/>
                <ZTUITypeList template="buy" name="unfiltered"><TypeListFilters/></ZTUITypeList>
            </children></UIRoot>"#,
        )
        .expect("valid UI source");
        let source = SourceUiDocument::parse(&parsed);
        let input = AuthoredUiDocument {
            virtual_path: "ui/layout/filter-test.xml".into(),
            id: AssetId::default(),
            role: UiDocumentRole::Fragment,
            source,
            templates: BTreeMap::new(),
            image_selections: Vec::new(),
            interaction_cursors: None,
            placement_preview: None,
            role_targets: BTreeMap::new(),
            resolved_dependencies: Default::default(),
        };
        let templates = collect_templates(&input.source.root).expect("template registry");
        let inherited = resolve_template(
            &input.source.root.children[1],
            &templates,
            &mut Vec::new(),
            &input,
        )
        .expect("resolved purchase list");
        let SourceUiWidgetData::TypeList(inherited) = inherited.widget else {
            panic!("purchase list");
        };
        assert_eq!(
            inherited.filter_fields,
            Some(vec!["s_ObjectType".into(), "biome".into()])
        );
        assert_eq!(inherited.grid.rows, Some(1));
        assert_eq!(inherited.grid.column_width, Some(58));
        let cleared = resolve_template(
            &input.source.root.children[2],
            &templates,
            &mut Vec::new(),
            &input,
        )
        .expect("resolved empty override");
        let SourceUiWidgetData::TypeList(cleared) = cleared.widget else {
            panic!("purchase list");
        };
        assert_eq!(cleared.filter_fields, Some(Vec::new()));
    }
}

/// Blue Fang authored scalable panels as a convenience template containing
/// nine static children. The owned UI contract stores the same object in the
/// shape Bevy consumes: one image, one source rectangle and four slice insets.
/// Keeping the nine source children would make layout rounding and scheduling
/// observable and needlessly multiply ECS entities.
pub(super) fn collapse_nine_slice(
    node: &mut SourceUiNode,
    input: &AuthoredUiDocument,
) -> io::Result<Option<[f32; 4]>> {
    const PARTS: [&str; 9] = [
        "TopLeft",
        "TopMiddle",
        "TopRight",
        "MidLeft",
        "Center",
        "MidRight",
        "BottomLeft",
        "BottomMiddle",
        "BottomRight",
    ];
    if node.children.len() < PARTS.len() {
        return Ok(None);
    }
    let by_name = node
        .children
        .iter()
        .filter(|child| {
            child
                .name
                .as_deref()
                .is_some_and(|name| PARTS.contains(&name))
        })
        .filter_map(|child| child.name.as_deref().map(|name| (name, child)))
        .collect::<BTreeMap<_, _>>();
    if by_name.len() != PARTS.len() || PARTS.iter().any(|name| !by_name.contains_key(name)) {
        return Ok(None);
    }

    let part_visual = |name: &str| -> io::Result<&SourceUiVisual> {
        let child = by_name[name];
        if !child.children.is_empty() || !child.events.is_empty() || !child.hotkeys.is_empty() {
            return Err(invalid_at(
                input,
                format!("nine-slice part {name:?} contains behavior or descendants"),
            ));
        }
        let visual = child
            .aspect
            .as_ref()
            .and_then(|aspect| aspect.default.as_ref())
            .ok_or_else(|| invalid_at(input, format!("nine-slice part {name:?} has no image")))?;
        Ok(visual)
    };

    let top_left_visual = part_visual("TopLeft")?;
    let expected_image = top_left_visual.image.as_ref().map(AssetPath::key);
    for name in PARTS.into_iter().skip(1) {
        let visual = part_visual(name)?;
        if visual.image.as_ref().map(AssetPath::key) != expected_image {
            return Err(invalid_at(
                input,
                format!("nine-slice part {name:?} uses a different image"),
            ));
        }
    }
    let image_size = expected_image
        .as_deref()
        .and_then(|path| input.resolved_dependencies.image_dimensions.get(path))
        .copied();
    let part = |name: &str| -> io::Result<[f32; 4]> {
        let visual = part_visual(name)?;
        let rect = visual.rect.as_ref().ok_or_else(|| {
            invalid_at(
                input,
                format!("nine-slice part {name:?} has no source rectangle"),
            )
        })?;
        let coordinate = |metric: &SourceUiMetric, field: &str| {
            number(metric).ok_or_else(|| {
                invalid_at(
                    input,
                    format!("nine-slice part {name:?} has non-numeric {field}"),
                )
            })
        };
        let x = coordinate(&rect.x, "x")?;
        let y = coordinate(&rect.y, "y")?;
        let remaining = |metric: &SourceUiMetric, start: f32, axis: usize| -> io::Result<f32> {
            match metric {
                SourceUiMetric::Number(value) => Ok(*value),
                SourceUiMetric::Token(token)
                    if token.eq_ignore_ascii_case("LAST_W") && axis == 0 =>
                {
                    Ok(image_size.ok_or_else(|| {
                        invalid_at(input, "nine-slice LAST_W has no image dimensions")
                    })?[0] as f32
                        - start)
                }
                SourceUiMetric::Token(token)
                    if token.eq_ignore_ascii_case("LAST_H") && axis == 1 =>
                {
                    Ok(image_size.ok_or_else(|| {
                        invalid_at(input, "nine-slice LAST_H has no image dimensions")
                    })?[1] as f32
                        - start)
                }
                _ => Err(invalid_at(
                    input,
                    format!("nine-slice part {name:?} has unsupported size metric"),
                )),
            }
        };
        Ok([
            x,
            y,
            remaining(&rect.width, x, 0)?,
            remaining(&rect.height, y, 1)?,
        ])
    };
    let top_left = part("TopLeft")?;
    let top_middle = part("TopMiddle")?;
    let top_right = part("TopRight")?;
    let mid_left = part("MidLeft")?;
    let center = part("Center")?;
    let mid_right = part("MidRight")?;
    let bottom_left = part("BottomLeft")?;
    let bottom_middle = part("BottomMiddle")?;
    let bottom_right = part("BottomRight")?;

    let [x, y, left, top] = top_left;
    let right = top_right[2];
    let bottom = bottom_left[3];
    let middle_width = top_middle[2];
    let middle_height = mid_left[3];
    let close = |left: f32, right: f32| (left - right).abs() <= 0.01;
    let expected = [
        (top_middle, [x + left, y, middle_width, top]),
        (top_right, [x + left + middle_width, y, right, top]),
        (mid_left, [x, y + top, left, middle_height]),
        (center, [x + left, y + top, middle_width, middle_height]),
        (
            mid_right,
            [x + left + middle_width, y + top, right, middle_height],
        ),
        (bottom_left, [x, y + top + middle_height, left, bottom]),
        (
            bottom_middle,
            [x + left, y + top + middle_height, middle_width, bottom],
        ),
        (
            bottom_right,
            [
                x + left + middle_width,
                y + top + middle_height,
                right,
                bottom,
            ],
        ),
    ];
    if expected
        .iter()
        .any(|(actual, expected)| actual.iter().zip(expected).any(|(a, e)| !close(*a, *e)))
    {
        return Err(invalid_at(
            input,
            "nine-slice source rectangles do not form one contiguous image region",
        ));
    }

    let mut visual = top_left_visual.clone();
    visual.rect = Some(SourceUiMetricRect {
        x: SourceUiMetric::Number(x),
        y: SourceUiMetric::Number(y),
        width: SourceUiMetric::Number(left + middle_width + right),
        height: SourceUiMetric::Number(top + middle_height + bottom),
    });
    let mut aspect = by_name["TopLeft"].aspect.clone().expect("validated above");
    aspect.default = Some(visual);
    aspect.standard.clear();
    aspect.alternate.clear();
    // Preserve typography without replacing the reconstructed whole-skin
    // rectangle with the owner's former atlas subrectangle.
    if let Some(owner) = node.aspect.as_ref() {
        aspect.localization_id = owner.localization_id.clone();
        aspect.authored_string = owner.authored_string.clone();
        if let (Some(skin), Some(owner)) = (aspect.default.as_mut(), owner.default.as_ref()) {
            skin.font = owner.font.clone();
            skin.text_alignment = owner.text_alignment.clone();
            skin.text_format = owner.text_format.clone();
        }
    }
    node.aspect = Some(aspect);
    node.children.retain(|child| {
        child
            .name
            .as_deref()
            .is_none_or(|name| !PARTS.contains(&name))
    });
    Ok(Some([left, right, top, bottom]))
}

/// Text-edit templates put their scalable skin in an `edit_back` child. Bevy
/// paints child UI nodes after the parent's editable glyphs, so retain the
/// authored skin on the text-edit entity itself where image content precedes
/// text content in Bevy's per-node render order.
pub(super) fn collapse_text_edit_background_template_into_editable_node(
    node: &mut SourceUiNode,
    templates: &BTreeMap<String, SourceUiNode>,
    template_stack: &mut Vec<String>,
    input: &AuthoredUiDocument,
) -> io::Result<Option<([f32; 4], [i32; 4])>> {
    if !matches!(node.kind, SourceUiWidgetKind::TextEdit) {
        return Ok(None);
    }
    if node
        .aspect
        .as_ref()
        .is_some_and(|aspect| !aspect.standard.is_empty() || !aspect.alternate.is_empty())
    {
        return Ok(None);
    }
    let Some(background_index) = node.children.iter().position(|child| {
        child
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("edit_back"))
    }) else {
        return Ok(None);
    };
    let mut background = resolve_template(
        &node.children[background_index],
        templates,
        template_stack,
        input,
    )?;
    let Some(slice_insets) = collapse_nine_slice(&mut background, input)? else {
        return Ok(None);
    };
    if !matches!(background.kind, SourceUiWidgetKind::Layout)
        || !background.children.is_empty()
        || !background.events.is_empty()
        || !background.hotkeys.is_empty()
        || !background.fields.is_empty()
        || !background.finance_categories.is_empty()
    {
        return Err(invalid_at(
            input,
            "text-edit background contains behavior or nonvisual content",
        ));
    }
    validate_represented_facts(&background, input)?;
    let background_aspect = background
        .aspect
        .as_ref()
        .ok_or_else(|| invalid_at(input, "text-edit background has no visual aspect"))?;
    let background_visual = background_aspect
        .default
        .as_ref()
        .ok_or_else(|| invalid_at(input, "text-edit background has no default visual"))?;
    let background_source = background_visual
        .rect
        .as_ref()
        .ok_or_else(|| invalid_at(input, "text-edit background has no source rectangle"))?;
    let exact_i32 = |metric: &SourceUiMetric, field: &str| {
        let value = number(metric).ok_or_else(|| {
            invalid_at(
                input,
                format!("text-edit background has non-numeric {field}"),
            )
        })?;
        if value.fract() != 0.0 || value < i32::MIN as f32 || value > i32::MAX as f32 {
            return Err(invalid_at(
                input,
                format!("text-edit background has non-integral {field}"),
            ));
        }
        Ok(value as i32)
    };
    let background_source = [
        exact_i32(&background_source.x, "source x")?,
        exact_i32(&background_source.y, "source y")?,
        exact_i32(&background_source.width, "source width")?,
        exact_i32(&background_source.height, "source height")?,
    ];
    node.aspect = Some(match node.aspect.as_ref() {
        Some(editable_aspect) => visuals::merge_source_ui_aspect_template_and_authored_override(
            Some(background_aspect),
            editable_aspect,
        ),
        None => background_aspect.clone(),
    });
    node.children.remove(background_index);
    Ok(Some((slice_insets, background_source)))
}

pub(super) fn logical_size(root: &SourceUiNode, role: UiDocumentRole) -> io::Result<[f32; 2]> {
    let Some(region) = root.region.as_ref() else {
        return (role == UiDocumentRole::Fragment)
            .then(|| fragment_content_size(root))
            .ok_or_else(|| invalid("UI role root has no UIRegion"));
    };
    let width = number(&region.width).unwrap_or(1.0);
    let height = number(&region.height).unwrap_or(1.0);
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return (role == UiDocumentRole::Fragment)
            .then(|| fragment_content_size(root))
            .ok_or_else(|| invalid("UI role root size is not finite and positive"));
    }
    Ok([width, height])
}

/// Measures an authored reusable fragment whose root intentionally relies on
/// its children for one or both dimensions.
///
/// A positive child box is the layout boundary for its descendants. Controls
/// may deliberately overflow that box to enlarge their hit area, but that
/// overflow does not advance the parent's list/grid. The source UITreeElement
/// row, for example, has `h=0`, a 12-pixel heading, and a 30-pixel action hit
/// target inside the heading; its intrinsic row height is 12, not 30.
pub(super) fn fragment_content_size(root: &SourceUiNode) -> [f32; 2] {
    fn extent(
        node: &SourceUiNode,
        origin: [f32; 2],
        unresolved: [bool; 2],
        maximum: &mut [f32; 2],
    ) {
        let rect = numeric_rect(node.region.as_ref());
        let position = [origin[0] + rect[0], origin[1] + rect[1]];
        let descendant_unresolved = std::array::from_fn(|axis| {
            if unresolved[axis] && rect[axis + 2] > 0.0 {
                maximum[axis] = maximum[axis].max(position[axis] + rect[axis + 2]);
                false
            } else {
                unresolved[axis]
            }
        });
        if descendant_unresolved.iter().any(|unresolved| *unresolved) {
            node.children.iter().for_each(|child| {
                extent(child, position, descendant_unresolved, maximum);
            });
        }
    }

    let mut maximum = [0.0_f32; 2];
    // The fragment root's x/y belongs to its original parent. Its descendants
    // are measured in the root-local coordinate system used by the row owner.
    let root_rect = numeric_rect(root.region.as_ref());
    maximum[0] = root_rect[2];
    maximum[1] = root_rect[3];
    let unresolved = [root_rect[2] <= 0.0, root_rect[3] <= 0.0];
    root.children.iter().for_each(|child| {
        extent(child, [0.0, 0.0], unresolved, &mut maximum);
    });
    [maximum[0].max(1.0), maximum[1].max(1.0)]
}

pub(super) fn validate_represented_facts(
    node: &SourceUiNode,
    input: &AuthoredUiDocument,
) -> io::Result<()> {
    let unsupported = |fact: &str| {
        Err(invalid_at(
            input,
            format!(
                "typed UI schema does not yet represent {fact} on {:?}",
                node.kind
            ),
        ))
    };
    if matches!(node.kind, SourceUiWidgetKind::Unknown(_)) {
        return unsupported("unknown source widget");
    }
    if node
        .fields
        .iter()
        .any(|field| !field.unknown_attributes.is_empty())
    {
        return unsupported("unknown typed-field attributes");
    }
    if node
        .finance_categories
        .iter()
        .any(|category| !category.unknown_attributes.is_empty())
    {
        return unsupported("unknown finance-category attributes");
    }
    if !node.unknown_attributes.is_empty() || !node.unknown_elements.is_empty() {
        return unsupported("unknown source payload");
    }
    if node.x_pack.is_some() {
        return unsupported("unresolved xPack availability guard");
    }
    if matches!(node.state.active, SourceUiActiveState::Unknown(_)) {
        return unsupported("unknown active state");
    }
    if let Some(region) = &node.region {
        if !region.unknown_attributes.is_empty() {
            return unsupported("unknown UIRegion attributes");
        }
    }
    if let Some(aspect) = &node.aspect {
        if !aspect.unknown_attributes.is_empty() || !aspect.unknown_elements.is_empty() {
            return unsupported("unknown UIAspect payload");
        }
        if matches!(aspect.hit_policy, SourceUiHitPolicy::Unknown(_)) {
            return unsupported("unknown UIAspect hit policy");
        }
        for visual in aspect
            .default
            .iter()
            .chain(aspect.standard.iter().map(|v| &v.visual))
            .chain(aspect.alternate.iter().map(|v| &v.visual))
        {
            if !visual.unknown_attributes.is_empty() || !visual.unknown_elements.is_empty() {
                return unsupported("unknown visual-state payload");
            }
            if visual
                .font
                .as_ref()
                .is_some_and(|font| !font.unknown_attributes.is_empty())
            {
                return unsupported("unknown font metadata");
            }
        }
    }
    match &node.widget {
        SourceUiWidgetData::DropList {
            layout,
            horizontal_layout,
            list,
            ..
        } if layout.is_some()
            || horizontal_layout.is_some()
            || list.balance_sheet_layout.is_some() =>
        {
            return unsupported("drop-list layout references");
        }
        SourceUiWidgetData::List(value) if value.balance_sheet_layout.is_some() => {
            return unsupported("balance-sheet layout reference");
        }
        SourceUiWidgetData::Text(value)
            if value
                .text_type
                .as_deref()
                .is_some_and(|value| value != "multi") =>
        {
            return unsupported("text presentation type");
        }
        SourceUiWidgetData::TextEdit { text, .. }
            if text
                .text_type
                .as_deref()
                .is_some_and(|value| value != "multi") =>
        {
            return unsupported("text presentation type");
        }
        SourceUiWidgetData::Tooltip(value)
            if value
                .text
                .text_type
                .as_deref()
                .is_some_and(|value| value != "multi") =>
        {
            return unsupported("tooltip text presentation type");
        }
        SourceUiWidgetData::Tooltip(value) if value.target.is_some() => {
            return unsupported("unevidenced tooltip target attribute");
        }
        SourceUiWidgetData::Tooltip(value)
            if value
                .tooltip_type
                .as_deref()
                .is_some_and(|value| !matches!(value, "name" | "short" | "long" | "help")) =>
        {
            return unsupported("unknown tooltip presentation type");
        }
        _ => {}
    }
    for event in node.events.iter().flat_map(|block| &block.events) {
        validate_event(event, input)?;
    }
    for hotkey in &node.hotkeys {
        if hotkey.localization_id.is_some() || hotkey.file.is_some() || hotkey.node.is_some() {
            return unsupported("hotkey localization/file/node routing");
        }
        validate_event(&hotkey.event, input)?;
    }
    Ok(())
}

pub(super) fn validate_event(event: &SourceUiEvent, input: &AuthoredUiDocument) -> io::Result<()> {
    validate_event_payload(event, input)?;
    if let Some(child) = &event.child {
        validate_event(child, input)?;
    }
    Ok(())
}

pub(super) fn validate_event_payload(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<()> {
    let geometry_payload = matches!(
        event.message.as_str(),
        "UI_SCROLL"
            | "UI_SET_SCROLL"
            | "UI_SET_SRC_RECT"
            | "UI_SET_POS"
            | "UI_SET_SIZE"
            | "ZT_TERRAINCURSOR_SIZE"
            | "ZT_SET_ADMISSION_PRICE_INDEX"
            | "ZT_ZOO_OPEN"
            | "ZT_PHOTOEVENT_ALBUM_SELECT_PICTURE"
            | "ZT_PHOTOEVENT_ALBUM_DESELECT_PICTURE"
            | "ZT_PHOTOEVENT_ENLARGE_ALBUM_PIC"
            | "ZT_PHOTOEVENT_CAMERA_SELECT_PICTURE"
            | "ZT_PHOTOEVENT_CAMERA_DESELECT_PICTURE"
            | "ZT_PHOTOEVENT_SELECT_ALBUM"
            | "ZT_PHOTOEVENT_MOVE_ENTER"
            | "ZT_PHOTOEVENT_MOVE_EXIT"
            | "ZT_PHOTOEVENT_MOVE_SELECTED_TARGET"
    );
    let typed_xml_object = event.message == "ZT_APPMSG"
        && event.xml_object.as_ref().is_some_and(|object| {
            object.message_type.as_deref() == Some("BFS_SETGLOBALVAR")
                && object.key.as_deref().is_some()
                && object.value.as_deref().is_some()
        });
    let typed_event_type = event.message == "UI_SEND_TEXT" && event.event_type.is_some();
    let typed_photo_attribute = event.message == "ZT_SET_USER_ATTRIBUTE"
        && event.key.as_deref() == Some("photohelpshown")
        && event.val.as_deref().and_then(parse_bool) == Some(true);
    let typed_new_album = event.message == "ZT_PHOTOEVENT_NEW_ALBUM"
        && event.payload.len() == 1
        && event.payload[0]
            .name
            .eq_ignore_ascii_case("new_albumstring");
    let typed_biome_auto_placement_list = event.message == "ZT_SET_AUTO_PLACEMENT_LIST"
        && event.payload.len() == 1
        && event.payload[0].name.eq_ignore_ascii_case("autoPlacement");
    if (event.xml_object.is_some() && !typed_xml_object)
        || (!event.payload.is_empty() && !typed_new_album && !typed_biome_auto_placement_list)
        || !event.unknown_attributes.is_empty()
        || event.color.is_some()
        || (!geometry_payload && event.rect.iter().any(Option::is_some))
        || ((event.key.is_some() || event.val.is_some()) && !typed_photo_attribute)
        || (event.event_type.is_some() && !typed_event_type)
    {
        return Err(invalid_at(
            input,
            format!(
                "typed UI command schema does not represent payload carried by {:?}",
                event.message
            ),
        ));
    }
    Ok(())
}

pub(super) fn lower_region(
    region: Option<&SourceUiRegion>,
    input: &AuthoredUiDocument,
) -> io::Result<UiNodeRegionDefinition> {
    let Some(region) = region else {
        return Ok(UiNodeRegionDefinition::default());
    };
    Ok(UiNodeRegionDefinition {
        metrics: [
            metric(&region.x, input)?,
            metric(&region.y, input)?,
            metric(&region.width, input)?,
            metric(&region.height, input)?,
        ],
        alignment: [
            alignment(&region.x_align, input)?,
            alignment(&region.y_align, input)?,
            alignment(&region.width_align, input)?,
            alignment(&region.height_align, input)?,
        ],
    })
}

pub(super) fn metric(
    value: &SourceUiMetric,
    input: &AuthoredUiDocument,
) -> io::Result<UiNodeRegionMetric> {
    match value {
        SourceUiMetric::Number(value) if value.is_finite() => {
            Ok(UiNodeRegionMetric::Pixels(*value))
        }
        SourceUiMetric::Token(value) => match value.as_str() {
            "OUT_TOP" => Ok(UiNodeRegionMetric::OutsideTop),
            "LAST_H" => Ok(UiNodeRegionMetric::Pixels(82.0)),
            "-LAST_H" => Ok(UiNodeRegionMetric::Pixels(-82.0)),
            // OUT_TOP is the only special metric present in the resolved
            // original UI documents. It denotes the zero-valued top edge in the
            // tooltip strip layouts. The other spellings are parser
            // vocabulary without supported layout semantics, so accepting
            // them would make live lowering certify a layout the game cannot
            // reproduce.
            "OUT_BOTTOM" | "OUT_LEFT" | "OUT_RIGHT" => Err(invalid_at(
                input,
                format!("unsupported UIRegion metric token {value:?}"),
            )),
            _ => Err(invalid_at(
                input,
                format!("unknown UIRegion metric token {value:?}"),
            )),
        },
        _ => Err(invalid_at(input, "non-finite UIRegion metric")),
    }
}

pub(super) fn alignment(
    value: &SourceUiAlignment,
    input: &AuthoredUiDocument,
) -> io::Result<UiNodeRegionAlignment> {
    match value {
        SourceUiAlignment::Min => Ok(UiNodeRegionAlignment::Minimum),
        SourceUiAlignment::Max => Ok(UiNodeRegionAlignment::Maximum),
        SourceUiAlignment::Length => Ok(UiNodeRegionAlignment::Length),
        SourceUiAlignment::Mid => Ok(UiNodeRegionAlignment::Middle),
        SourceUiAlignment::PercentMin => Ok(UiNodeRegionAlignment::PercentageFromMinimum),
        SourceUiAlignment::PercentMax => Ok(UiNodeRegionAlignment::PercentageFromMaximum),
        SourceUiAlignment::Unknown(value) => Err(invalid_at(
            input,
            format!("unknown UIRegion alignment {value:?}"),
        )),
    }
}

pub(super) fn text_alignment(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<UiTextAlignment> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("left") => Ok(UiTextAlignment::Left),
        Some("center") => Ok(UiTextAlignment::Center),
        Some("right") => Ok(UiTextAlignment::Right),
        Some("justified") => Ok(UiTextAlignment::Justified),
        Some(value) => Err(invalid_at(
            input,
            format!("unsupported text alignment {value:?}"),
        )),
    }
}

pub(super) fn system_font(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<UiSystemFont> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(UiSystemFont::Arial),
        Some(value) if value.eq_ignore_ascii_case("arial") => Ok(UiSystemFont::Arial),
        Some(value) if value.eq_ignore_ascii_case("comic sans ms") => Ok(UiSystemFont::ComicSansMs),
        Some(value) => Err(invalid_at(
            input,
            format!("unsupported system font family {value:?}"),
        )),
    }
}
