use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::{
    information_sort_field, target_node,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_event_collection_lowering::{
    information_graph_series, information_graph_type, information_setting,
    information_view_category,
};
use crate::assets::ui_document::source::lower::canonical_source_value_resolution;
use openzt2_game_data::ui_document::action::information::{
    InformationListCriterion, UiInformationAction, UiInformationActionRecord,
};
use openzt2_game_data::ui_document::action::presentation::{
    UiPresentationAction, UiPresentationActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_information_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_VIEWFILTER_HIDE" | "ZT_VIEWFILTER_SHOW" if event.string.as_deref().is_some_and(|value| value.eq_ignore_ascii_case("emote")) => {
            Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
                trigger,
                action: UiPresentationAction::SetAllEmotePresentationsVisible {
                    visible: event.message == "ZT_VIEWFILTER_SHOW",
                },
            }))
        }
        "ZT_EVENT"
            if event
                .string
                .as_deref()
                .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "use8bitsoundson" | "use8bitsoundsoff")) =>
        {
            Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
                trigger,
                action: UiPresentationAction::SetTargetNodeInteractionEnabled { target_node: current, enabled: false },
            }))
        }
        "ZT_SELECTNEXT_DISEASED_ANIMAL" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SelectNextDiseasedAnimal,
        })),
        "ZT_SELECTNEXT_RAMPAGING_ANIMAL" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SelectNextRampagingAnimal,
        })),
        "ZT_SET_SELECTED_ENTITY" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SelectEntityFromSource,
        })),
        "ZT_VIEWFILTER_HIDE" | "ZT_VIEWFILTER_SHOW" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SetViewFilter {
                category: information_view_category(event.string.as_deref(), input)?,
                visible: event.message == "ZT_VIEWFILTER_SHOW",
            },
        })),
        "ZT_EXPORT_OVERVIEW_MAP" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::ExportOverviewMap {
                return_control: UiDocumentRole::node_id(role, "return"),
            },
        })),
        "ZT_EVENT" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::ApplySetting {
                setting: information_setting(event.string.as_deref(), input)?,
            },
        })),
        "UI_HYPERLINK" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::OpenEncyclopediaEntry {
                entry: canonical_source_value_resolution::lower_authored_encyclopedia_entry_reference_to_asset_id(event.string.as_deref()),
            },
        })),
        "ZT_FIND_ANIMAL_ON_PANEL" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SelectEntityFromSource,
        })),
        "ZT_MULTILIST_SORT_BY_TYPE" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SortEntityList {
                list: target_node(event, role, current),
                criterion: InformationListCriterion::Type,
            },
        })),
        "ZT_ZOOPEDIA_BACK" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::ZoopediaBack,
        })),
        "ZT_MULTILIST_SORT_BY_NEED_DOWN" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SortEntityListByNeed {
                list: target_node(event, role, current),
                descending: true,
            },
        })),
        "ZT_ZOOPEDIA_FORWARD" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::ZoopediaForward,
        })),
        "ZT_MULTILIST_SORT_BY_PREGNANCY_DOWN" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SortAnimalsByPregnancy {
                list: target_node(event, role, current),
                descending: true,
            },
        })),
        "ZT_GRAPHTYPE_SELECTION" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SelectGraphType {
                graph_type: information_graph_type(event.string.as_deref().or(event.value.as_deref()), input)?,
            },
        })),
        "ZT_MULTILIST_SORT_BY_FAVORITE_ANIMAL" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SortGuestsByFavouriteAnimal {
                list: target_node(event, role, current),
            },
        })),
        "ZT_SET_RESEARCH_FILTER" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SetResearchFilter {
                list: target_node(event, role, current),
                unlocked_only: event.string.as_deref().or(event.value.as_deref()) == Some("unlocked"),
            },
        })),
        "ZT_SET_BIOME_FILTER" | "ZT_SET_THEME_FILTER" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SetCatalogueKindFilter {
                list: target_node(event, role, current),
                kind: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(event.string.as_deref().or(event.value.as_deref())),
            },
        })),
        "ZT_SET_NO_FILTER" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::ClearTypeFilter {
                list: target_node(event, role, current),
            },
        })),
        "ZT_MULTILIST_SORT_BY_INT_GREATER" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SortEntityListDirected {
                list: target_node(event, role, current),
                field: information_sort_field(event, input)?,
                descending: true,
            },
        })),
        "ZT_MULTILIST_SORT_BY_STRING_REVERSE" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SortEntityListDirected {
                list: target_node(event, role, current),
                field: information_sort_field(event, input)?,
                descending: true,
            },
        })),
        "ZT_MULTILIST_SORT_BY_STRING" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SortEntityList {
                list: target_node(event, role, current),
                criterion: InformationListCriterion::Name,
            },
        })),
        "ZT_MULTILIST_SORT_BY_ANIMAL_HAPPINESS_UP" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SortAnimalsByHappiness {
                list: target_node(event, role, current),
                descending: false,
            },
        })),
        "ZT_GRAPH_SELECTION" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::SelectGraph {
                graph: information_graph_series(event.string.as_deref().or(event.value.as_deref()), input)?,
            },
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}
