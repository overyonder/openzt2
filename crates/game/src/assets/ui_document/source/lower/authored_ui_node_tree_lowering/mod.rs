use crate::assets::source_document::ui::model::{
    SourceUiNode, SourceUiWidgetData, SourceUiWidgetKind,
};
use crate::assets::ui_document::source::lower::authored_ui_animation_lowering::{
    lower_animation, lower_widget_animation,
};
use crate::assets::ui_document::source::lower::authored_ui_asset_dependency_resolution::cursor_texture_dependency;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_event_collection_lowering::{
    lower_events, objective_status_filter,
};
use crate::assets::ui_document::source::lower::authored_ui_hotkey_lowering::add_lowered_hotkey_definitions_to_current_ui_node;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, numeric_rect,
};
use crate::assets::ui_document::source::lower::authored_ui_template_resolution::{
    collapse_nine_slice, collapse_text_edit_background_template_into_editable_node, lower_region,
    resolve_template, validate_represented_facts,
};
use crate::assets::ui_document::source::lower::authored_ui_widget_record_lowering::{
    authored_text, node_flags, node_kind,
};
use crate::assets::ui_document::source::lower::{
    authored_ui_node_property_binding_resolution, authored_ui_owned_descendant_node_resolution,
    authored_ui_widget_vocabulary_resolution, canonical_source_value_resolution, visuals, widgets,
};
use openzt2_game_data::ui_document::action::animal_shows::{
    UiShowAction, UiShowActionRecord, UiShowPlatformUpgrade,
};
use openzt2_game_data::ui_document::action::audio_settings::{
    UiAudioSettingAction, UiAudioSettingActionRecord,
};
use openzt2_game_data::ui_document::action::information::{
    UiInformationAction, UiInformationActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::finance_table::UiFinanceTableCategoryDefinition;
use openzt2_game_data::ui_document::hotkey::UiDocumentHotkeyDefinition;
use openzt2_game_data::ui_document::node::{
    UiNodeDefinition, UiNodeFieldDefinition, UiNodeFlags, UiNodeKind,
};
use openzt2_game_data::ui_document::node_presentation::UiNodeVisualStateDefinition;
use openzt2_game_data::ui_document::node_property_binding::UiNodePropertyBinding;
use openzt2_game_data::ui_document::widget::UiWidgetRecord;
use openzt2_game_data::AssetId;
use std::collections::{BTreeMap, BTreeSet};
use std::io;

#[derive(Default)]
pub(super) struct BuildOutput {
    pub(super) nodes: Vec<UiNodeDefinition>,
    pub(super) bindings: Vec<UiNodePropertyBinding>,
    pub(super) actions: Vec<UiActionRecord>,
    pub(super) visuals: Vec<UiNodeVisualStateDefinition>,
    pub(super) hotkeys: Vec<UiDocumentHotkeyDefinition>,
    pub(super) field_definitions: Vec<UiNodeFieldDefinition>,
    pub(super) finance_categories: Vec<UiFinanceTableCategoryDefinition>,
    pub(super) dependencies: BTreeSet<[u8; 16]>,
    pub(super) interactive_textures: BTreeSet<[u8; 16]>,
    pub(super) explicit_scenes: BTreeMap<[u8; 16], String>,
    pub(super) node_ids: BTreeSet<[u8; 16]>,
    pub(super) simulation_paused_visible_nodes: BTreeSet<String>,
}

pub(super) fn nearest_tree_element(output: &BuildOutput, mut parent: u32) -> Option<AssetId> {
    while parent != u32::MAX {
        let node = output.nodes.get(parent as usize)?;
        if matches!(node.kind, UiNodeKind::TreeElement) {
            return Some(node.id);
        }
        parent = node.parent;
    }
    None
}

pub(super) fn nearest_scroll_receiver(output: &BuildOutput, mut parent: u32) -> Option<AssetId> {
    while parent != u32::MAX {
        let node = output.nodes.get(parent as usize)?;
        if matches!(
            &node.widget,
            UiWidgetRecord::Slider(_)
                | UiWidgetRecord::List { .. }
                | UiWidgetRecord::TypeList { .. }
                | UiWidgetRecord::AdoptionList { .. }
                | UiWidgetRecord::FinanceList { .. }
                | UiWidgetRecord::Window { .. }
        ) {
            return Some(node.id);
        }
        parent = node.parent;
    }
    None
}

pub(super) fn lower_node(
    authored: &SourceUiNode,
    parent: u32,
    inherited_confirmation_scope: Option<&str>,
    input: &AuthoredUiDocument,
    templates: &BTreeMap<String, SourceUiNode>,
    template_stack: &mut Vec<String>,
    output: &mut BuildOutput,
) -> io::Result<u32> {
    let mut effective = resolve_template(authored, templates, template_stack, input)?;
    let text_edit_background = collapse_text_edit_background_template_into_editable_node(
        &mut effective,
        templates,
        template_stack,
        input,
    )?;
    let nine_slice = match text_edit_background {
        Some((slice_insets, _)) => Some(slice_insets),
        None => collapse_nine_slice(&mut effective, input)?,
    };
    let confirmation_scope = effective
        .modal
        .is_some_and(|modal| modal)
        .then_some(effective.name.as_deref())
        .flatten()
        .or(inherited_confirmation_scope);
    validate_represented_facts(&effective, input)?;
    let index =
        u32::try_from(output.nodes.len()).map_err(|_| invalid_at(input, "too many UI nodes"))?;
    let stable_name = effective
        .name
        .clone()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("node-{index}"));
    let mut id = UiDocumentRole::node_id(input.role, &stable_name);
    if !output.node_ids.insert(id.0) {
        id = UiDocumentRole::node_id(input.role, &format!("{stable_name}#{index}"));
        output.node_ids.insert(id.0);
    }

    if text_edit_background.is_none() {
        visuals::lower_source_ui_node_visuals_to_canonical_visual_state_definitions(
            &effective, output, input,
        )?;
    }
    let mut style = visuals::lower_source_ui_node_style_to_canonical_style_record(
        &effective, nine_slice, output, input,
    )?;
    if let Some((_, background_source)) = text_edit_background {
        style.background_source = background_source;
    }
    let text = authored_text(&effective).unwrap_or_default().to_owned();
    output
        .field_definitions
        .extend(effective.fields.iter().map(|field| UiNodeFieldDefinition {
            name:
                canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(
                    field.name.as_deref(),
                ),
            value_type:
                canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(
                    field.type_name.as_deref(),
                ),
            format:
                canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(
                    field.format.as_deref(),
                ),
        }));
    for category in &effective.finance_categories {
        output.finance_categories.push(UiFinanceTableCategoryDefinition {
            value_source: authored_ui_widget_vocabulary_resolution::resolve_authored_finance_table_value_source(&category.name)
                .ok_or_else(|| invalid_at(input, format!("unsupported authored finance category {:?}", category.name)))?,
            label: AssetId::from_key(&format!("zoostatus:econ_category_{}", category.name.to_ascii_lowercase())),
            format: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(category.format.as_ref().map(|value| value.as_str())),
        });
    }
    let widget = widgets::lower_source_ui_widget_to_canonical_widget_record(
        &effective, index, id, output, input,
    )?;
    authored_ui_node_property_binding_resolution::lower_authored_ui_node_property_bindings_to_canonical_records(&effective, parent, input, output);
    if stable_name == "purchase tv button" {
        output
            .actions
            .push(UiActionRecord::Show(UiShowActionRecord {
                trigger: UiTrigger::On,
                action: UiShowAction::SelectPlatformUpgrade {
                    upgrade: UiShowPlatformUpgrade::Television,
                },
            }));
    } else if stable_name == "purchase canopy button" {
        output
            .actions
            .push(UiActionRecord::Show(UiShowActionRecord {
                trigger: UiTrigger::On,
                action: UiShowAction::SelectPlatformUpgrade {
                    upgrade: UiShowPlatformUpgrade::Canopy,
                },
            }));
    }
    let audio_setting = match stable_name.to_ascii_lowercase().as_str() {
        "soundvolumeslider" => Some(UiAudioSettingAction::SetMasterVolume),
        "musicvolumeslider" => Some(UiAudioSettingAction::SetMusicVolume),
        "2dsoundfxvolumeslider" => Some(UiAudioSettingAction::SetTwoDimensionalVolume),
        "3dsoundfxvolumeslider" => Some(UiAudioSettingAction::SetThreeDimensionalVolume),
        _ => None,
    };
    if let Some(action) = audio_setting {
        output
            .actions
            .push(UiActionRecord::AudioSetting(UiAudioSettingActionRecord {
                trigger: UiTrigger::Change,
                action,
            }));
    }
    if matches!(
        input.role,
        UiDocumentRole::EntityInfo | UiDocumentRole::PurchaseCatalogue
    ) && stable_name.eq_ignore_ascii_case("zoopedia link")
    {
        output
            .actions
            .push(UiActionRecord::Information(UiInformationActionRecord {
                trigger: UiTrigger::Press,
                action: UiInformationAction::OpenContextEncyclopediaEntry,
            }));
    }
    if input
        .virtual_path
        .eq_ignore_ascii_case("ui/layout/multiinfo/multilistanimal.xml")
        && stable_name.eq_ignore_ascii_case("selector")
    {
        output
            .actions
            .push(UiActionRecord::Information(UiInformationActionRecord {
                trigger: UiTrigger::Press,
                action: UiInformationAction::SelectEntityFromSource,
            }));
    }
    let tree_element = if matches!(&effective.kind, SourceUiWidgetKind::TreeElement) {
        Some(id)
    } else {
        nearest_tree_element(output, parent)
    };
    let scroll_receiver = if matches!(
        &effective.widget,
        SourceUiWidgetData::Slider(_)
            | SourceUiWidgetData::List(_)
            | SourceUiWidgetData::DropList { .. }
            | SourceUiWidgetData::Window(_)
    ) {
        id
    } else {
        nearest_scroll_receiver(output, parent).unwrap_or(id)
    };
    lower_events(
        &effective,
        input.role,
        id,
        &stable_name,
        scroll_receiver,
        tree_element,
        output,
        input,
        confirmation_scope,
    )?;
    let animation = match &effective.widget {
        SourceUiWidgetData::Animation(source) => lower_widget_animation(source, input)?,
        _ => lower_animation(effective.show_hide_animation.as_ref(), input)?,
    };
    let name = stable_name.clone();
    let mut flags = node_flags(&effective);
    let obsolete_eight_bit_sound_control = effective
        .events
        .iter()
        .flat_map(|block| &block.events)
        .any(|event| {
            event.message == "ZT_EVENT"
                && event.string.as_deref().is_some_and(|value| {
                    matches!(
                        value.to_ascii_lowercase().as_str(),
                        "use8bitsoundson" | "use8bitsoundsoff"
                    )
                })
        });
    if obsolete_eight_bit_sound_control {
        flags.0 &= !(UiNodeFlags::VISIBLE.0 | UiNodeFlags::ENABLED.0);
    }
    let authoring_layout_control =
        effective
            .events
            .iter()
            .flat_map(|block| &block.events)
            .any(|event| {
                event.message == "ZT_SETMODE" && event.string.as_deref() == Some("mode_ui_layout")
            });
    if authoring_layout_control {
        flags.0 &= !(UiNodeFlags::VISIBLE.0 | UiNodeFlags::ENABLED.0);
    }

    let cursor = cursor_texture_dependency(effective.cursor.clone(), output, input)?;
    output.nodes.push(UiNodeDefinition {
        id,
        name,
        kind: node_kind(&effective.kind, &effective.widget),
        parent,
        children: Vec::new(),
        rect: numeric_rect(effective.region.as_ref()),
        anchors: [0.0; 4],
        region: lower_region(effective.region.as_ref(), input)?,
        text,
        cursor,
        text_format: effective.text_format.as_deref().map_or_else(AssetId::default, |format| {
            if format.starts_with("openzt2:money:") {
                authored_ui_node_property_binding_resolution::resolve_authored_money_format_localization_key(format, "positive")
            } else {
                canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(Some(format))
            }
        }),
        help: canonical_source_value_resolution::lower_authored_ui_help_to_canonical_definition(effective.help.as_ref()),
        cacheable_from_lua: effective.cacheable_from_lua.unwrap_or(false),
        field_definitions: std::mem::take(&mut output.field_definitions),
        finance_categories: std::mem::take(&mut output.finance_categories),
        style,
        bindings: std::mem::take(&mut output.bindings),
        actions: std::mem::take(&mut output.actions),
        animation,
        visuals: std::mem::take(&mut output.visuals),
        hotkeys: Vec::new(),
        widget,
        scenario_objective_visual: effective.scenario_objective_visual.as_deref().map(|value| objective_status_filter(Some(value), input)).transpose()?,
        flags,
        z: 0,
        tab_order: -1,
    });

    add_lowered_hotkey_definitions_to_current_ui_node(&effective, id, input.role, output, input)?;
    output.nodes[index as usize].hotkeys = std::mem::take(&mut output.hotkeys);

    let mut direct = Vec::with_capacity(effective.children.len());
    for child in &effective.children {
        direct.push(lower_node(
            child,
            index,
            confirmation_scope,
            input,
            templates,
            template_stack,
            output,
        )?);
    }
    resolve_owned_widget_references(&effective, &direct, index, output);
    output.nodes[index as usize].children = direct;
    Ok(index)
}

pub(super) fn resolve_owned_widget_references(
    node: &SourceUiNode,
    direct_children: &[u32],
    node_index: u32,
    output: &mut BuildOutput,
) {
    match &node.widget {
        SourceUiWidgetData::CompositeButton {
            child_button,
            hover_child,
            ..
        } => {
            let child_button = child_button
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            let hover_child = hover_child
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            if let UiWidgetRecord::Button(record) = &mut output.nodes[node_index as usize].widget {
                if let Some(child_button) = child_button {
                    record.child_button = child_button;
                }
                if let Some(hover_child) = hover_child {
                    record.hover_child = hover_child;
                }
            }
        }
        SourceUiWidgetData::DropList {
            list,
            opener,
            drop_list,
            ..
        } => {
            let opener = opener
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            let drop_list = drop_list
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            let count_component = list
                .count_component
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            if let UiWidgetRecord::List {
                opener_node: record_opener_node,
                drop_list_display_node: record_drop_list_display_node,
                count_component: record_count_component,
                ..
            } = &mut output.nodes[node_index as usize].widget
            {
                if let Some(opener) = opener {
                    *record_opener_node = opener;
                }
                if let Some(drop_list) = drop_list {
                    *record_drop_list_display_node = drop_list;
                }
                if let Some(count_component) = count_component {
                    *record_count_component = count_component;
                }
            }
        }
        SourceUiWidgetData::List(list) => {
            let count_component = list
                .count_component
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            if let (
                Some(count_component),
                UiWidgetRecord::List {
                    count_component: record,
                    ..
                },
            ) = (
                count_component,
                &mut output.nodes[node_index as usize].widget,
            ) {
                *record = count_component;
            }
        }
        SourceUiWidgetData::Slider(source) => {
            let thumb = source
                .thumb_name
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            let field = source
                .field
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            if let UiWidgetRecord::Slider(record) = &mut output.nodes[node_index as usize].widget {
                if let Some(thumb) = thumb {
                    record.thumb = thumb;
                }
                if let Some(field) = field {
                    record.field = field;
                }
            }
        }
        SourceUiWidgetData::Window(source) => {
            let title = source
                .title
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            let horizontal_scroll = source
                .horizontal_scroll
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            let vertical_scroll = source
                .vertical_scroll
                .as_deref()
                .and_then(|name| authored_ui_owned_descendant_node_resolution::resolve_owned_authored_ui_descendant_node_id(node, direct_children, name, output));
            if let UiWidgetRecord::Window {
                title: record_title,
                horizontal_scroll: record_horizontal_scroll,
                vertical_scroll: record_vertical_scroll,
                ..
            } = &mut output.nodes[node_index as usize].widget
            {
                if let Some(title) = title {
                    *record_title = title;
                }
                if let Some(horizontal_scroll) = horizontal_scroll {
                    *record_horizontal_scroll = horizontal_scroll;
                }
                if let Some(vertical_scroll) = vertical_scroll {
                    *record_vertical_scroll = vertical_scroll;
                }
            }
        }
        _ => {}
    }
}
