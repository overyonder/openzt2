use crate::source_document::ui::model::{SourceUiEvent, SourceUiNode};
use crate::ui_document::source::lower::authored_ui_action_argument_lowering::{
    cross_document_role, target_node, trigger,
};
use crate::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::ui_document::source::lower::authored_ui_event_action_lowering;
use crate::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::action::information::{
    InformationGraphSeries, InformationGraphType, InformationListCategory,
    InformationSettingAction, InformationViewCategory,
};
use openzt2_game_data::ui_document::action::scenarios::ObjectiveStatusFilter;
use openzt2_game_data::ui_document::action::shell_navigation::{
    UiShellAction, UiShellActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_events(
    node: &SourceUiNode,
    role: UiDocumentRole,
    current: AssetId,
    current_stable_name: &str,
    scroll_receiver: AssetId,
    tree_element: Option<AssetId>,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
    confirmation_scope: Option<&str>,
    node_index: u32,
) -> io::Result<()> {
    for block in &node.events {
        let photo_safari_transition_owns_legacy_first_person_mode_setup = block
            .events
            .iter()
            .any(source_event_enters_photo_safari_mode);
        let returns_from_map_selection =
            matches!(role, UiDocumentRole::Globe | UiDocumentRole::MapSelect)
                && block
                    .events
                    .iter()
                    .any(source_event_clears_scenario_selection)
                && block.events.iter().any(source_event_shows_main_menu_layout);
        let returns_from_options = role == UiDocumentRole::Options
            && block.events.iter().any(|event| {
                event.message == "ZT_EVENT" && event.string.as_deref() == Some("back")
            })
            && block.events.iter().any(source_event_shows_main_menu_layout);
        let shown_by_shell = block
            .events
            .iter()
            .filter_map(shell_transition_destination)
            .collect::<Vec<_>>();
        let mut returned_to_main_menu = false;
        for event in &block.events {
            if photo_safari_transition_owns_legacy_first_person_mode_setup
                && source_event_enters_first_person_mode(event)
            {
                continue;
            }
            if (returns_from_options || returns_from_map_selection)
                && source_event_shows_main_menu_layout(event)
            {
                if !returned_to_main_menu {
                    output
                        .actions
                        .push(UiActionRecord::Shell(UiShellActionRecord {
                            trigger: trigger(&block.trigger),
                            action: if returns_from_options {
                                UiShellAction::NavigateBackFromOptions
                            } else {
                                UiShellAction::NavigateBackFromMapSelection
                            },
                        }));
                    returned_to_main_menu = true;
                }
                continue;
            }
            if returns_from_options && source_event_hides_its_receiver(event) {
                continue;
            }
            let first = output.actions.len();
            let receiver = flatten_event(
                trigger(&block.trigger),
                event,
                role,
                current,
                current_stable_name,
                scroll_receiver,
                tree_element,
                output,
                input,
                confirmation_scope,
                &shown_by_shell,
            )?;
            if output.actions.len() > first && !receiver.nodes.is_empty() {
                output
                    .action_receivers
                    .insert((node_index, first), receiver);
            }
        }
    }
    Ok(())
}

fn source_event_enters_photo_safari_mode(event: &SourceUiEvent) -> bool {
    source_event_sets_named_application_mode(event, "mode_photo_safari")
}

fn source_event_enters_first_person_mode(event: &SourceUiEvent) -> bool {
    source_event_sets_named_application_mode(event, "mode_first_person")
}

fn source_event_sets_named_application_mode(event: &SourceUiEvent, mode: &str) -> bool {
    (event.message == "ZT_SETMODE"
        && event
            .string
            .as_deref()
            .or(event.value.as_deref())
            .is_some_and(|value| value == mode))
        || event
            .child
            .as_deref()
            .is_some_and(|child| source_event_sets_named_application_mode(child, mode))
}

fn source_event_clears_scenario_selection(event: &SourceUiEvent) -> bool {
    event.message == "BFS_CLEARSCENARIO"
        || event
            .child
            .as_deref()
            .is_some_and(source_event_clears_scenario_selection)
}

fn source_event_hides_its_receiver(event: &SourceUiEvent) -> bool {
    event.message == "UI_HIDE" && event.target_child.is_none() && event.string.is_none()
}

fn source_event_shows_main_menu_layout(event: &SourceUiEvent) -> bool {
    event.message == "UI_CHILD"
        && event.target_child.as_deref() == Some("Main Menu Layout")
        && event
            .child
            .as_deref()
            .is_some_and(|child| child.message == "UI_SHOW")
}

#[derive(Default)]
pub(super) struct UiActionReceiverPath {
    pub(super) from_root: bool,
    pub(super) nodes: Vec<AssetId>,
}

pub(super) fn flatten_event<'a>(
    trigger: UiTrigger,
    event: &'a SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
    current_stable_name: &str,
    scroll_receiver: AssetId,
    tree_element: Option<AssetId>,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
    mut confirmation_scope: Option<&'a str>,
    shown_by_shell: &[UiDocumentRole],
) -> io::Result<UiActionReceiverPath> {
    use openzt2_game_data::ui_document::action::presentation::{
        UiPresentationAction, UiPresentationActionRecord,
    };
    let mut receiver = UiActionReceiverPath::default();
    let mut event = event;
    let mut foreign = None;
    while let Some(child) = event.child.as_deref().filter(|_| {
        matches!(
            event.message.as_str(),
            "UI_CHILD" | "UI_ROOT_CHILD" | "UI_SELF"
        )
    }) {
        if event.message == "UI_ROOT_CHILD" {
            receiver.from_root = true;
            receiver.nodes.clear();
            foreign = None;
        }
        if matches!(event.message.as_str(), "UI_CHILD" | "UI_ROOT_CHILD") {
            if let Some(target) = event.target_child.as_deref() {
                confirmation_scope = Some(target);
                foreign = foreign.or_else(|| cross_document_role(event, input));
                receiver.nodes.push(UiDocumentRole::node_id(role, target));
            }
        }
        event = child;
    }
    if matches!(event.message.as_str(), "UI_SHOW" | "UI_HIDE") {
        if let Some(document_role) = foreign.or_else(|| cross_document_role(event, input)) {
            let target_name = event.target_child.as_deref().or(event.string.as_deref());
            let shell_entry =
                match document_role {
                    UiDocumentRole::Options => target_name
                        .is_some_and(|target| target.eq_ignore_ascii_case("Options Layout")),
                    UiDocumentRole::Downloads => target_name
                        .is_some_and(|target| target.eq_ignore_ascii_case("Download Layout")),
                    UiDocumentRole::Globe | UiDocumentRole::MapSelect => {
                        target_name.is_some_and(|target| {
                            target.eq_ignore_ascii_case("Freeform Selection Layout")
                                || target.eq_ignore_ascii_case("Map Selection Layout")
                        })
                    }
                    _ => false,
                };
            if event.message != "UI_SHOW"
                || !(shell_entry && shown_by_shell.contains(&document_role))
            {
                output
                    .actions
                    .push(UiActionRecord::Presentation(UiPresentationActionRecord {
                        trigger,
                        action: UiPresentationAction::SetDocumentNodeVisible {
                            document_role,
                            target_node: target_node(event, document_role, current),
                            visible: event.message == "UI_SHOW",
                        },
                    }));
            }
            return Ok(receiver);
        }
    }
    let confirmation_scope = event.target_child.as_deref().or(confirmation_scope);
    let target = if matches!(event.message.as_str(), "UI_EXPAND" | "UI_COLLAPSE") {
        tree_element.unwrap_or(current)
    } else {
        receiver.nodes.last().copied().unwrap_or(current)
    };
    if let Some(action) =
        authored_ui_event_action_lowering::lower_authored_ui_event_to_canonical_action(
            trigger,
            event,
            role,
            target,
            current_stable_name,
            scroll_receiver,
            output,
            input,
            confirmation_scope,
        )?
    {
        output.actions.push(action);
    }
    Ok(receiver)
}

fn shell_transition_destination(event: &SourceUiEvent) -> Option<UiDocumentRole> {
    let value = event
        .string
        .as_deref()
        .or(event.value.as_deref())
        .unwrap_or_default();
    match event.message.as_str() {
        "OPEN_OPTIONS" => Some(UiDocumentRole::Options),
        "OPEN_DOWNLOADS" => Some(UiDocumentRole::Downloads),
        "OPEN_SAVED_GAMES" => Some(UiDocumentRole::SavedGames),
        "START_FREEFORM" | "OPEN_CHALLENGE" => Some(UiDocumentRole::Globe),
        "OPEN_CAMPAIGN" => Some(UiDocumentRole::MapSelect),
        "ZT_SET_GAME_MODE" => match value.to_ascii_lowercase().as_str() {
            "freeform" | "config/freeform_mode.xml" | "challenge" | "config/challenge_mode.xml" => {
                Some(UiDocumentRole::Globe)
            }
            "campaign" | "config/campaign_mode.xml" => Some(UiDocumentRole::MapSelect),
            _ => None,
        },
        "ZT_SETMODE" => match value {
            "mode_options" => Some(UiDocumentRole::Options),
            "mode_download" => Some(UiDocumentRole::Downloads),
            _ => None,
        },
        _ => event
            .child
            .as_deref()
            .and_then(shell_transition_destination),
    }
}

pub(super) fn closed_information_value<T>(
    value: Option<&str>,
    input: &AuthoredUiDocument,
    kind: &str,
    parse: impl FnOnce(&str) -> Option<T>,
) -> io::Result<T> {
    let value = value.unwrap_or_default();
    parse(value)
        .ok_or_else(|| invalid_at(input, format!("unsupported information {kind}: {value:?}")))
}

pub(super) fn information_view_category(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<InformationViewCategory> {
    closed_information_value(value, input, "view category", |value| {
        match value.to_ascii_lowercase().as_str() {
            "animal" => Some(InformationViewCategory::Animals),
            "guest" => Some(InformationViewCategory::Guests),
            "building" => Some(InformationViewCategory::Buildings),
            "entrance" => Some(InformationViewCategory::Entrances),
            "fence" => Some(InformationViewCategory::Fences),
            "curb" => Some(InformationViewCategory::Curbs),
            "zoowall" => Some(InformationViewCategory::ZooWalls),
            "foliage" => Some(InformationViewCategory::Foliage),
            "showplatform" => Some(InformationViewCategory::Shows),
            "tankplatform" | "tankwallsegment" | "tankgate" => Some(InformationViewCategory::Tanks),
            _ => None,
        }
    })
}

pub(super) fn information_list_category(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<InformationListCategory> {
    closed_information_value(value, input, "list category", |value| {
        match value.to_ascii_lowercase().as_str() {
            "animal" => Some(InformationListCategory::Animals),
            "guest" => Some(InformationListCategory::Guests),
            "staff" => Some(InformationListCategory::Staff),
            "building" => Some(InformationListCategory::Buildings),
            "donationbox" => Some(InformationListCategory::DonationBoxes),
            "vehicle" => Some(InformationListCategory::Vehicles),
            _ => None,
        }
    })
}

pub(super) fn objective_status_filter(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<ObjectiveStatusFilter> {
    closed_information_value(value, input, "status filter", |value| {
        match value.to_ascii_lowercase().as_str() {
            "all" => Some(ObjectiveStatusFilter::All),
            "success" => Some(ObjectiveStatusFilter::Success),
            "failure" => Some(ObjectiveStatusFilter::Failure),
            "neutral" => Some(ObjectiveStatusFilter::Neutral),
            _ => None,
        }
    })
}

pub(super) fn information_setting(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<InformationSettingAction> {
    closed_information_value(value, input, "setting action", |value| {
        match value.to_ascii_lowercase().as_str() {
            "" => Some(InformationSettingAction::Refresh),
            "highest detail" => Some(InformationSettingAction::HighestDetail),
            "high detail" => Some(InformationSettingAction::HighDetail),
            "medium detail" => Some(InformationSettingAction::MediumDetail),
            "custom detail" => Some(InformationSettingAction::CustomDetail),
            "fullscreen" => Some(InformationSettingAction::Fullscreen),
            "windowed" => Some(InformationSettingAction::Windowed),
            "freemouselookon" => Some(InformationSettingAction::FreeMouseLook(true)),
            "freemouselookoff" => Some(InformationSettingAction::FreeMouseLook(false)),
            "motdenabled" => Some(InformationSettingAction::MessageOfTheDay(true)),
            "motddisabled" => Some(InformationSettingAction::MessageOfTheDay(false)),
            "accept settings" => Some(InformationSettingAction::Accept),
            "back" => Some(InformationSettingAction::Back),
            _ => None,
        }
    })
}

pub(super) fn information_graph_type(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<InformationGraphType> {
    closed_information_value(value, input, "graph type", |value| {
        match value.to_ascii_lowercase().as_str() {
            "bar" => Some(InformationGraphType::Bar),
            "line" => Some(InformationGraphType::Line),
            _ => None,
        }
    })
}

pub(super) fn authored_graph_type(value: Option<&str>) -> Option<InformationGraphType> {
    match value {
        None => Some(InformationGraphType::Line),
        Some("bar") => Some(InformationGraphType::Bar),
        Some("line") => Some(InformationGraphType::Line),
        Some(_) => None,
    }
}

pub(super) fn information_graph_series(
    value: Option<&str>,
    input: &AuthoredUiDocument,
) -> io::Result<InformationGraphSeries> {
    closed_information_value(value, input, "graph series", |value| match value {
        "value Zoo ui/layout/moneygraphlabel.xml" => Some(InformationGraphSeries::ZooValue),
        "profit Zoo ui/layout/moneygraphlabel.xml" => Some(InformationGraphSeries::ZooProfit),
        "profit donations_income ui/layout/moneygraphlabel.xml" => {
            Some(InformationGraphSeries::DonationIncome)
        }
        "totalUsers admissions ui/layout/graphlabel.xml" => {
            Some(InformationGraphSeries::AdmissionUsers)
        }
        "fame fame ui/layout/graphlabel.xml" => Some(InformationGraphSeries::Fame),
        "members members ui/layout/graphlabel.xml" => Some(InformationGraphSeries::Members),
        _ => None,
    })
}
