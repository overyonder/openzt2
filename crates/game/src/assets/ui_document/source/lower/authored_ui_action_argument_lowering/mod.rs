use crate::assets::source_document::ui::model::{
    SourceUiEvent, SourceUiEventTrigger, SourceUiNode,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, parse_bool,
};
use openzt2_game_data::ui_document::action::camera::{
    UiCameraPanDirection, UiCameraSignedAxisDirection,
};
use openzt2_game_data::ui_document::action::information::InformationSortField;
use openzt2_game_data::ui_document::action::staff_management::UiWorkerDuty;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn trigger(value: &SourceUiEventTrigger) -> UiTrigger {
    match value {
        SourceUiEventTrigger::Enter => UiTrigger::Enter,
        SourceUiEventTrigger::Leave => UiTrigger::Leave,
        SourceUiEventTrigger::Activate => UiTrigger::Press,
        SourceUiEventTrigger::AnimationCompleted => UiTrigger::AnimationCompleted,
        SourceUiEventTrigger::Show => UiTrigger::Show,
        SourceUiEventTrigger::Hide => UiTrigger::Hide,
        SourceUiEventTrigger::On => UiTrigger::On,
        SourceUiEventTrigger::Off => UiTrigger::Off,
        SourceUiEventTrigger::TextChanged => UiTrigger::Change,
        SourceUiEventTrigger::DoubleClick => UiTrigger::Submit,
    }
}

pub(super) fn target_node(
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
) -> AssetId {
    event
        .target_child
        .as_deref()
        .or(event.string.as_deref())
        .map(|target| UiDocumentRole::node_id(role, target))
        .unwrap_or(current)
}
pub(super) fn cross_document_role(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> Option<UiDocumentRole> {
    let target = event.target_child.as_deref().or(event.string.as_deref())?;
    (!document_contains_named_node(input, target))
        .then(|| role_target(event, input))
        .flatten()
        .filter(|role| *role != input.role)
}

pub(super) fn document_contains_named_node(input: &AuthoredUiDocument, target: &str) -> bool {
    fn contains(node: &SourceUiNode, target: &str) -> bool {
        node.name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(target))
            || node.children.iter().any(|child| contains(child, target))
    }

    contains(&input.source.root, target)
        || input
            .templates
            .values()
            .any(|template| contains(template, target))
}

pub(super) fn role_target(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> Option<UiDocumentRole> {
    event
        .target_child
        .as_deref()
        .or(event.string.as_deref())
        .map(|target| target.trim().to_ascii_lowercase())
        .and_then(|target| input.role_targets.get(&target).copied())
}
pub(super) fn event_i32(event: &SourceUiEvent, index: usize) -> i32 {
    event.rect[index]
        .as_deref()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}
pub(super) fn required_photo_slot(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<i32> {
    if event.data.as_deref() != Some("BFPoint") {
        return Err(invalid_at(
            input,
            format!("{} requires a BFPoint payload", event.message),
        ));
    }
    event.rect[0]
        .as_deref()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|slot| (0..8).contains(slot))
        .ok_or_else(|| {
            invalid_at(
                input,
                format!("{} has no BFPoint x slot in 0..8", event.message),
            )
        })
}
pub(super) fn camera_pan_direction(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<UiCameraPanDirection> {
    match event
        .string
        .as_deref()
        .or(event.value.as_deref())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("up" | "forward" | "north") => Ok(UiCameraPanDirection::North),
        Some("northeast") => Ok(UiCameraPanDirection::NorthEast),
        Some("right" | "east") => Ok(UiCameraPanDirection::East),
        Some("southeast") => Ok(UiCameraPanDirection::SouthEast),
        Some("down" | "back" | "south") => Ok(UiCameraPanDirection::South),
        Some("southwest") => Ok(UiCameraPanDirection::SouthWest),
        Some("left" | "west") => Ok(UiCameraPanDirection::West),
        Some("northwest") => Ok(UiCameraPanDirection::NorthWest),
        value => Err(invalid_at(
            input,
            format!("{} has unknown camera pan operand {value:?}", event.message),
        )),
    }
}
pub(super) fn camera_axis_direction(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<UiCameraSignedAxisDirection> {
    match event
        .string
        .as_deref()
        .or(event.value.as_deref())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("right" | "clockwise" | "in" | "up" | "positive" | "+" | "1") => {
            Ok(UiCameraSignedAxisDirection::Positive)
        }
        Some("left" | "counterclockwise" | "out" | "down" | "negative" | "-" | "-1") => {
            Ok(UiCameraSignedAxisDirection::Negative)
        }
        value => Err(invalid_at(
            input,
            format!(
                "{} has unknown camera axis operand {value:?}",
                event.message
            ),
        )),
    }
}
pub(super) fn required_u32(event: &SourceUiEvent, input: &AuthoredUiDocument) -> io::Result<u32> {
    event
        .value
        .as_deref()
        .or(event.string.as_deref())
        .or(event.data.as_deref())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            invalid_at(
                input,
                format!("{} command has no numeric slot", event.message),
            )
        })
}
pub(super) fn required_bool(event: &SourceUiEvent, input: &AuthoredUiDocument) -> io::Result<bool> {
    event
        .value
        .as_deref()
        .or(event.string.as_deref())
        .and_then(parse_bool)
        .ok_or_else(|| {
            invalid_at(
                input,
                format!("{} command has no bool payload", event.message),
            )
        })
}
pub(super) fn required_u8(event: &SourceUiEvent, input: &AuthoredUiDocument) -> io::Result<u8> {
    required_u32(event, input)?
        .try_into()
        .map_err(|_| invalid_at(input, format!("{} payload is outside u8", event.message)))
}
pub(super) fn required_i8(event: &SourceUiEvent, input: &AuthoredUiDocument) -> io::Result<i8> {
    event
        .value
        .as_deref()
        .or(event.string.as_deref())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            invalid_at(
                input,
                format!("{} command has no i8 payload", event.message),
            )
        })
}
pub(super) fn required_tenths(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<i32> {
    event
        .value
        .as_deref()
        .or(event.string.as_deref())
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .map(|value| (value * 10.0).round() as i32)
        .ok_or_else(|| {
            invalid_at(
                input,
                format!("{} command has no finite scalar payload", event.message),
            )
        })
}
pub(super) fn current_named(current: AssetId, role: UiDocumentRole, name: &str) -> bool {
    current == UiDocumentRole::node_id(role, name)
}
pub(super) fn worker_duty(
    current: AssetId,
    role: UiDocumentRole,
    input: &AuthoredUiDocument,
) -> io::Result<UiWorkerDuty> {
    [
        ("clean_filter_button", UiWorkerDuty::CleanAquaticTankFilter),
        (
            "clean_recycle_button",
            UiWorkerDuty::EmptyRecyclingContainer,
        ),
        ("empty_trash_button", UiWorkerDuty::EmptyTrashContainer),
        ("sweep_trash_button", UiWorkerDuty::SweepLitter),
    ]
    .into_iter()
    .find_map(|(name, duty)| current_named(current, role, name).then_some(duty))
    .ok_or_else(|| {
        invalid_at(
            input,
            "worker assignment button has no closed duty classification",
        )
    })
}
pub(super) fn price_index(
    current: AssetId,
    role: UiDocumentRole,
    input: &AuthoredUiDocument,
) -> io::Result<u8> {
    [("Cheap", 0), ("Moderate", 1), ("Expensive", 2)]
        .into_iter()
        .find_map(|(name, index)| current_named(current, role, name).then_some(index))
        .ok_or_else(|| invalid_at(input, "facility price button has no closed price band"))
}
pub(super) fn information_sort_field(
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<InformationSortField> {
    match event
        .string
        .as_deref()
        .or(event.value.as_deref())
        .unwrap_or_default()
    {
        "months" => Ok(InformationSortField::MonthsOpen),
        "profit" => Ok(InformationSortField::Profit),
        "avg profit" => Ok(InformationSortField::AverageProfit),
        "occupancy" => Ok(InformationSortField::CurrentCapacity),
        "capacity" => Ok(InformationSortField::TotalCapacity),
        "name" => Ok(InformationSortField::Name),
        "type" => Ok(InformationSortField::Type),
        value => Err(invalid_at(
            input,
            format!("directed multilist sort has unknown authored column {value:?}"),
        )),
    }
}
pub(super) fn required_i64(event: &SourceUiEvent, input: &AuthoredUiDocument) -> io::Result<i64> {
    event
        .value
        .as_deref()
        .or(event.string.as_deref())
        .or(event.data.as_deref())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            invalid_at(
                input,
                format!("{} command has no integer payload", event.message),
            )
        })
}
