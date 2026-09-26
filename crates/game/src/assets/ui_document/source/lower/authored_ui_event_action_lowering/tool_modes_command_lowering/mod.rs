use crate::assets::source_document::ui::model::SourceUiEvent;
use openzt2_game_data::ui_document::action::animal_health::{
    UiAnimalHealthAction, UiAnimalHealthActionRecord,
};
use openzt2_game_data::ui_document::action::camera::{UiCameraAction, UiCameraActionRecord};
use openzt2_game_data::ui_document::action::construction::{
    UiBiomeSurface, UiConstructionAction, UiConstructionActionRecord,
};
use openzt2_game_data::ui_document::action::immersive_mode::{
    UiEnterImmersiveModeActionRecord, UiImmersiveModeKind,
};
use openzt2_game_data::ui_document::action::persistence::{
    UiPersistenceAction, UiPersistenceActionRecord,
};
use openzt2_game_data::ui_document::action::photography::{UiPhotoAction, UiPhotoActionRecord};
use openzt2_game_data::ui_document::action::presentation::{
    UiPresentationAction, UiPresentationActionRecord,
};
use openzt2_game_data::ui_document::action::shell_navigation::{
    UiShellAction, UiShellActionRecord,
};
use openzt2_game_data::ui_document::action::staff_management::{
    UiStaffAction, UiStaffActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_tool_modes_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_SETMODE" => match event
            .string
            .as_deref()
            .or(event.value.as_deref())
            .unwrap_or_default()
        {
            "mode_placement" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::EnterPlacementMode,
            })),
            "mode_selection" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::SelectInspectionTool,
            })),
            "mode_biome" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::EnterBiomeEditing,
            })),
            "deepwater" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::SetBiomeSurface {
                    surface: UiBiomeSurface::DeepWater,
                },
            })),
            "shallowwater" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::SetBiomeSurface {
                    surface: UiBiomeSurface::ShallowWater,
                },
            })),
            "ground" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::SetBiomeSurface {
                    surface: UiBiomeSurface::Ground,
                },
            })),
            "mix" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::SetBiomeSurface {
                    surface: UiBiomeSurface::MixedGround,
                },
            })),
            "ground-cover" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::SetBiomeSurface {
                    surface: UiBiomeSurface::GroundCover,
                },
            })),
            "foliage-mix" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::SetBiomeSurface {
                    surface: UiBiomeSurface::FoliageMix,
                },
            })),
            "mode_deformation" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::EnterTerrainEditing,
            })),
            "mode_deletion" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::EnterDeletionMode,
            })),
            "mode_tank_manipulation" => {
                Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                    trigger,
                    action: UiConstructionAction::EnterTankEditing,
                }))
            }
            "mode_fence_placement" => {
                Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                    trigger,
                    action: UiConstructionAction::EnterFencePlacementMode,
                }))
            }
            "mode_path_placement" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::EnterPathPlacementMode,
            })),
            "mode_elevated_path" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::EnterElevatedPathPlacementMode,
            })),
            "mode_elevated_curb" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::EnterElevatedCurbPlacementMode,
            })),
            "mode_sky_track_placement" => {
                Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                    trigger,
                    action: UiConstructionAction::EnterSkyTrackPlacementMode,
                }))
            }
            "mode_ground_track_placement" => {
                Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                    trigger,
                    action: UiConstructionAction::EnterGroundTrackPlacementMode,
                }))
            }
            "mode_staff_assign" => Ok(UiActionRecord::Staff(UiStaffActionRecord {
                trigger,
                action: UiStaffAction::EnterStaffAssignmentMode,
            })),
            "mode_super_staff" if role == UiDocumentRole::PhotoMode => {
                Ok(UiActionRecord::Photo(UiPhotoActionRecord {
                    trigger,
                    action: UiPhotoAction::ExitActivePhotoMode,
                }))
            }
            // Entering first-person mode also enters its default super-staff child.
            "mode_first_person" | "mode_super_staff" => Ok(UiActionRecord::EnterImmersiveMode(
                UiEnterImmersiveModeActionRecord {
                    trigger,
                    mode: UiImmersiveModeKind::SuperStaff,
                },
            )),
            "mode_guest_view" => Ok(UiActionRecord::EnterImmersiveMode(
                UiEnterImmersiveModeActionRecord {
                    trigger,
                    mode: UiImmersiveModeKind::GuestView,
                },
            )),
            "mode_cure_disease" => Ok(UiActionRecord::AnimalHealth(UiAnimalHealthActionRecord {
                trigger,
                action: UiAnimalHealthAction::EnterDiseaseTreatmentMode,
            })),
            "mode_tranquilize" => Ok(UiActionRecord::AnimalHealth(UiAnimalHealthActionRecord {
                trigger,
                action: UiAnimalHealthAction::EnterTranquilizerMode,
            })),
            "mode_find_fossils" => Ok(UiActionRecord::EnterImmersiveMode(
                UiEnterImmersiveModeActionRecord {
                    trigger,
                    mode: UiImmersiveModeKind::FossilSearch,
                },
            )),
            "mode_overhead" => Ok(UiActionRecord::Camera(UiCameraActionRecord {
                trigger,
                action: UiCameraAction::RestorePreviouslySavedCameraMode,
            })),
            "mode_photo_safari" => Ok(UiActionRecord::EnterImmersiveMode(
                UiEnterImmersiveModeActionRecord {
                    trigger,
                    mode: UiImmersiveModeKind::Photo,
                },
            )),
            "mode_save" => Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
                trigger,
                action: UiPersistenceAction::OpenSaveSlotCatalogueForSaving,
            })),
            "mode_load" | "mode_mainload" => {
                Ok(UiActionRecord::Persistence(UiPersistenceActionRecord {
                    trigger,
                    action: UiPersistenceAction::OpenSaveSlotCatalogueForLoading,
                }))
            }
            "mode_options" => Ok(UiActionRecord::Shell(UiShellActionRecord {
                trigger,
                action: UiShellAction::ShowOptions,
            })),
            "mode_download" => Ok(UiActionRecord::Shell(UiShellActionRecord {
                trigger,
                action: UiShellAction::ShowDownloads,
            })),
            "mode_ui_layout" => Ok(UiActionRecord::Presentation(UiPresentationActionRecord {
                trigger,
                action: UiPresentationAction::SetTargetNodeInteractionEnabled {
                    target_node: current,
                    enabled: false,
                },
            })),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    result.map(Some)
}
