//! Recursive authored UI widget-tree parsing and source-coverage collection.

use crate::assets::source_document::{
    ordered_source_document_types::{
        OrderedSourceDocumentAttribute, OrderedSourceDocumentChildren, OrderedSourceDocumentNode,
        OrderedSourceDocumentSpan,
    },
    path::AssetPath,
};

use super::{
    super::model::{
        SourceUiBackgroundLayout, SourceUiHitPolicy, SourceUiNode, SourceUiPayloadNode,
        SourceUiWidgetKind,
    },
    focused_widget_property_parsing::{
        parse_authored_finance_category as parse_finance_category,
        parse_authored_show_hide_animation as parse_show_hide,
        parse_authored_ui_field as parse_field, parse_authored_ui_help as parse_help,
    },
    source_scalar_and_attribute_reading::{
        copy_source_ui_attribute as source_attribute, is_known_widget_attribute,
        parse_source_bool as parse_bool, parse_source_i32_attribute as parse_i32_attr,
        source_attribute_value as attr,
    },
    ui_aspect_and_visual_source_parsing::{
        create_text_only_ui_aspect as text_aspect, parse_authored_ui_aspect as parse_aspect,
        parse_direct_default_ui_aspect as direct_default_aspect,
    },
    ui_event_and_hotkey_parsing::{
        parse_authored_ui_event_block as parse_event_block,
        parse_authored_ui_event_trigger as parse_event_trigger,
        parse_authored_ui_hotkeys as parse_hotkeys,
    },
    ui_region_and_state_parsing::{
        parse_authored_ui_region as parse_region, parse_authored_ui_state as parse_state,
    },
    ui_widget_kind_dispatch::parse_authored_ui_widget_data,
    SourceUiDiagnostic,
};

pub(super) fn parse_authored_ui_root(
    root: &OrderedSourceDocumentNode,
    diagnostics: &mut Vec<SourceUiDiagnostic>,
) -> SourceUiNode {
    parse_authored_ui_node_parts(
        &root.name,
        &root.attributes,
        &root.children,
        root.first_text(),
        root.span,
        diagnostics,
    )
}

pub(super) fn parse_authored_ui_node(
    node: &OrderedSourceDocumentNode,
    diagnostics: &mut Vec<SourceUiDiagnostic>,
) -> SourceUiNode {
    parse_authored_ui_node_parts(
        &node.name,
        &node.attributes,
        &node.children,
        node.first_text(),
        node.span,
        diagnostics,
    )
}

fn parse_authored_ui_node_parts(
    tag: &str,
    attributes: &[OrderedSourceDocumentAttribute],
    children: &OrderedSourceDocumentChildren,
    text: Option<&str>,
    span: OrderedSourceDocumentSpan,
    diagnostics: &mut Vec<SourceUiDiagnostic>,
) -> SourceUiNode {
    let kind = SourceUiWidgetKind::from_tag(tag);
    if matches!(kind, SourceUiWidgetKind::Unknown(_)) {
        diagnostics.push(SourceUiDiagnostic::UnknownWidget {
            tag: tag.to_owned(),
            span,
        });
    }
    let element_children: Vec<_> = children.iter().collect();
    let child = |name: &str| {
        element_children
            .iter()
            .copied()
            .find(|node| node.name == name)
    };
    let aspect = child("UIAspect")
        .map(parse_aspect)
        .or_else(|| child("default").map(direct_default_aspect))
        .or_else(|| text.map(text_aspect));
    let region = child("UIRegion").map(parse_region);
    let state = child("UIState").map(parse_state).unwrap_or_default();
    let fields = element_children
        .iter()
        .copied()
        .filter(|node| node.name == "field")
        .map(parse_field)
        .collect();
    let finance_categories = child("econcategories")
        .into_iter()
        .flat_map(|node| node.element_children())
        .filter(|node| node.name == "category")
        .filter_map(parse_finance_category)
        .collect();
    let hotkeys = element_children
        .iter()
        .copied()
        .filter(|node| node.name == "UIHotKeys")
        .flat_map(parse_hotkeys)
        .collect();
    let events = element_children
        .iter()
        .copied()
        .filter_map(parse_event_block)
        .collect();
    let mut authored_children: Vec<_> = element_children
        .iter()
        .copied()
        // Cursor and highlight groups belong to UITextEdit's editing policy.
        // They are not persistent layout children: the native consumer uses
        // Bevy EditableText for caret and selection geometry.
        .filter(|node| matches!(node.name.as_str(), "children" | "decorations" | "drag"))
        .flat_map(|group| group.children.iter())
        .chain(
            element_children
                .iter()
                .copied()
                .filter(|node| SourceUiWidgetKind::is_known_tag(&node.name)),
        )
        .map(|node| parse_authored_ui_node(node, diagnostics))
        .collect();
    authored_children.extend(
        element_children
            .iter()
            .copied()
            .filter(|node| {
                matches!(
                    node.name.as_str(),
                    "overview" | "group" | "neutral" | "success" | "failure"
                )
            })
            .filter_map(|wrapper| {
                wrapper
                    .element_children()
                    .find(|node| SourceUiWidgetKind::is_known_tag(&node.name))
                    .map(|node| {
                        let mut parsed = parse_authored_ui_node(node, diagnostics);
                        parsed.name.get_or_insert_with(|| wrapper.name.to_string());
                        parsed
                    })
            }),
    );
    let unknown_attributes = attributes
        .iter()
        .filter(|attribute| !is_known_widget_attribute(attribute.name()))
        .map(source_attribute)
        .collect::<Vec<_>>();
    diagnostics.extend(unknown_attributes.iter().cloned().map(|attribute| {
        SourceUiDiagnostic::UnknownAttribute {
            widget: tag.to_owned(),
            attribute,
            span,
        }
    }));
    let unknown_elements = element_children
        .iter()
        .copied()
        .filter(|node| !is_known_widget_element(&node.name))
        .filter(|node| !SourceUiWidgetKind::is_known_tag(&node.name))
        .filter(|node| parse_event_trigger(&node.name).is_none())
        .map(SourceUiPayloadNode::from_data_node)
        .collect::<Vec<_>>();
    diagnostics.extend(
        unknown_elements
            .iter()
            .map(|element| SourceUiDiagnostic::UnknownElement {
                widget: tag.to_owned(),
                element: element.name.clone(),
                span: element.span,
            }),
    );

    let clip_children = matches!(
        kind,
        SourceUiWidgetKind::ListBox
            | SourceUiWidgetKind::MultiList
            | SourceUiWidgetKind::DropList
            | SourceUiWidgetKind::ContextList
            | SourceUiWidgetKind::TypeList
    );
    SourceUiNode {
        widget: parse_authored_ui_widget_data(
            &kind,
            attributes,
            &element_children,
            aspect.as_ref(),
            diagnostics,
        ),
        kind,
        name: attr(attributes, "name").map(str::to_owned),
        template: attr(attributes, "template")
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        template_name: attr(attributes, "templateName")
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        scenario_objective_visual: attr(attributes, "openzt2ScenarioObjectiveVisual")
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        cursor: attr(attributes, "cursor")
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        modal: attr(attributes, "modal").and_then(parse_bool),
        always_hit: attr(attributes, "alwayshit").map(SourceUiHitPolicy::parse),
        cacheable_from_lua: attr(attributes, "canBeCachedWhenCalledFromLua").and_then(parse_bool),
        use_list_box_color_if_in_container: attr(attributes, "useListBoxColorIfInContainer")
            .and_then(parse_bool),
        clip_children,
        x_pack: attr(attributes, "xPack")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .and_then(|value| u32::try_from(value).ok()),
        background_layout: attr(attributes, "bgimg").map(|image| SourceUiBackgroundLayout {
            image: AssetPath::new(image),
            source: [
                parse_i32_attr(attributes, "sx").unwrap_or(0),
                parse_i32_attr(attributes, "sy").unwrap_or(0),
                parse_i32_attr(attributes, "sw").unwrap_or(0),
                parse_i32_attr(attributes, "sh").unwrap_or(0),
            ],
            padding: [
                parse_i32_attr(attributes, "padx").unwrap_or(0),
                parse_i32_attr(attributes, "pady").unwrap_or(0),
            ],
            width: parse_i32_attr(attributes, "width"),
        }),
        region,
        state,
        aspect,
        show_hide_animation: child("UIShowHideAnim").map(parse_show_hide),
        notify_animation_completed: attr(attributes, "notifyTargetsOfCompletedAnimation")
            .and_then(parse_bool),
        help: child("UIHelpInfo").map(parse_help),
        text_format: child("text")
            .and_then(|node| node.attribute("format"))
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        fields,
        finance_categories,
        hotkeys,
        events,
        children: authored_children,
        unknown_attributes,
        unknown_elements,
        span,
    }
}

fn is_known_widget_element(name: &str) -> bool {
    const NAMES: &str = "UIRegion UIState UIAspect default UIShowHideAnim UIHelpInfo UIHotKeys field children decorations drag text econcategories thumb images timedEvents start end colors layers secondaryGlobe globe clouds dot selected dotselected highlighted pointer dotHighlightColor kinds kindsExclude nonRememberedKinds TypeListFilters hscroll vscroll cursor highlight overview group neutral success failure replacement namedEventLists";
    NAMES
        .split_ascii_whitespace()
        .any(|candidate| candidate == name)
}
