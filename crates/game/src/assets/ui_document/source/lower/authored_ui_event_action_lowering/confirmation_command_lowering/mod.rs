use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::required_bool;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::action::animal_shows::{UiShowAction, UiShowActionRecord};
use openzt2_game_data::ui_document::action::construction::{
    UiConstructionAction, UiConstructionActionRecord, UiConstructionConfirmation,
};
use openzt2_game_data::ui_document::action::presentation::{
    UiPresentationAction, UiPresentationActionRecord,
};
use openzt2_game_data::ui_document::action::shell_navigation::{
    UiShellAction, UiShellActionRecord,
};
use openzt2_game_data::ui_document::action::transportation::{
    UiTransportAction, UiTransportActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_confirmation_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
    scope: Option<&str>,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_ACTION_CONFIRMED" => {
            let confirmed = required_bool(event, input)?;
            if scope == Some("exit_zoo") {
                return Ok(Some(UiActionRecord::Shell(UiShellActionRecord {
                    trigger,
                    action: if confirmed {
                        UiShellAction::ExitApplication
                    } else {
                        UiShellAction::DismissExitConfirmationDialog
                    },
                })));
            }
            Ok(match scope.unwrap_or_default() {
                "tranporation_confirmation" => UiActionRecord::Transport(UiTransportActionRecord {
                    trigger,
                    action: UiTransportAction::ResolvePendingTransportPathDeletion {
                        deletion_confirmed: confirmed,
                    },
                }),
                "zoogatepath_delete_confirmation" => {
                    UiActionRecord::Construction(UiConstructionActionRecord {
                        trigger,
                        action: UiConstructionAction::ResolveConfirmation {
                            kind: UiConstructionConfirmation::ZooGatePathDeletion,
                            confirmed: confirmed,
                        },
                    })
                }
                "tank_delete_confirmation" => {
                    UiActionRecord::Construction(UiConstructionActionRecord {
                        trigger,
                        action: UiConstructionAction::ResolveConfirmation {
                            kind: UiConstructionConfirmation::TankDeletion,
                            confirmed: confirmed,
                        },
                    })
                }
                "tank_merge_confirmation" => {
                    UiActionRecord::Construction(UiConstructionActionRecord {
                        trigger,
                        action: UiConstructionAction::ResolveConfirmation {
                            kind: UiConstructionConfirmation::TankMerge,
                            confirmed: confirmed,
                        },
                    })
                }
                "show_tank_split_confirmation" => {
                    UiActionRecord::Construction(UiConstructionActionRecord {
                        trigger,
                        action: UiConstructionAction::ResolveConfirmation {
                            kind: UiConstructionConfirmation::TankSplit,
                            confirmed: confirmed,
                        },
                    })
                }
                "show_platform_delete_confirmation" => UiActionRecord::Show(UiShowActionRecord {
                    trigger,
                    action: UiShowAction::ResolvePlatformDeletion {
                        confirmed: confirmed,
                    },
                }),
                "show_tank_merge_message" => {
                    return Ok(Some(UiActionRecord::Presentation(
                        UiPresentationActionRecord {
                            trigger,
                            action:
                                UiPresentationAction::HideOwningDocumentAfterConfirmationDismissal,
                        },
                    )));
                }
                unknown => {
                    return Err(invalid_at(
                        input,
                        format!("action confirmation has no closed modal owner {unknown:?}"),
                    ));
                }
            })
        }
        _ => return Ok(None),
    };
    result.map(Some)
}
