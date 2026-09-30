use crate::assets::source_document::ui::model::{SourceUiEvent, SourceUiNode};
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::{
    cross_document_role, trigger,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_event_action_lowering;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
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
        if returns_from_options {
            output
                .actions
                .push(UiActionRecord::Shell(UiShellActionRecord {
                    trigger: trigger(&block.trigger),
                    action: UiShellAction::NavigateBackFromOptions,
                }));
        }
        let shell_owns_role_transition = returns_from_map_selection
            || returns_from_options
            || block.events.iter().any(source_event_enters_shell_role);
        if returns_from_map_selection {
            output
                .actions
                .push(UiActionRecord::Shell(UiShellActionRecord {
                    trigger: trigger(&block.trigger),
                    action: UiShellAction::NavigateBackFromMapSelection,
                }));
        }
        let event_scope = block
            .events
            .iter()
            .find_map(|event| {
                (event.message == "UI_CHILD")
                    .then_some(event.target_child.as_deref())
                    .flatten()
            })
            .or(confirmation_scope);
        for event in &block.events {
            if photo_safari_transition_owns_legacy_first_person_mode_setup
                && source_event_enters_first_person_mode(event)
            {
                continue;
            }
            // The shell replaces the whole options screen with the main menu.
            // Back and Apply's receiver-less UI_HIDE would otherwise hide only
            // the pressed button, leaving the screen open without it.
            if returns_from_options
                && (source_event_hides_its_receiver(event)
                    || source_event_shows_main_menu_layout(event))
            {
                continue;
            }
            flatten_event(
                trigger(&block.trigger),
                event,
                role,
                current,
                current_stable_name,
                scroll_receiver,
                tree_element,
                output,
                input,
                event_scope,
                shell_owns_role_transition,
            )?;
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

pub(super) fn flatten_event(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
    current_stable_name: &str,
    scroll_receiver: AssetId,
    tree_element: Option<AssetId>,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
    scope: Option<&str>,
    shell_owns_role_transition: bool,
) -> io::Result<()> {
    if event.message == "UI_CHILD" && event.child.is_some() {
        let child = inherited_child(event, input)?;
        return flatten_event(
            trigger,
            &child,
            role,
            current,
            current_stable_name,
            scroll_receiver,
            tree_element,
            output,
            input,
            scope,
            shell_owns_role_transition,
        );
    }
    if shell_owns_role_transition
        && matches!(event.message.as_str(), "UI_SHOW" | "UI_HIDE")
        && cross_document_role(event, input).is_some()
    {
        // The shell already opens this screen; inline show/hide would open it twice.
        return Ok(());
    }
    let target = if matches!(event.message.as_str(), "UI_EXPAND" | "UI_COLLAPSE") {
        tree_element.unwrap_or(current)
    } else {
        current
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
            scope,
        )?
    {
        output.actions.push(action);
    }
    Ok(())
}

pub(super) fn source_event_enters_shell_role(event: &SourceUiEvent) -> bool {
    let direct_transition = matches!(
        event.message.as_str(),
        "ZT_SET_GAME_MODE"
            | "START_FREEFORM"
            | "OPEN_CHALLENGE"
            | "OPEN_CAMPAIGN"
            | "OPEN_OPTIONS"
            | "OPEN_DOWNLOADS"
            | "OPEN_SAVED_GAMES"
    );
    let set_mode_transition = event.message == "ZT_SETMODE"
        && matches!(
            event
                .string
                .as_deref()
                .or(event.value.as_deref())
                .unwrap_or_default(),
            "mode_options" | "mode_download"
        );
    direct_transition
        || set_mode_transition
        || event
            .child
            .as_deref()
            .is_some_and(source_event_enters_shell_role)
}

pub(super) fn inherited_child(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<SourceUiEvent> {
    let mut child = event
        .child
        .as_deref()
        .cloned()
        .ok_or_else(|| invalid_at(input, "UI_CHILD event has no typed child event"))?;
    if child.target_child.is_none() {
        child.target_child.clone_from(&event.target_child);
    }
    Ok(child)
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
