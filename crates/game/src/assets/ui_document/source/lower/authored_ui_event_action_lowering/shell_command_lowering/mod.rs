use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::event_i32;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use crate::assets::ui_document::source::lower::canonical_source_value_resolution;
use openzt2_game_data::ui_document::action::shell_navigation::{
    UiShellAction, UiShellActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_shell_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_SET_WORLD_LOCATION" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::FilterWorldChoicesToLocation {
                world_location: event
                    .string
                    .as_deref()
                    .or(event.value.as_deref())
                    .filter(|value| !value.eq_ignore_ascii_case("all"))
                    .map(AssetId::from_key)
                    .unwrap_or_default(),
            },
        })),
        "ZT_SET_XPACK_FILTER" => {
            let expansion_pack_identifier = u16::try_from(event_i32(event, 0)).map_err(|_| invalid_at(input, "map expansion-pack filter must be an unsigned 16-bit integer"))?;
            Ok(UiActionRecord::Shell(UiShellActionRecord {
                trigger,
                action: UiShellAction::FilterWorldChoicesToExpansionPack { expansion_pack_identifier },
            }))
        }
        "ZT_EXITTOMAINMENU" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ReturnToMainMenu,
        })),
        "ZT_SET_SECONDARY_GLOBE" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ShowSecondaryGlobeBiome {
                biome: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(event.string.as_deref().or(event.value.as_deref())),
            },
        })),
        "ZT_SET_GAME_MODE" => {
            let value = event.string.as_deref().or(event.value.as_deref()).unwrap_or_default();
            let action = match value.to_ascii_lowercase().as_str() {
                "freeform" | "config/freeform_mode.xml" => UiShellAction::SelectFreeformModeOrStartSelectedWorld,
                "challenge" | "config/challenge_mode.xml" => UiShellAction::SelectChallengeModeOrStartSelectedWorld,
                "campaign" | "config/campaign_mode.xml" => UiShellAction::SelectCampaignModeOrStartSelectedWorld,
                _ => {
                    return Err(invalid_at(input, format!("unsupported game mode {value:?}")));
                }
            };
            Ok(UiActionRecord::Shell(UiShellActionRecord { trigger, action: action }))
        }
        "ZT_CANCEL_EXITCONFIRMATION" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::DismissExitConfirmationDialog,
        })),
        "ZT_CONFIRM_EXIT" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ShowExitConfirmationDialog,
        })),
        "ZT_SET_EXITCONFIRMATION" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::MarkExitConfirmationPending,
        })),
        "ZT_MAINMENU_AFTER_SAVE" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ReturnToMainMenuAfterWorldSnapshotSave,
        })),
        "ZT_REQUEST_IN_GAME_OPTIONS" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ShowInGameOptionsOverlay,
        })),
        "ZT_EXIT_AFTER_SAVE" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ExitApplicationAfterWorldSnapshotSave,
        })),
        "ZT_SAVE_SCREENSHOT" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::CaptureScreenshotFromSoleActive3dCamera,
        })),
        "ZT_SPLASH_CLOSED" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::FinishSplashPresentation,
        })),
        "ZT_EXIT_DOWNLOADS" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::NavigateBackFromDownloads,
        })),
        "START_FREEFORM" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::SelectFreeformModeOrStartSelectedWorld,
        })),
        "OPEN_CHALLENGE" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::SelectChallengeModeOrStartSelectedWorld,
        })),
        "OPEN_CAMPAIGN" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::SelectCampaignModeOrStartSelectedWorld,
        })),
        "OPEN_OPTIONS" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ShowOptions,
        })),
        "OPEN_DOWNLOADS" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ShowDownloads,
        })),
        "OPEN_SAVED_GAMES" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ShowSavedGames,
        })),
        "QUIT" | "ZT_EXITAPP" => Ok(UiActionRecord::Shell(UiShellActionRecord {
            trigger,
            action: UiShellAction::ExitApplication,
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}
