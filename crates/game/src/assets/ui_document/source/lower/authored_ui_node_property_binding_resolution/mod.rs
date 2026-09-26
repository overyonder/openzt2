//! Resolution of authored UI node semantics to canonical property bindings.

use crate::assets::source_document::ui::model::{
    SourceUiEvent, SourceUiField, SourceUiNode, SourceUiWidgetData,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::parse_bool;
use crate::assets::ui_document::source::lower::canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiImagePropertyBindingSource, UiIntegerPropertyBindingSource,
    UiNodePropertyBinding, UiShellPresentationSlotBinding, UiTextPropertyBindingSource,
    UiTranquilizerHeadsUpDisplaySlotBinding,
};
use openzt2_game_data::AssetId;
use std::collections::BTreeSet;

/// Lowers the authored coupling between simulation pause commands and their
/// presentation target into a direct native binding. Both directions must be
/// present, so an unrelated show beside a pause command cannot be mistaken
/// for persistent paused-state presentation.
pub(super) fn collect_authored_simulation_paused_visibility_target_names(
    root: &SourceUiNode,
) -> BTreeSet<String> {
    fn normalized(value: &str) -> String {
        value
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }
    fn pause_value(event: &SourceUiEvent) -> Option<bool> {
        match event.message.as_str() {
            "ZT_PAUSE" | "ZT_TEMPORARY_PAUSE" => match event
                .string
                .as_deref()
                .or(event.value.as_deref())
                .or(event.data.as_deref())
            {
                Some("pause") => Some(true),
                Some("unpause") => Some(false),
                value => value.and_then(parse_bool).or(Some(true)),
            },
            _ => None,
        }
    }
    fn collect(
        node: &SourceUiNode,
        shown_when_paused: &mut BTreeSet<String>,
        hidden_when_unpaused: &mut BTreeSet<String>,
    ) {
        for block in &node.events {
            let pause = block.events.iter().find_map(pause_value);
            let Some(pause) = pause else { continue };
            let expected = if pause { "UI_SHOW" } else { "UI_HIDE" };
            for event in &block.events {
                if event.message != "UI_CHILD"
                    || event.child.as_deref().map(|child| child.message.as_str()) != Some(expected)
                {
                    continue;
                }
                let Some(target) = event.target_child.as_deref() else {
                    continue;
                };
                if pause {
                    shown_when_paused.insert(normalized(target));
                } else {
                    hidden_when_unpaused.insert(normalized(target));
                }
            }
        }
        node.children.iter().for_each(|child| {
            collect(child, shown_when_paused, hidden_when_unpaused);
        });
    }

    let mut shown_when_paused = BTreeSet::new();
    let mut hidden_when_unpaused = BTreeSet::new();
    collect(root, &mut shown_when_paused, &mut hidden_when_unpaused);
    shown_when_paused
        .intersection(&hidden_when_unpaused)
        .cloned()
        .collect()
}

pub(super) fn lower_authored_ui_node_property_bindings_to_canonical_records(
    node: &SourceUiNode,
    parent: u32,
    input: &AuthoredUiDocument,
    output: &mut BuildOutput,
) {
    if node
        .name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case("research progress bar"))
    {
        output.bindings.push(UiNodePropertyBinding::IntegerValue(
            UiIntegerPropertyBindingSource::SelectedCatalogueResearchProgressBasisPoints,
        ));
    }
    let role = input.role;
    let research_cost = output
        .nodes
        .get(parent as usize)
        .is_some_and(|parent| parent.name.eq_ignore_ascii_case("research cost"));

    let normalized_name = node
        .name
        .as_deref()
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if output
        .simulation_paused_visible_nodes
        .contains(&normalized_name)
    {
        output
            .bindings
            .push(UiNodePropertyBinding::SimulationPausedVisibility);
    }
    let shell_presentation = match (role, normalized_name.as_str()) {
        (UiDocumentRole::Modal, "exitzoo") => {
            Some(UiShellPresentationSlotBinding::ExitConfirmation)
        }
        _ => None,
    };
    if let Some(binding) = shell_presentation {
        output
            .bindings
            .push(UiNodePropertyBinding::ShellPresentationSlot(binding));
    }
    if role == UiDocumentRole::Zoopedia {
        let binding = match normalized_name.as_str() {
            "entryname" => Some(UiTextPropertyBindingSource::ZoopediaTitle),
            "entrytextpage1" => Some(UiTextPropertyBindingSource::ZoopediaBody),
            _ => None,
        };
        if let Some(binding) = binding {
            output
                .bindings
                .push(UiNodePropertyBinding::TextContent(binding));
        }
    }
    if role == UiDocumentRole::PhotoAlbum {
        if let Some(slot) = normalized_name
            .strip_prefix("caption")
            .and_then(|slot| slot.parse::<u8>().ok())
            .filter(|slot| *slot < 8)
        {
            output.bindings.push(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::PhotoAlbumCaption { slot },
            ));
        }
    }
    let normalized_virtual_path = input.virtual_path.to_ascii_lowercase();
    let information_list_row_binding = if normalized_virtual_path
        .starts_with("ui/layout/multiinfo/multilist")
    {
        match (&node.widget, normalized_name.as_str()) {
            (SourceUiWidgetData::Text(_), "name") => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::SelectedEntityName,
            )),
            (_, "icon") => Some(UiNodePropertyBinding::ImageContent(
                UiImagePropertyBindingSource::SelectedEntityIcon,
            )),
            (SourceUiWidgetData::MultiIcon(entries), "iconbackground")
                if entries
                    .iter()
                    .any(|entry| entry.key.as_deref() == Some("animal_happy")) =>
            {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::InformationListAnimalHappinessIcon,
                ))
            }
            (SourceUiWidgetData::MultiIcon(entries), "iconbackground")
                if entries
                    .iter()
                    .any(|entry| entry.key.as_deref() == Some("animal_sick")) =>
            {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::InformationListAnimalHealthIcon,
                ))
            }
            (SourceUiWidgetData::MultiIcon(entries), "iconbackground")
                if entries
                    .iter()
                    .any(|entry| entry.key.as_deref() == Some("animal_pregnant")) =>
            {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::InformationListAnimalPregnancyIcon,
                ))
            }
            (SourceUiWidgetData::MultiIcon(entries), "iconbackground")
                if entries
                    .iter()
                    .any(|entry| entry.key.as_deref() == Some("guest_happy")) =>
            {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::InformationListGuestHappinessIcon,
                ))
            }
            (SourceUiWidgetData::Text(_), "tasklabel") => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::StaffCurrentJobName,
            )),
            (SourceUiWidgetData::Text(_), "numassignments") => {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::InformationListStaffAssignmentCount,
                ))
            }
            (SourceUiWidgetData::Text(_), "months") => Some(UiNodePropertyBinding::IntegerValue(
                UiIntegerPropertyBindingSource::FacilityOperatingMonths,
            )),
            (SourceUiWidgetData::Text(_), "profit") => Some(UiNodePropertyBinding::IntegerValue(
                UiIntegerPropertyBindingSource::FacilityProfitCents,
            )),
            (SourceUiWidgetData::Text(_), "avgprofit") => {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::FacilityAverageProfitCents,
                ))
            }
            (SourceUiWidgetData::Text(_), "occupancy") => {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::TransportVehicleOccupied,
                ))
            }
            (SourceUiWidgetData::Text(_), "capacity") => Some(UiNodePropertyBinding::IntegerValue(
                UiIntegerPropertyBindingSource::TransportVehicleSeats,
            )),
            (SourceUiWidgetData::Text(_), "state") => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::TransportTripState,
            )),
            _ => None,
        }
    } else {
        match (normalized_virtual_path.as_str(), normalized_name.as_str()) {
            ("ui/layout/econlistcategory.xml", "label") => {
                Some(UiNodePropertyBinding::TextContent(
                    UiTextPropertyBindingSource::FinanceBalanceSheetCategoryLabel,
                ))
            }
            ("ui/layout/econlistitem.xml", "value") => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::FinanceBalanceSheetCategoryValue,
            )),
            ("ui/layout/balancesheetmonth.xml", "monthname") => {
                Some(UiNodePropertyBinding::TextContent(
                    UiTextPropertyBindingSource::FinanceBalanceSheetMonthName,
                ))
            }
            ("ui/layout/buildinglistitem.xml" | "ui/layout/donationboxlistitem.xml", "name") => {
                Some(UiNodePropertyBinding::TextContent(
                    UiTextPropertyBindingSource::SelectedEntityName,
                ))
            }
            (
                "ui/layout/buildinglistitem.xml" | "ui/layout/donationboxlistitem.xml",
                "monthsinoperation",
            ) => Some(UiNodePropertyBinding::IntegerValue(
                UiIntegerPropertyBindingSource::FacilityOperatingMonths,
            )),
            ("ui/layout/buildinglistitem.xml", "users") => {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::FacilityCapacityUsed,
                ))
            }
            ("ui/layout/buildinglistitem.xml", "profit") => {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::FacilityProfitCents,
                ))
            }
            ("ui/layout/buildinglistitem.xml", "averageprofit") => {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::FacilityAverageProfitCents,
                ))
            }
            ("ui/layout/donationboxlistitem.xml", "donationsnumber") => Some(
                UiNodePropertyBinding::IntegerValue(UiIntegerPropertyBindingSource::DonationCount),
            ),
            ("ui/layout/donationboxlistitem.xml", "donationsamount") => {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::DonationTotalCents,
                ))
            }
            ("ui/layout/donationboxlistitem.xml", "averagedonationamount") => {
                Some(UiNodePropertyBinding::IntegerValue(
                    UiIntegerPropertyBindingSource::DonationAverageCents,
                ))
            }
            _ => None,
        }
    };
    let information_list_row_has_typed_value_binding = information_list_row_binding.is_some();
    if let Some(binding) = information_list_row_binding {
        output.bindings.push(binding);
    }
    if role == UiDocumentRole::TranquilizerHud {
        let binding = match normalized_name.as_str() {
            "tranqgunscreen" => Some(UiTranquilizerHeadsUpDisplaySlotBinding::Screen),
            "tranqguntargetname" => Some(UiTranquilizerHeadsUpDisplaySlotBinding::TargetName),
            "tranqguntargetimage" => Some(UiTranquilizerHeadsUpDisplaySlotBinding::TargetImage),
            "tranqgundistancelabel" => Some(UiTranquilizerHeadsUpDisplaySlotBinding::DistanceLabel),
            "tranqgundistancetext" => Some(UiTranquilizerHeadsUpDisplaySlotBinding::DistanceText),
            "tranqgunoutofrangelabel" => {
                Some(UiTranquilizerHeadsUpDisplaySlotBinding::OutOfRangeLabel)
            }
            "tranqgunmisfireindicator" => {
                Some(UiTranquilizerHeadsUpDisplaySlotBinding::MisfireIndicator)
            }
            "tranqaimingreticule" => Some(UiTranquilizerHeadsUpDisplaySlotBinding::Reticle),
            "tranqgunchargingbarleft"
            | "tranqgunchargingbarright"
            | "tranqgunchargingbardisplay" => {
                Some(UiTranquilizerHeadsUpDisplaySlotBinding::ChargeBar)
            }
            _ => None,
        };
        if let Some(binding) = binding {
            output
                .bindings
                .push(UiNodePropertyBinding::TranquilizerHeadsUpDisplaySlot(
                    binding,
                ));
        }
    }
    if matches!(role, UiDocumentRole::Globe | UiDocumentRole::MapSelect) {
        let binding = match node
            .name
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("map name label" | "map name field") => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::SelectedWorldName,
            )),
            Some("map location" | "map location field") => {
                Some(UiNodePropertyBinding::TextContent(
                    UiTextPropertyBindingSource::SelectedWorldLocation,
                ))
            }
            Some("map biome" | "map biome field") => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::SelectedWorldBiome,
            )),
            Some("map size") => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::SelectedWorldSize,
            )),
            Some("scenario description") => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::SelectedWorldDescription,
            )),
            Some("map image") => Some(UiNodePropertyBinding::ImageContent(
                UiImagePropertyBindingSource::SelectedWorldThumbnail,
            )),
            Some("starting cash slider") => Some(UiNodePropertyBinding::Visibility(
                UiBooleanPropertyBindingSource::SelectedWorldAdjustableCash,
            )),
            Some("unlimited display") => Some(UiNodePropertyBinding::Visibility(
                UiBooleanPropertyBindingSource::SelectedWorldUnlimitedCash,
            )),
            Some("difficulty header" | "scenario difficulty") => {
                Some(UiNodePropertyBinding::Visibility(
                    UiBooleanPropertyBindingSource::SelectedWorldDifficulty,
                ))
            }
            Some("number") if matches!(&node.widget, SourceUiWidgetData::Text(_)) => {
                Some(UiNodePropertyBinding::TextContent(
                    UiTextPropertyBindingSource::SelectedWorldStartingCash,
                ))
            }
            _ => None,
        };
        if let Some(binding) = binding {
            output.bindings.push(binding);
        }
    }
    if role == UiDocumentRole::ZooStatus {
        if normalized_name == "userstringzooname"
            && matches!(&node.widget, SourceUiWidgetData::TextEdit { .. })
        {
            output.bindings.push(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::ZooName,
            ));
        }
        if let ("numguestnum", SourceUiWidgetData::Text(text)) =
            (normalized_name.as_str(), &node.widget)
        {
            output.bindings.push(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::ZooGuestCount {
                    format: lower_optional_authored_semantic_key_to_asset_id(
                        text.text_format.as_deref(),
                    ),
                },
            ));
        }
        let donation_summary_count_category = match normalized_name.as_str() {
            "educationnumber" => Some(Some(AssetId::from_key("education-donation"))),
            "allspeciesnumber" => Some(Some(AssetId::from_key("animal-donation"))),
            "toursnumber" => Some(Some(AssetId::from_key("tour-donation"))),
            "shownumber" => Some(Some(AssetId::from_key("show-donation"))),
            "combinednumber" => Some(None),
            _ => None,
        };
        if let (Some(category), SourceUiWidgetData::Text(text)) =
            (donation_summary_count_category, &node.widget)
        {
            output.bindings.push(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::ZooDonationSummaryDonationCount {
                    category,
                    format: lower_optional_authored_semantic_key_to_asset_id(
                        text.text_format.as_deref(),
                    ),
                },
            ));
        }
    }
    if role == UiDocumentRole::InGameHud && normalized_name == "biomelabel" {
        output.bindings.push(UiNodePropertyBinding::TextContent(
            UiTextPropertyBindingSource::SelectedTerrainBiomeName,
        ));
    }
    if normalized_name == "openzt2scenariooverviewtext" {
        output.bindings.push(UiNodePropertyBinding::TextContent(
            UiTextPropertyBindingSource::ScenarioDescription,
        ));
    }
    if role == UiDocumentRole::AnimalCareCatalogue {
        let binding = match normalized_name.as_str() {
            "sortnamelabel" => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::CatalogueEntryName { row: 0 },
            )),
            "sortimagedisplay" => Some(UiNodePropertyBinding::ImageContent(
                UiImagePropertyBindingSource::CatalogueEntryIcon { row: 0 },
            )),
            _ => None,
        };
        if let Some(binding) = binding {
            output.bindings.push(binding);
        }
    }
    if role == UiDocumentRole::PurchaseCatalogue {
        let binding = match normalized_name.as_str() {
            "entitynamelabel" => Some(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::CatalogueEntryName { row: 0 },
            )),
            "entityimagedisplay" => Some(UiNodePropertyBinding::ImageContent(
                UiImagePropertyBindingSource::CatalogueEntryPreview,
            )),
            "biomeicon" => Some(UiNodePropertyBinding::ImageContent(
                UiImagePropertyBindingSource::CatalogueEntryBiomeIcon,
            )),
            "locationmap" => Some(UiNodePropertyBinding::ImageContent(
                UiImagePropertyBindingSource::CatalogueEntryLocationIcon,
            )),
            _ => None,
        };
        if let Some(binding) = binding {
            output.bindings.push(binding);
        }
        let visibility_binding = match normalized_name.as_str() {
            "researchobjectpanel" | "researchbump" => {
                Some(UiBooleanPropertyBindingSource::CatalogueSelectedEntryRequiresResearch)
            }
            "availableobjectpanel" => {
                Some(UiBooleanPropertyBindingSource::CatalogueSelectedEntryAvailable)
            }
            "upkeeppanel" => {
                Some(UiBooleanPropertyBindingSource::CatalogueSelectedEntryHasMonthlyUpkeep)
            }
            "biome" => {
                Some(UiBooleanPropertyBindingSource::CatalogueSelectedEntryHasBiomeAndLocation)
            }
            "biomelocationendangerment" | "gender" | "sortbutton" => {
                Some(UiBooleanPropertyBindingSource::CatalogueSelectedEntryIsAnimal)
            }
            "rotate" => {
                Some(UiBooleanPropertyBindingSource::CatalogueSelectedEntryPlacementCanRotate)
            }
            "itemssold" => Some(UiBooleanPropertyBindingSource::CatalogueSelectedEntrySellsItems),
            _ => None,
        };
        if let Some(visibility_binding) = visibility_binding {
            output
                .bindings
                .push(UiNodePropertyBinding::Visibility(visibility_binding));
        }
    }
    if role == UiDocumentRole::EntityInfo {
        let binding = match normalized_name.as_str() {
            "entitynamelabel"
                if matches!(
                    &node.widget,
                    SourceUiWidgetData::Text(_) | SourceUiWidgetData::TextEdit { .. }
                ) =>
            {
                Some(UiNodePropertyBinding::TextContent(
                    UiTextPropertyBindingSource::SelectedEntityName,
                ))
            }
            "entityimagedisplay" => Some(UiNodePropertyBinding::ImageContent(
                UiImagePropertyBindingSource::SelectedEntityIcon,
            )),
            _ => None,
        };
        if let Some(binding) = binding {
            output.bindings.push(binding);
        }
    }
    if role == UiDocumentRole::PhotoAlbum {
        let text_binding = match normalized_name.as_str() {
            "leftpagenumber" => {
                Some(UiTextPropertyBindingSource::PhotoAlbumPageNumber { offset: 1 })
            }
            "rightpagenumber" => {
                Some(UiTextPropertyBindingSource::PhotoAlbumPageNumber { offset: 2 })
            }
            "photoalbumselectiontext2" => Some(UiTextPropertyBindingSource::PhotoAlbumName),
            "filmcount" => Some(UiTextPropertyBindingSource::PhotoFilmCount),
            _ => None,
        };
        if let Some(binding) = text_binding {
            output
                .bindings
                .push(UiNodePropertyBinding::TextContent(binding));
        }
        if let Some(index) = node
            .name
            .as_deref()
            .map(str::trim)
            .and_then(|name| name.strip_prefix("thumb"))
            .and_then(|index| index.parse::<u8>().ok())
        {
            output.bindings.push(UiNodePropertyBinding::ImageContent(
                UiImagePropertyBindingSource::PhotoAlbumPicture { index },
            ));
        }
    }
    if role == UiDocumentRole::PhotoMode && normalized_name == "photocounter" {
        output.bindings.push(UiNodePropertyBinding::TextContent(
            UiTextPropertyBindingSource::PhotoFilmCount,
        ));
    }
    if matches!(role, UiDocumentRole::MapSelect | UiDocumentRole::Globe)
        && normalized_name == "mapmodeheading"
    {
        output.bindings.push(UiNodePropertyBinding::TextContent(
            UiTextPropertyBindingSource::PlayModeHeading,
        ));
    }
    let money_format = match &node.widget {
        SourceUiWidgetData::Text(text) => text
            .text_format
            .as_deref()
            .filter(|format| format.starts_with("openzt2:money:")),
        _ => None,
    };
    if let Some(format) = money_format.filter(|_| !information_list_row_has_typed_value_binding) {
        let source = if role == UiDocumentRole::ZooStatus
            && matches!(
                normalized_name.as_str(),
                "educationamount"
                    | "allspeciesamount"
                    | "alltoursamount"
                    | "showdonationamount"
                    | "combinedamount"
            ) {
            UiTextPropertyBindingSource::ZooDonationSummaryDonationAmount {
                category: match normalized_name.as_str() {
                    "educationamount" => Some(AssetId::from_key("education-donation")),
                    "allspeciesamount" => Some(AssetId::from_key("animal-donation")),
                    "alltoursamount" => Some(AssetId::from_key("tour-donation")),
                    "showdonationamount" => Some(AssetId::from_key("show-donation")),
                    "combinedamount" => None,
                    _ => unreachable!("the enclosing exact-name match is exhaustive"),
                },
                positive_format: resolve_authored_money_format_localization_key(format, "positive"),
            }
        } else if format.contains("positive=entityinfo:poscursorformat")
            && format.contains("negative=entityinfo:negcursorformat")
        {
            UiTextPropertyBindingSource::ConstructionPreviewCost {
                positive_format: resolve_authored_money_format_localization_key(format, "positive"),
                negative_format: resolve_authored_money_format_localization_key(format, "negative"),
                no_cents: parse_authored_money_format_boolean_option(format, "no-cents"),
                no_minus: parse_authored_money_format_boolean_option(format, "no-minus"),
            }
        } else if role == UiDocumentRole::PurchaseCatalogue && normalized_name == "costdisplay" {
            let format_key = resolve_authored_money_format_localization_key(format, "positive");
            let omit_fractional_currency_cents =
                parse_authored_money_format_boolean_option(format, "no-cents");
            if research_cost {
                UiTextPropertyBindingSource::CatalogueEntryResearchPrice {
                    format: format_key,
                    omit_fractional_currency_cents,
                }
            } else {
                UiTextPropertyBindingSource::CatalogueEntryPrice {
                    row: 0,
                    format: format_key,
                    omit_fractional_currency_cents,
                }
            }
        } else if role == UiDocumentRole::PurchaseCatalogue && normalized_name == "upkeepdisplay" {
            UiTextPropertyBindingSource::CatalogueEntryUpkeep {
                row: 0,
                format: resolve_authored_money_format_localization_key(format, "positive"),
                omit_fractional_currency_cents: parse_authored_money_format_boolean_option(
                    format, "no-cents",
                ),
            }
        } else if normalized_name == "sellitemrefund" {
            UiTextPropertyBindingSource::SelectedEntitySaleRefund {
                format: resolve_authored_money_format_localization_key(format, "positive"),
                omit_fractional_currency_cents: parse_authored_money_format_boolean_option(
                    format, "no-cents",
                ),
            }
        } else if normalized_name == "admissionpricedisplay" {
            UiTextPropertyBindingSource::ZooAdmissionPrice
        } else {
            UiTextPropertyBindingSource::ZooCash {
                positive_format: resolve_authored_money_format_localization_key(format, "positive"),
                negative_format: resolve_authored_money_format_localization_key(format, "negative"),
            }
        };
        // A money presentation format does not identify the account it displays.
        // Only the shipped zoo-balance control belongs to the zoo cash presenter.
        if !matches!(source, UiTextPropertyBindingSource::ZooCash { .. })
            || matches!(
                normalized_name.as_str(),
                "zoobucksdisplay" | "zoobucksstaticdisplay" | "zoobucksmovingdisplay"
            )
        {
            output
                .bindings
                .push(UiNodePropertyBinding::TextContent(source));
        }
    }
    if matches!(
        &node.widget,
        SourceUiWidgetData::Text(text)
            if text.text_format.as_deref() == Some("shell:currentdatedisplay")
    ) {
        output.bindings.push(UiNodePropertyBinding::TextContent(
            if role == UiDocumentRole::MainMenu {
                UiTextPropertyBindingSource::ApplicationVersion
            } else {
                UiTextPropertyBindingSource::ZooDate
            },
        ));
    }
    if matches!(
        &node.widget,
        SourceUiWidgetData::Text(text)
            if matches!(
                text.text_format.as_deref(),
                Some("mainmenu:profile_name" | "shell:profilenameformat")
            )
    ) {
        output.bindings.push(UiNodePropertyBinding::TextContent(
            UiTextPropertyBindingSource::ProfileName,
        ));
    }
    if normalized_name == "sellitemlabel" {
        output.bindings.push(UiNodePropertyBinding::TextContent(
            UiTextPropertyBindingSource::SelectedEntitySaleConfirmation,
        ));
    }
    if let SourceUiWidgetData::Text(text) = &node.widget {
        if normalized_name == "scorenumber"
            && text.text_format.as_deref() == Some("training:score_number_format")
        {
            output.bindings.push(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::TrainingAttemptScore {
                    format: lower_optional_authored_semantic_key_to_asset_id(
                        text.text_format.as_deref(),
                    ),
                },
            ));
        }
    }
    if normalized_name == "trainingscorebar"
        && matches!(&node.widget, SourceUiWidgetData::Slider(_))
    {
        output.bindings.push(UiNodePropertyBinding::IntegerValue(
            UiIntegerPropertyBindingSource::TrainingAttemptScoreMilli,
        ));
    }
    if normalized_name == "trainingfailedlayout"
        && matches!(&node.widget, SourceUiWidgetData::Layout(_))
    {
        output.bindings.push(UiNodePropertyBinding::Visibility(
            UiBooleanPropertyBindingSource::TrainingAttemptRejected,
        ));
    }
    if node
        .fields
        .iter()
        .any(authored_ui_field_binds_profile_presence)
    {
        output
            .bindings
            .push(UiNodePropertyBinding::InteractionEnabled(
                UiBooleanPropertyBindingSource::ProfilePresent,
            ));
    }
    let exact_live_value = match normalized_name.as_str() {
        // These are exact identities in the shipped entity-info and donation
        // row documents. live lowering binds them once; runtime never looks up a
        // named control or infers a field from text.
        "currentupkeep" | "upkeep" | "upkeepcost" => {
            Some(UiIntegerPropertyBindingSource::FacilityUpkeepCentsPerMonth)
        }
        "donationnumber" => Some(UiIntegerPropertyBindingSource::DonationCount),
        "donationamount" => Some(UiIntegerPropertyBindingSource::DonationTotalCents),
        "showdonationquantity" => Some(UiIntegerPropertyBindingSource::DonationCount),
        "donationtotalamount" => Some(UiIntegerPropertyBindingSource::DonationTotalCents),
        "averagedonationamount" => Some(UiIntegerPropertyBindingSource::DonationAverageCents),
        "currentusers" => Some(UiIntegerPropertyBindingSource::FacilityCapacityUsed),
        "guestcashdisplay" => Some(UiIntegerPropertyBindingSource::GuestCashCents),
        "salary" => Some(UiIntegerPropertyBindingSource::StaffWageCentsPerDay),
        "monthsemployed" => Some(UiIntegerPropertyBindingSource::StaffMonthsEmployed),
        "totalbirths" => Some(UiIntegerPropertyBindingSource::EndangeredBirthCount),
        "buypanelconservationimagedisplay" if role == UiDocumentRole::PurchaseCatalogue => {
            Some(UiIntegerPropertyBindingSource::CatalogueEntryConservationStatus)
        }
        "conservationimagedisplay" if role == UiDocumentRole::EntityInfo => {
            Some(UiIntegerPropertyBindingSource::AnimalConservationStatus)
        }
        // Exact shipped UIMultiIcon in `UI/layout/entityinfo.xml`; its four
        // entries are already lowered by UI lowerer and information supplies only the live
        // index derived from the canonical waste container.
        "trashquantityimagedisplay" => Some(UiIntegerPropertyBindingSource::FacilityTrashLevel),
        _ => None,
    };
    if let Some(source) = exact_live_value {
        output
            .bindings
            .push(UiNodePropertyBinding::IntegerValue(source));
    }
    let worker_duty = (role == UiDocumentRole::EntityInfo)
        .then_some(node.name.as_deref())
        .flatten()
        .and_then(|name| {
            [
                (
                    "clean_filter_button",
                    UiBooleanPropertyBindingSource::WorkerCleansFilters,
                ),
                (
                    "clean_recycle_button",
                    UiBooleanPropertyBindingSource::WorkerCleansRecycling,
                ),
                (
                    "empty_trash_button",
                    UiBooleanPropertyBindingSource::WorkerEmptiesTrash,
                ),
                (
                    "sweep_trash_button",
                    UiBooleanPropertyBindingSource::WorkerSweepsTrash,
                ),
            ]
            .into_iter()
            .find_map(|(button, source)| name.eq_ignore_ascii_case(button).then_some(source))
        });
    if let Some(source) = worker_duty {
        output
            .bindings
            .push(UiNodePropertyBinding::SelectionState(source));
    }
    if role == UiDocumentRole::EntityInfo {
        let bindings: &[UiNodePropertyBinding] = match normalized_name.as_str() {
            "pickup" => &[
                UiNodePropertyBinding::Visibility(
                    UiBooleanPropertyBindingSource::AnimalPickupVisible,
                ),
                UiNodePropertyBinding::InteractionEnabled(
                    UiBooleanPropertyBindingSource::AnimalPickupEnabled,
                ),
            ],
            "sell" => &[UiNodePropertyBinding::Visibility(
                UiBooleanPropertyBindingSource::AnimalReleaseVisible,
            )],
            // Compile the exact `Uncrate` child identity into the
            // native binding; runtime presentation then needs no named-control
            // lookup or token synchronization pass.
            "uncrate" => &[UiNodePropertyBinding::Visibility(
                UiBooleanPropertyBindingSource::SelectedEntityCrated,
            )],
            _ => &[],
        };
        bindings
            .iter()
            .for_each(|binding| output.bindings.push(binding.clone()));
        if let Some(section) = [
            "Animal Info",
            "Guest Info",
            "Enrichment Info",
            "Food Info",
            "Tree Info",
            "Plant Info",
            "Rocks Info",
            "Scenery Info",
            "Commerce Building Info",
            "Building Info",
            "Dino Recovery Building Info",
            "Breeding Center Info",
            "Donation Box Info",
            "Trash Info",
            "Keeper Info",
            "MW Info",
            "Educator Info",
            "Path Info",
            "Crate Info",
            "Egg Info",
            "Station Info",
            "Vehicle Info",
            "Crated entity Info",
            "Show Info",
            "Trainer Info",
            "Tank Filter Info",
            "MC Info",
            "Training Area Info",
            "Entertainer Info",
            "Cloning Center Info",
            "Fossil Education Center Info",
            "Staff Info",
        ]
        .into_iter()
        .find(|name| {
            node.name
                .as_deref()
                .is_some_and(|source| source.eq_ignore_ascii_case(name))
        })
        .map(AssetId::from_key)
        {
            output.bindings.push(UiNodePropertyBinding::Visibility(
                UiBooleanPropertyBindingSource::SelectedInformationPanelSubjectUsesSection {
                    section,
                },
            ));
        }
        if normalized_name == "conservationtext" {
            output.bindings.push(UiNodePropertyBinding::TextContent(
                UiTextPropertyBindingSource::AnimalConservationStatus,
            ));
        }
    }
}

pub(super) fn resolve_authored_money_format_localization_key(format: &str, field: &str) -> AssetId {
    format
        .split(';')
        .find_map(|part| {
            part.strip_prefix(field)
                .and_then(|value| value.strip_prefix('='))
        })
        .filter(|value| !value.is_empty())
        .map_or_else(AssetId::default, AssetId::from_key)
}

pub(super) fn parse_authored_money_format_boolean_option(format: &str, field: &str) -> bool {
    format
        .strip_prefix("openzt2:money:")
        .unwrap_or(format)
        .split(';')
        .find_map(|part| {
            part.strip_prefix(field)
                .and_then(|value| value.strip_prefix('='))
        })
        == Some("1")
}

pub(super) fn authored_ui_field_binds_profile_presence(field: &SourceUiField) -> bool {
    field.name.as_deref() == Some("enabled")
        && field.type_name.as_deref() == Some("bool")
        && field.format.as_deref() == Some("openzt2:profile-present")
        && field.unknown_attributes.is_empty()
}
