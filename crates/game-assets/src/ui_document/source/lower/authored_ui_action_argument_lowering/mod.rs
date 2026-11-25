use crate::source_document::ui::model::{SourceUiEvent, SourceUiEventTrigger, SourceUiNode};
use crate::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, parse_bool,
};
use openzt2_game_data::ui_document::action::camera::{
    UiCameraPanDirection, UiCameraSignedAxisDirection,
};
use openzt2_game_data::ui_document::action::information::InformationSortField;
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::ui_document::node::UiNodeDefinition;
use openzt2_game_data::ui_document::widget::UiWidgetRecord;
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;
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
        SourceUiEventTrigger::SecondaryPress => UiTrigger::SecondaryPress,
    }
}

/// The node an event names. A name that repeats in the document is settled
/// once the tree is complete, by `resolve_repeated_target_names_to_nearest_scope`.
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

/// Repeated names, such as every confirm.xml dialog's "cancel", receive
/// indexed ids, so a name alone reaches only its first node. The original
/// looks a name up from the node acting on it: its own subtree first, then
/// each enclosing subtree outwards. Point every same-document target of a
/// repeated name at that nearest node.
pub(super) fn resolve_repeated_target_names_to_nearest_scope(
    nodes: &mut [UiNodeDefinition],
    role: UiDocumentRole,
    receivers: &BTreeMap<
        (u32, usize),
        super::authored_ui_event_collection_lowering::UiActionReceiverPath,
    >,
    timed_receivers: &BTreeMap<
        (u32, usize),
        super::authored_ui_event_collection_lowering::UiActionReceiverPath,
    >,
) {
    let mut named = BTreeMap::<AssetId, Vec<u32>>::new();
    for (index, node) in nodes.iter().enumerate() {
        named
            .entry(UiDocumentRole::node_id(role, &node.name))
            .or_default()
            .push(index as u32);
    }
    let parent = |nodes: &[UiNodeDefinition], index: u32| {
        nodes
            .get(index as usize)
            .map(|node| node.parent)
            .filter(|parent| (*parent as usize) < nodes.len())
    };
    let encloses = |nodes: &[UiNodeDefinition], scope: u32, mut node: u32| loop {
        if node == scope {
            break true;
        }
        match parent(nodes, node) {
            Some(next) => node = next,
            None => break false,
        }
    };
    let nearest_in_scope = |actor: u32, candidates: &[u32]| {
        std::iter::successors(Some(actor), |scope| parent(nodes, *scope)).find_map(|scope| {
            candidates
                .iter()
                .copied()
                .find(|candidate| encloses(nodes, scope, *candidate))
        })
    };
    let mut resolved = Vec::new();
    for (actor, node) in nodes.iter().enumerate() {
        let actor = actor as u32;
        let records = node
            .actions
            .iter()
            .chain(node.hotkeys.iter().flat_map(|hotkey| &hotkey.actions))
            .enumerate()
            .map(|(index, action)| (index, action, receivers.get(&(actor, index))));
        let timed_start = node.actions.len()
            + node
                .hotkeys
                .iter()
                .map(|hotkey| hotkey.actions.len())
                .sum::<usize>();
        let timed_records = match &node.widget {
            UiWidgetRecord::TimedSequence { events, .. } => events.as_slice(),
            _ => &[],
        };
        let timed_records = timed_records.iter().enumerate().map(|(index, event)| {
            (
                timed_start + index,
                &event.action,
                timed_receivers.get(&(actor, index)),
            )
        });
        for (record_index, record, receiver) in records.chain(timed_records) {
            let scope = receiver.map_or(actor, |receiver| {
                receiver.nodes.iter().fold(
                    if receiver.from_root { 0 } else { actor },
                    |scope, node| {
                        named
                            .get(node)
                            .and_then(|candidates| nearest_in_scope(scope, candidates))
                            .unwrap_or(scope)
                    },
                )
            });
            visit_local_targets(&mut record.clone(), |target_index, target| {
                if let Some(candidates) =
                    named.get(target).filter(|candidates| candidates.len() > 1)
                {
                    if let Some(nearest) = nearest_in_scope(scope, candidates) {
                        resolved.push((
                            actor,
                            record_index,
                            target_index,
                            nodes[nearest as usize].id,
                        ));
                    }
                }
            });
        }
    }
    for (actor, record_index, target_index, id) in resolved {
        let node = &mut nodes[actor as usize];
        let action_count = node.actions.len();
        let timed_start = action_count
            + node
                .hotkeys
                .iter()
                .map(|hotkey| hotkey.actions.len())
                .sum::<usize>();
        let record = if record_index < action_count {
            &mut node.actions[record_index]
        } else if record_index < timed_start {
            node.hotkeys
                .iter_mut()
                .flat_map(|hotkey| &mut hotkey.actions)
                .nth(record_index - action_count)
                .expect("hotkey action index")
        } else {
            let UiWidgetRecord::TimedSequence { events, .. } = &mut node.widget else {
                continue;
            };
            &mut events[record_index - timed_start].action
        };
        visit_local_targets(record, |index, target| {
            if index == target_index {
                *target = id;
            }
        });
    }
}

fn visit_local_targets(action: &mut UiActionRecord, mut visit: impl FnMut(usize, &mut AssetId)) {
    match action {
        UiActionRecord::Presentation(record) => {
            for (index, target) in record.action.local_node_targets_mut().enumerate() {
                visit(index, target);
            }
        }
        UiActionRecord::Script(record) if record.document_role.is_none() => {
            visit(0, &mut record.receiver)
        }
        _ => {}
    }
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

pub(super) fn worker_task(event: &SourceUiEvent) -> Option<AssetId> {
    let token = event.string.as_deref()?.trim();
    (!token.is_empty()).then(|| AssetId::from_key(
        &crate::source_document::source_document_semantic_name::canonicalize_source_document_record_key(token),
    ))
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
