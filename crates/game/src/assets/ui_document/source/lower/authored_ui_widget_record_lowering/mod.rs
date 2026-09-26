use crate::assets::source_document::ui::model::{
    SourceUiActiveState, SourceUiButton, SourceUiDragAxis, SourceUiGlobe, SourceUiGrid,
    SourceUiHitPolicy, SourceUiList, SourceUiListSource, SourceUiNode, SourceUiText,
    SourceUiWidgetData, SourceUiWidgetKind,
};
use crate::assets::ui_document::source::lower::authored_ui_asset_dependency_resolution::{
    cursor_texture_dependency, dependency, resolved_asset_dependency,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::{
    list_row_document_path, AuthoredUiDocument, CAMPAIGN_ROW_DOCUMENT,
    CAMPAIGN_SCENARIO_ROW_DOCUMENT, DISPLAY_RESOLUTION_ROW_DOCUMENT, FINANCE_CATEGORY_ROW_DOCUMENT,
    FINANCE_VALUE_ROW_DOCUMENT, MULTILIST_ROW_DOCUMENTS, OVERVIEW_LEGEND_ROW_DOCUMENT,
    PHOTO_ALBUM_CHOICE_ROW_DOCUMENT, PHOTO_CAMERA_ROLL_ROW_DOCUMENT, PROFILE_ROW_DOCUMENT,
    SELECTED_ANIMAL_NEED_ROW_DOCUMENT, SELECTED_ENTITY_INVENTORY_ROW_DOCUMENT,
};
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use crate::assets::ui_document::source::lower::canonical_source_value_resolution;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::globe_presentation::{
    UiGlobeBiomeModelDefinition, UiGlobePresentationDefinition,
};
use openzt2_game_data::ui_document::node::{UiNodeFlags, UiNodeKind};
use openzt2_game_data::ui_document::node_presentation::{
    UiNodePointerHitPolicy, UiNodeVisualState,
};
use openzt2_game_data::ui_document::widget_control::{
    UiAxis, UiButtonRecord, UiGridRecord, UiTextRecord,
};
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn node_kind(kind: &SourceUiWidgetKind, widget: &SourceUiWidgetData) -> UiNodeKind {
    // `toggle` is an initial selection value, not a boolean enabling toggle
    // behavior. UIButton templates can declare toggle="0" and still latch.
    if matches!(
        kind,
        SourceUiWidgetKind::Button
            | SourceUiWidgetKind::HoverButton
            | SourceUiWidgetKind::PushButton
    ) && matches!(widget, SourceUiWidgetData::Button(button) if button.toggle.is_some())
    {
        return UiNodeKind::Toggle;
    }
    match kind {
        SourceUiWidgetKind::Root => UiNodeKind::Root,
        SourceUiWidgetKind::App => UiNodeKind::App,
        SourceUiWidgetKind::Container
        | SourceUiWidgetKind::Layout
        | SourceUiWidgetKind::RailCamera
        | SourceUiWidgetKind::Static => UiNodeKind::Container,
        SourceUiWidgetKind::Button
        | SourceUiWidgetKind::HoverButton
        | SourceUiWidgetKind::PushButton => UiNodeKind::Button,
        SourceUiWidgetKind::ToggleButton | SourceUiWidgetKind::ToggleHoverButton => {
            UiNodeKind::Toggle
        }
        SourceUiWidgetKind::CompositeButton | SourceUiWidgetKind::CompositeHoverButton => {
            UiNodeKind::CompositeButton
        }
        SourceUiWidgetKind::Slider => UiNodeKind::Slider,
        SourceUiWidgetKind::ListBox | SourceUiWidgetKind::MultiList => UiNodeKind::List,
        SourceUiWidgetKind::Text => UiNodeKind::Text,
        SourceUiWidgetKind::TextBuffer => UiNodeKind::TextBuffer,
        SourceUiWidgetKind::TextEdit => UiNodeKind::Edit,
        SourceUiWidgetKind::Animation => UiNodeKind::Animation,
        SourceUiWidgetKind::Drag => UiNodeKind::Drag,
        SourceUiWidgetKind::DragCommand => UiNodeKind::DragCommand,
        SourceUiWidgetKind::DropList => UiNodeKind::DropList,
        SourceUiWidgetKind::Graph => UiNodeKind::Graph,
        SourceUiWidgetKind::MultiIcon => UiNodeKind::MultiIcon,
        SourceUiWidgetKind::WorldMap => UiNodeKind::WorldMap,
        SourceUiWidgetKind::SoundManager => UiNodeKind::SoundManager,
        SourceUiWidgetKind::ToggleSet => UiNodeKind::ToggleSet,
        SourceUiWidgetKind::Globe => UiNodeKind::Globe,
        SourceUiWidgetKind::Tool => UiNodeKind::Tool,
        SourceUiWidgetKind::Tooltip => UiNodeKind::Tooltip,
        SourceUiWidgetKind::TreeElement => UiNodeKind::TreeElement,
        SourceUiWidgetKind::TimedEvents => UiNodeKind::TimedPresentation,
        SourceUiWidgetKind::ContextList => UiNodeKind::ContextList,
        SourceUiWidgetKind::FullscreenButton => UiNodeKind::FullscreenButton,
        SourceUiWidgetKind::GoalPanel => UiNodeKind::GoalPanel,
        SourceUiWidgetKind::PhotoAlbum => UiNodeKind::PhotoAlbum,
        SourceUiWidgetKind::TypeList => UiNodeKind::TypeList,
        SourceUiWidgetKind::Window => UiNodeKind::Window,
        SourceUiWidgetKind::XmlEdit => UiNodeKind::XmlEdit,
        SourceUiWidgetKind::Unknown(_) => UiNodeKind::Container,
    }
}

pub(super) fn node_flags(node: &SourceUiNode) -> UiNodeFlags {
    let mut flags = UiNodeFlags::default();
    if node.state.visible {
        flags = flags | UiNodeFlags::VISIBLE;
    }
    if matches!(node.state.active, SourceUiActiveState::Normal) {
        flags = flags | UiNodeFlags::ENABLED;
    }
    if node.modal == Some(true) {
        flags = flags | UiNodeFlags::MODAL;
    }
    if node.use_list_box_color_if_in_container == Some(true) {
        flags = flags | UiNodeFlags::USE_LIST_BOX_COLOR;
    }
    if node.clip_children {
        flags = flags | UiNodeFlags::CLIP_CHILDREN;
    }
    if node.notify_animation_completed == Some(true) {
        flags = flags | UiNodeFlags::NOTIFY_ANIMATION_COMPLETED;
    }
    if node.name.as_deref() == Some("MOTD items") {
        flags = flags | UiNodeFlags::ONLINE_MESSAGE_SURFACE;
    }
    match node.name.as_deref() {
        Some("MOTDListBox") => flags = flags | UiNodeFlags::ONLINE_MESSAGE_LIST,
        Some("MOTDText") => flags = flags | UiNodeFlags::ONLINE_MESSAGE_TEXT,
        Some("MOTDIcon") => flags = flags | UiNodeFlags::ONLINE_MESSAGE_ICON,
        Some("feedbackMessageList") => flags = flags | UiNodeFlags::FEEDBACK_ALERT_LIST,
        Some("selector") => flags = flags | UiNodeFlags::FEEDBACK_ALERT_SELECTOR,
        Some("messageEntry") => flags = flags | UiNodeFlags::FEEDBACK_ALERT_TEXT,
        Some("clickable icon") => flags = flags | UiNodeFlags::FEEDBACK_ALERT_ICON,
        _ => {}
    }
    let hit = node
        .always_hit
        .as_ref()
        .or_else(|| node.aspect.as_ref().map(|aspect| &aspect.hit_policy));
    if !matches!(hit, Some(SourceUiHitPolicy::Never)) {
        flags = flags | UiNodeFlags::POINTER_BLOCKING;
    }
    flags
}

pub(super) fn authored_text(node: &SourceUiNode) -> Option<&str> {
    node.aspect
        .as_ref()
        .and_then(|aspect| aspect.authored_string.as_deref())
        .or_else(|| match &node.widget {
            SourceUiWidgetData::Text(text) | SourceUiWidgetData::TextEdit { text, .. } => {
                text.authored_string.as_deref()
            }
            _ => None,
        })
}
pub(super) fn hit_policy(
    value: Option<&SourceUiHitPolicy>,
    input: &AuthoredUiDocument,
) -> io::Result<UiNodePointerHitPolicy> {
    match value.unwrap_or(&SourceUiHitPolicy::Normal) {
        SourceUiHitPolicy::Normal => Ok(UiNodePointerHitPolicy::Normal),
        SourceUiHitPolicy::Always => Ok(UiNodePointerHitPolicy::Always),
        SourceUiHitPolicy::Never => Ok(UiNodePointerHitPolicy::Never),
        SourceUiHitPolicy::Region => Ok(UiNodePointerHitPolicy::Region),
        SourceUiHitPolicy::Unknown(value) => {
            Err(invalid_at(input, format!("unknown hit policy {value:?}")))
        }
    }
}
pub(super) fn visual_state(
    value: &str,
    alternate: bool,
    input: &AuthoredUiDocument,
) -> io::Result<UiNodeVisualState> {
    match (alternate, value) {
        (false, "default") => Ok(UiNodeVisualState::Default),
        (false, "normal") => Ok(UiNodeVisualState::Normal),
        (false, "highlighted") => Ok(UiNodeVisualState::Highlighted),
        (false, "activated") => Ok(UiNodeVisualState::Activated),
        (false, "disabled") => Ok(UiNodeVisualState::Disabled),
        (true, "normal") => Ok(UiNodeVisualState::AlternateNormal),
        (true, "highlighted") => Ok(UiNodeVisualState::AlternateHighlighted),
        (true, "activated") => Ok(UiNodeVisualState::AlternateActivated),
        (true, "disabled") => Ok(UiNodeVisualState::AlternateDisabled),
        _ => Err(invalid_at(
            input,
            format!("unknown UI visual state {value:?}"),
        )),
    }
}
pub(super) fn grid_record(value: &SourceUiGrid) -> UiGridRecord {
    UiGridRecord {
        auto_size: value.auto_size.unwrap_or(false),
        auto_size_parent: value.auto_size_parent.unwrap_or(false),
        columns: value.columns.unwrap_or(0),
        rows: value.rows.unwrap_or(0),
        x_spacing: value.x_spacing.unwrap_or(0),
        y_spacing: value.y_spacing.unwrap_or(0),
        column_width: value.column_width.unwrap_or(0),
        row_height: value.row_height.unwrap_or(0),
        initial_x: value.initial_x.unwrap_or(0),
        initial_y: value.initial_y.unwrap_or(0),
    }
}

pub(super) fn list_source(
    role: UiDocumentRole,
    name: Option<&str>,
    list: &crate::assets::source_document::ui::model::SourceUiList,
) -> UiWidgetLiveCollectionSource {
    match list.source {
        SourceUiListSource::SelectedEntityInventory => {
            UiWidgetLiveCollectionSource::SelectedEntityInventory
        }
        SourceUiListSource::ScenarioObjectives => UiWidgetLiveCollectionSource::ScenarioObjectives,
        SourceUiListSource::ZoopediaTableOfContents => {
            UiWidgetLiveCollectionSource::ZoopediaTableOfContents
        }
        SourceUiListSource::Unbound => unbound_list_source(role, name),
    }
}

pub(super) fn unbound_list_source(
    role: UiDocumentRole,
    name: Option<&str>,
) -> UiWidgetLiveCollectionSource {
    match (role, name) {
        (UiDocumentRole::MapSelect, Some(name)) if name.eq_ignore_ascii_case("campaign list") => {
            UiWidgetLiveCollectionSource::Campaigns
        }
        (UiDocumentRole::MapSelect, Some(name))
            if name.eq_ignore_ascii_case("campaign scenario list") =>
        {
            UiWidgetLiveCollectionSource::CampaignScenarios
        }
        (UiDocumentRole::EntityInfo, Some("Inventory")) => {
            UiWidgetLiveCollectionSource::SelectedEntityInventory
        }
        (UiDocumentRole::EntityInfo, Some(name))
            if name.eq_ignore_ascii_case("basic animal needs") =>
        {
            UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds
        }
        (UiDocumentRole::EntityInfo, Some(name))
            if name.eq_ignore_ascii_case("advanced animal needs") =>
        {
            UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds
        }
        (UiDocumentRole::Globe | UiDocumentRole::MapSelect, Some(name))
            if name.eq_ignore_ascii_case("map list") =>
        {
            UiWidgetLiveCollectionSource::WorldChoices
        }
        (UiDocumentRole::Globe | UiDocumentRole::MapSelect, Some(name))
            if name.eq_ignore_ascii_case("location list") =>
        {
            UiWidgetLiveCollectionSource::WorldChoicePrototype
        }
        (UiDocumentRole::Overview, Some(name)) if name.eq_ignore_ascii_case("filterLegend") => {
            UiWidgetLiveCollectionSource::OverviewLayers
        }
        (UiDocumentRole::PhotoAlbum, Some(name)) if name.eq_ignore_ascii_case("exposures") => {
            UiWidgetLiveCollectionSource::PhotoCameraRoll
        }
        (UiDocumentRole::PhotoAlbum, Some(name)) if name.eq_ignore_ascii_case("albumlist") => {
            UiWidgetLiveCollectionSource::PhotoAlbums
        }
        (UiDocumentRole::ZooStatus, Some(name))
            if name.eq_ignore_ascii_case("balance sheet categories") =>
        {
            UiWidgetLiveCollectionSource::FinanceBalanceSheetCategories
        }
        (UiDocumentRole::ZooStatus, Some(name))
            if name.eq_ignore_ascii_case("balance sheet months") =>
        {
            UiWidgetLiveCollectionSource::FinanceBalanceSheetMonths
        }
        (UiDocumentRole::ZooStatus, Some(name))
            if name.eq_ignore_ascii_case("balance sheet items") =>
        {
            UiWidgetLiveCollectionSource::FinanceBalanceSheetMonthColumns
        }
        (UiDocumentRole::Fragment, Some(name))
            if name.eq_ignore_ascii_case("balance sheet item") =>
        {
            UiWidgetLiveCollectionSource::FinanceBalanceSheetMonthValues
        }
        (UiDocumentRole::ZooStatus, Some(name)) if name.eq_ignore_ascii_case("buildings list") => {
            UiWidgetLiveCollectionSource::FinanceBuildings
        }
        (UiDocumentRole::ZooStatus, Some(name))
            if name.eq_ignore_ascii_case("donation boxes list") =>
        {
            UiWidgetLiveCollectionSource::FinanceDonationBoxes
        }
        (UiDocumentRole::ZooStatus, Some(name))
            if name.eq_ignore_ascii_case("donations by species list") =>
        {
            UiWidgetLiveCollectionSource::FinanceDonationsBySpecies
        }
        (UiDocumentRole::ZooStatus, Some(name))
            if name.eq_ignore_ascii_case("tour donation list") =>
        {
            UiWidgetLiveCollectionSource::FinanceTourDonations
        }
        (UiDocumentRole::ZooStatus, Some(name))
            if name.eq_ignore_ascii_case("show donation list") =>
        {
            UiWidgetLiveCollectionSource::FinanceShowDonations
        }
        _ => UiWidgetLiveCollectionSource::Unbound,
    }
}

pub(super) fn list_row_document(
    role: UiDocumentRole,
    name: Option<&str>,
    list: &SourceUiList,
) -> AssetId {
    list.row_template
        .as_ref()
        .map(|value| AssetId::from_virtual_path(&list_row_document_path(value)))
        .unwrap_or_else(|| match list_source(role, name, list) {
            UiWidgetLiveCollectionSource::SelectedEntityInventory => {
                AssetId::from_virtual_path(SELECTED_ENTITY_INVENTORY_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::SelectedAnimalBasicNeeds
            | UiWidgetLiveCollectionSource::SelectedAnimalAdvancedNeeds => {
                AssetId::from_virtual_path(SELECTED_ANIMAL_NEED_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::DisplayResolutions => {
                AssetId::from_virtual_path(DISPLAY_RESOLUTION_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::ProfileIndex => {
                AssetId::from_virtual_path(PROFILE_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::Campaigns => {
                AssetId::from_virtual_path(CAMPAIGN_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::CampaignScenarios => {
                AssetId::from_virtual_path(CAMPAIGN_SCENARIO_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::OverviewLayers => {
                AssetId::from_virtual_path(OVERVIEW_LEGEND_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::PhotoCameraRoll => {
                AssetId::from_virtual_path(PHOTO_CAMERA_ROLL_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::PhotoAlbums => {
                AssetId::from_virtual_path(PHOTO_ALBUM_CHOICE_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::FinanceBalanceSheetCategories => {
                AssetId::from_virtual_path(FINANCE_CATEGORY_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::FinanceBalanceSheetMonths => {
                AssetId::from_virtual_path("ui/fragment/ui/layout/balancesheetmonth.xml")
            }
            UiWidgetLiveCollectionSource::FinanceBalanceSheetMonthColumns => {
                AssetId::from_virtual_path("ui/fragment/ui/layout/balancesheetitem.xml")
            }
            UiWidgetLiveCollectionSource::FinanceBalanceSheetMonthValues => {
                AssetId::from_virtual_path(FINANCE_VALUE_ROW_DOCUMENT)
            }
            UiWidgetLiveCollectionSource::FinanceBuildings => {
                AssetId::from_virtual_path("ui/fragment/ui/layout/buildinglistitem.xml")
            }
            UiWidgetLiveCollectionSource::FinanceDonationBoxes => {
                AssetId::from_virtual_path("ui/fragment/ui/layout/donationboxlistitem.xml")
            }
            UiWidgetLiveCollectionSource::FinanceDonationsBySpecies
            | UiWidgetLiveCollectionSource::FinanceTourDonations => {
                AssetId::from_virtual_path("ui/fragment/ui/layout/donationsbyspeciesitem.xml")
            }
            UiWidgetLiveCollectionSource::FinanceShowDonations => {
                AssetId::from_virtual_path("ui/fragment/ui/layout/showdonationentry.xml")
            }
            UiWidgetLiveCollectionSource::Unbound if role == UiDocumentRole::MultiList => name
                .and_then(|name| {
                    MULTILIST_ROW_DOCUMENTS
                        .iter()
                        .find(|(candidate, _)| name.eq_ignore_ascii_case(candidate))
                })
                .map_or_else(AssetId::default, |(_, path)| {
                    AssetId::from_virtual_path(path)
                }),
            UiWidgetLiveCollectionSource::Unbound
            | UiWidgetLiveCollectionSource::WorldChoices
            | UiWidgetLiveCollectionSource::WorldChoicePrototype
            | UiWidgetLiveCollectionSource::ScenarioObjectives
            | UiWidgetLiveCollectionSource::ZoopediaTableOfContents => AssetId::default(),
        })
}

pub(super) fn button_record(value: &SourceUiButton) -> UiButtonRecord {
    UiButtonRecord {
        auto_size: value.auto_size.unwrap_or(false),
        minimum_height: value.min_height.unwrap_or(0),
        initially_selected: value.toggle.unwrap_or(false),
        sticky: value.sticky.unwrap_or(false),
        repress: value.repress.unwrap_or(false),
        repeat_delay_seconds: value.repeat_delay_seconds.unwrap_or(0.0),
        hold_change: value.hold_change.unwrap_or(1.0),
        hold_interval_cap_seconds: value.hold_interval_cap_seconds.unwrap_or(0.0),
        broadcast: value.broadcast.unwrap_or(false),
        delayed_activation_seconds: value.delayed_activation_seconds.unwrap_or(0.0),
        activation_data:
            canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(
                value.activate_data.as_deref(),
            ),
        child_button: AssetId::default(),
        hover_child: AssetId::default(),
    }
}
pub(super) fn text_record(value: &SourceUiText, output: &mut BuildOutput) -> UiTextRecord {
    UiTextRecord {
        auto_size: value.auto_size.unwrap_or(false),
        minimum_height: value.min_height.unwrap_or(0),
        localization_key: dependency(value.localization_id.clone(), output),
        value: value
            .authored_string
            .as_deref()
            .unwrap_or_default()
            .to_owned(),
        format: dependency(value.text_format.clone(), output),
    }
}
pub(super) fn axis(value: &SourceUiDragAxis, input: &AuthoredUiDocument) -> io::Result<UiAxis> {
    match value {
        SourceUiDragAxis::X => Ok(UiAxis::X),
        SourceUiDragAxis::Y => Ok(UiAxis::Y),
        SourceUiDragAxis::Both => Ok(UiAxis::Both),
        SourceUiDragAxis::Unknown(value) => {
            Err(invalid_at(input, format!("unknown UI axis {value:?}")))
        }
    }
}
pub(super) fn globe_record(
    _node: u32,
    value: &SourceUiGlobe,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<UiGlobePresentationDefinition> {
    let mut biome_models = Vec::new();
    for (biome, model) in &value.secondary_models {
        let model = resolved_asset_dependency(
            Some(model.key()),
            &input.resolved_dependencies.models,
            "model",
            output,
            input,
        )?;
        biome_models.push(UiGlobeBiomeModelDefinition {
            biome:
                canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(
                    Some(biome),
                ),
            model,
        });
    }
    Ok(UiGlobePresentationDefinition {
        primary_model: resolved_asset_dependency(
            value.primary_model.as_ref().map(|p| p.key()),
            &input.resolved_dependencies.models,
            "model",
            output,
            input,
        )?,
        primary_translation: globe_translation(value.primary_translation),
        clouds_model: resolved_asset_dependency(
            value.clouds_model.as_ref().map(|p| p.key()),
            &input.resolved_dependencies.models,
            "model",
            output,
            input,
        )?,
        clouds_translation: globe_translation(value.clouds_translation),
        dot_model: resolved_asset_dependency(
            value.dot_model.as_ref().map(|p| p.key()),
            &input.resolved_dependencies.models,
            "model",
            output,
            input,
        )?,
        dot_translation: globe_translation(value.dot_translation),
        selected_dot_model: resolved_asset_dependency(
            value.selected_dot_model.as_ref().map(|p| p.key()),
            &input.resolved_dependencies.models,
            "model",
            output,
            input,
        )?,
        selected_dot_translation: globe_translation(value.selected_dot_translation),
        pointer_model: resolved_asset_dependency(
            value.pointer_model.as_ref().map(|p| p.key()),
            &input.resolved_dependencies.models,
            "model",
            output,
            input,
        )?,
        selection_rotate_speed: value.selection_rotate_speed.unwrap_or(0.0),
        mouse_increment: value.mouse_increment.unwrap_or(0.0),
        mouse_down_friction: value.mouse_down_friction.unwrap_or(0.0),
        mouse_up_friction: value.mouse_up_friction.unwrap_or(0.0),
        friction_transition_seconds: value.friction_transition_seconds.unwrap_or(0.0),
        move_seconds: value.move_seconds.unwrap_or(0.0),
        scream_threshold: value.scream_threshold.unwrap_or(0.0),
        scream_delay_seconds: value.scream_delay_seconds.unwrap_or(0.0),
        dot_highlight_rgba: value.dot_highlight_color.map_or([255; 4], |color| {
            [color.red, color.green, color.blue, color.alpha]
        }),
        dot_highlight_cursor: cursor_texture_dependency(
            value.dot_highlight_cursor.clone(),
            output,
            input,
        )?,
        biome_models: biome_models,
    })
}
pub(super) fn globe_translation(value: Option<[f32; 3]>) -> [f32; 3] {
    value.map_or([0.0; 3], |[x, y, z]| [x, z, y])
}
