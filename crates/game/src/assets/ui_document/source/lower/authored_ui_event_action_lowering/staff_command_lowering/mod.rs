use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::worker_duty;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use openzt2_game_data::ui_document::action::staff_management::{
    UiStaffAction, UiStaffActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_staff_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
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
            let duty = worker_duty(current, role, input)?;
            Ok(UiActionRecord::Staff(UiStaffActionRecord {
                trigger,
                action: UiStaffAction::SetWorkerDutyAssignment {
                    duty: duty,
                    assigned: event.message == "ZT_ASSIGN_WORKER",
                },
            }))
        }
        _ => return Ok(None),
    };
    result.map(Some)
}
