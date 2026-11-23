use crate::source_document::ui::model::SourceUiEvent;
use crate::ui_document::source::lower::authored_ui_action_argument_lowering::worker_duty;
use crate::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::action::staff_management::{
    UiStaffAction, UiStaffActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_staff_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_KEEPER_ASSIGNMENT_SELECTED" => Ok(UiActionRecord::Staff(UiStaffActionRecord {
            trigger,
            action: UiStaffAction::SetKeeperAssignmentSelection {
                assignment_index: event
                    .value
                    .as_deref()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(-1),
            },
        })),
        "ZT_TRAINER_ASSIGNMENT_SELECTED" => Ok(UiActionRecord::Staff(UiStaffActionRecord {
            trigger,
            action: UiStaffAction::SetTrainerAssignmentSelection {
                assignment_index: event
                    .value
                    .as_deref()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(-1),
            },
        })),
        "ZT_FIRE_STAFF" => Ok(UiActionRecord::Staff(UiStaffActionRecord {
            trigger,
            action: UiStaffAction::FireSelectedStaff,
        })),
        "ZT_ASSIGN_KEEPER" => Ok(UiActionRecord::Staff(UiStaffActionRecord {
            trigger,
            action: UiStaffAction::MarkSelectedEntityAsKeeperAssignmentTarget,
        })),
        "ZT_DELETE_KEEPER_ASSIGNMENT" => Ok(UiActionRecord::Staff(UiStaffActionRecord {
            trigger,
            action: UiStaffAction::ClearSelectedKeeperAssignmentTarget,
        })),
        "ZT_ASSIGN_WORKER" | "ZT_UNASSIGN_WORKER" => {
            let duty = worker_duty(event).ok_or_else(|| {
                invalid_at(
                    input,
                    format!(
                        "{} has an unknown worker task {:?}",
                        event.message, event.string
                    ),
                )
            })?;
            Ok(UiActionRecord::Staff(UiStaffActionRecord {
                trigger,
                action: UiStaffAction::SetWorkerDutyAssignment {
                    duty,
                    assigned: event.message == "ZT_ASSIGN_WORKER",
                },
            }))
        }
        _ => return Ok(None),
    };
    result.map(Some)
}
