use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

mod animal_command_lowering;
mod animal_health_command_lowering;
mod application_command_lowering;
mod camera_command_lowering;
mod catalogue_command_lowering;
mod confirmation_command_lowering;
mod construction_command_lowering;
mod economy_command_lowering;
mod immersive_mode_command_lowering;
mod information_command_lowering;
mod persistence_command_lowering;
mod photo_command_lowering;
mod presentation_command_lowering;
mod research_command_lowering;
mod scenario_command_lowering;
mod shell_command_lowering;
mod show_command_lowering;
mod simulation_command_lowering;
mod staff_command_lowering;
mod text_submission_command_lowering;
mod tool_modes_command_lowering;
mod transport_command_lowering;
pub(super) fn lower_authored_ui_event_to_canonical_action(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
    current_stable_name: &str,
    scroll_receiver: AssetId,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
    scope: Option<&str>,
) -> io::Result<Option<UiActionRecord>> {
    match event.message.as_str() {
        "ZT_ANIMAL_RELEASE_WILD"
        | "ZT_ANIMAL_RELEASE_ZOO"
        | "ZT_DECLINE_ADOPTION"
        | "ZT_DECLINE_ALL_ADOPTIONS"
        | "ZT_SET_GENDER" => animal_command_lowering::lower_animal_command(trigger, event, input),
        "ZT_ENTER_DISEASE_MODE" | "ZT_ENTER_TRANQ_MODE" => {
            animal_health_command_lowering::lower_animal_health_command(trigger, event)
        }
        "ZT_RUN_SCRIPT" => application_command_lowering::lower_application_command(trigger, event),
        "ZT_CAMERA_PAN"
        | "ZT_CAMERA_POSITION"
        | "ZT_CAMERA_ROTATE"
        | "ZT_CAMERA_ZOOM"
        | "ZT_CLEAR_COMMAND_STATE"
        | "ZT_FP_TRACINGSTART"
        | "ZT_MOUSELOOK"
        | "ZT_SET_COMMAND_STATE"
        | "ZT_START_FOLLOWCAM"
        | "ZT_STOP_FOLLOWCAM" => {
            camera_command_lowering::lower_camera_command(trigger, event, role, input)
        }
        "ZT_AUTOPOPULATE_LIST" => catalogue_command_lowering::lower_catalogue_command(
            trigger, event, role, current, input,
        ),
        "ZT_ACTION_CONFIRMED" => {
            confirmation_command_lowering::lower_confirmation_command(trigger, event, input, scope)
        }
        "ZT_BIOME_PAINT_FOLIAGE"
        | "ZT_BIOME_PAINT_ROCKS"
        | "ZT_BIOME_PAINT_TREES"
        | "ZT_CLEARMODE"
        | "ZT_CONFIRM_ENTITY_PLACEMENT"
        | "ZT_CRATE_ENTITY"
        | "ZT_ELEVATED_PATH_PLACEMENT_HEIGHT"
        | "ZT_ELEVATED_PATH_PLACEMENT_MODE"
        | "ZT_EXITMODE"
        | "ZT_EYEDROPPER"
        | "ZT_MOVE_SELECTION_ENTITY"
        | "ZT_OBJECT_ROTATE"
        | "ZT_SELL_ENTITY"
        | "ZT_SELL_PHYSOBJ"
        | "ZT_SETFENCEPLACEMENTMODE"
        | "ZT_SETPLACEMENTOBJECT"
        | "ZT_SETSINGLEOBJECTPLACEMENT"
        | "ZT_SETTANKMODE"
        | "ZT_SETTERRAINFLATTEN_HEIGHT"
        | "ZT_SETTERRAINFLATTEN_SPEED"
        | "ZT_SETTERRAINMODE"
        | "ZT_SET_AUTO_PLACEMENT_LIST"
        | "ZT_SET_BIOME"
        | "ZT_SKYTOWER_PLACEMENT_HEIGHT"
        | "ZT_TERRAINCURSOR_SIZE"
        | "ZT_UNCRATE_ENTITY"
        | "ZT_UNDOACTION" => construction_command_lowering::lower_construction_command(
            trigger,
            event,
            current_stable_name,
            input,
        ),
        "ZT_GIVE_CASH_GRANT"
        | "ZT_SET_ADMISSION_PRICE_INDEX"
        | "ZT_SET_FILTER_MAINTENANCE"
        | "ZT_SET_PRICE_INDEX"
        | "ZT_UPDATE_SELL_MESSAGE"
        | "ZT_ZOO_OPEN" => {
            economy_command_lowering::lower_economy_command(trigger, event, role, current, input)
        }
        "ZT_BOARD_SELECTED_ENTITY"
        | "ZT_ENTER_CLONING_MODE"
        | "ZT_ENTER_FOSSIL_MODE"
        | "ZT_ENTER_PUZZLE_MODE" => {
            immersive_mode_command_lowering::lower_immersive_mode_command(trigger, event)
        }
        "UI_HYPERLINK"
        | "ZT_EVENT"
        | "ZT_EXPORT_OVERVIEW_MAP"
        | "ZT_FIND_ANIMAL_ON_PANEL"
        | "ZT_GRAPHTYPE_SELECTION"
        | "ZT_GRAPH_SELECTION"
        | "ZT_MULTILIST_SORT_BY_ANIMAL_HAPPINESS_UP"
        | "ZT_MULTILIST_SORT_BY_FAVORITE_ANIMAL"
        | "ZT_MULTILIST_SORT_BY_INT_GREATER"
        | "ZT_MULTILIST_SORT_BY_NEED_DOWN"
        | "ZT_MULTILIST_SORT_BY_PREGNANCY_DOWN"
        | "ZT_MULTILIST_SORT_BY_STRING"
        | "ZT_MULTILIST_SORT_BY_STRING_REVERSE"
        | "ZT_MULTILIST_SORT_BY_TYPE"
        | "ZT_SELECTNEXT_DISEASED_ANIMAL"
        | "ZT_SELECTNEXT_RAMPAGING_ANIMAL"
        | "ZT_SET_BIOME_FILTER"
        | "ZT_SET_NO_FILTER"
        | "ZT_SET_RESEARCH_FILTER"
        | "ZT_SET_SELECTED_ENTITY"
        | "ZT_SET_THEME_FILTER"
        | "ZT_VIEWFILTER_HIDE"
        | "ZT_VIEWFILTER_SHOW"
        | "ZT_ZOOPEDIA_BACK"
        | "ZT_ZOOPEDIA_FORWARD" => information_command_lowering::lower_information_command(
            trigger, event, role, current, input,
        ),
        "LOAD" | "SAVE" | "ZT_LOADGAME" | "ZT_LOAD_AFTER_SAVE" | "ZT_SAVEGAME" => {
            persistence_command_lowering::lower_persistence_command(trigger, event, input)
        }
        "ZT_PHOTOEVENT_ALBUM"
        | "ZT_PHOTOEVENT_ALBUM_DESELECT_PICTURE"
        | "ZT_PHOTOEVENT_ALBUM_INCREASE_SIZE"
        | "ZT_PHOTOEVENT_ALBUM_NEXTPAGE"
        | "ZT_PHOTOEVENT_ALBUM_PREVPAGE"
        | "ZT_PHOTOEVENT_ALBUM_SAVE_AS_HTML"
        | "ZT_PHOTOEVENT_ALBUM_SELECT_PICTURE"
        | "ZT_PHOTOEVENT_CAMERA_DELETE_ALL_PICTURES"
        | "ZT_PHOTOEVENT_CAMERA_DESELECT_PICTURE"
        | "ZT_PHOTOEVENT_CAMERA_SELECT_PICTURE"
        | "ZT_PHOTOEVENT_DELETE_ALBUM"
        | "ZT_PHOTOEVENT_DELETE_ALBUM_PICTURE"
        | "ZT_PHOTOEVENT_DELETE_CAMERA_PICTURE"
        | "ZT_PHOTOEVENT_DELETE_SELECTED_PICTURE"
        | "ZT_PHOTOEVENT_ENLARGE"
        | "ZT_PHOTOEVENT_ENLARGE_ALBUM_PIC"
        | "ZT_PHOTOEVENT_MOVE_ENTER"
        | "ZT_PHOTOEVENT_MOVE_EXIT"
        | "ZT_PHOTOEVENT_MOVE_SELECTED_PICTURE"
        | "ZT_PHOTOEVENT_MOVE_SELECTED_PICTURE_CANCELED"
        | "ZT_PHOTOEVENT_MOVE_SELECTED_TARGET"
        | "ZT_PHOTOEVENT_NEW_ALBUM"
        | "ZT_PHOTOEVENT_NUM_PICTURES"
        | "ZT_PHOTOEVENT_SELECT_ALBUM"
        | "ZT_PHOTOEVENT_SHRINK"
        | "ZT_PHOTOEVENT_TAKE_PHOTO"
        | "ZT_SET_USER_ATTRIBUTE" => {
            photo_command_lowering::lower_photo_command(trigger, event, input)
        }
        "UI_ACTIVATE"
        | "UI_ACTIVATE_DATA"
        | "UI_ACTIVATE_OFF"
        | "UI_ACTIVATE_ON"
        | "UI_ALERT_RETURN"
        | "UI_CHILD"
        | "UI_CLOSE"
        | "UI_COLLAPSE"
        | "UI_COPY_NAME"
        | "UI_COPY_TEXT"
        | "UI_DISABLE"
        | "UI_ENABLE"
        | "UI_EXPAND"
        | "UI_HIDE"
        | "UI_LBUTTONUP"
        | "UI_MOUSE_ENTER"
        | "UI_MOUSE_LEAVE"
        | "UI_NEXT_CACHED_MSG"
        | "UI_PLAY_SOUND"
        | "UI_QUERY_TARGET"
        | "UI_REPRESS"
        | "UI_ROOT_CHILD"
        | "UI_SCROLL"
        | "UI_SELF"
        | "UI_SETCURSOR"
        | "UI_SET_IMAGE"
        | "UI_SET_LOCID"
        | "UI_SET_LTT"
        | "UI_SET_MODAL"
        | "UI_SET_NEXT"
        | "UI_SET_POS"
        | "UI_SET_PREVIOUS"
        | "UI_SET_SCROLL"
        | "UI_SET_SIZE"
        | "UI_SET_SRC_RECT"
        | "UI_SET_STT"
        | "UI_SET_TEXT"
        | "UI_SET_VALUE"
        | "UI_SHOW"
        | "UI_SHOW_CHILD_EX"
        | "UI_WIND_ANIMATION"
        | "ZT_ACTIVATE_MODE_HELP"
        | "ZT_CANCEL_GENERIC_CONFIRMATION"
        | "ZT_GENERIC_CONFIRMATION_PROCEED" => {
            presentation_command_lowering::lower_presentation_command(
                trigger,
                event,
                role,
                current,
                current_stable_name,
                scroll_receiver,
                output,
                input,
                scope,
            )
        }
        "ZT_START_RESEARCHING_ENTITY" => {
            research_command_lowering::lower_research_command(trigger, event)
        }
        "BFS_CLEARSCENARIO"
        | "ZT_APPMSG"
        | "ZT_INCREMENT_DISEASE_HINT"
        | "ZT_PLAY_NEXT_SCENARIO"
        | "ZT_PLAY_TUTORIAL"
        | "ZT_POPULATE_CAMPAIGN_UI"
        | "ZT_POPULATE_SCENARIO_UI"
        | "ZT_SCENARIO_SELECTION_CHANGED"
        | "ZT_SET_STATUS_FILTER"
        | "ZT_SET_TYPE_FILTER" => {
            scenario_command_lowering::lower_scenario_command(trigger, event, input)
        }
        "OPEN_CAMPAIGN"
        | "OPEN_CHALLENGE"
        | "OPEN_DOWNLOADS"
        | "OPEN_OPTIONS"
        | "OPEN_SAVED_GAMES"
        | "QUIT"
        | "START_FREEFORM"
        | "ZT_CANCEL_EXITCONFIRMATION"
        | "ZT_CONFIRM_EXIT"
        | "ZT_EXITAPP"
        | "ZT_EXITTOMAINMENU"
        | "ZT_EXIT_AFTER_SAVE"
        | "ZT_EXIT_DOWNLOADS"
        | "ZT_MAINMENU_AFTER_SAVE"
        | "ZT_REQUEST_IN_GAME_OPTIONS"
        | "ZT_SAVE_SCREENSHOT"
        | "ZT_SET_EXITCONFIRMATION"
        | "ZT_SET_GAME_MODE"
        | "ZT_SET_SECONDARY_GLOBE"
        | "ZT_SET_WORLD_LOCATION"
        | "ZT_SET_XPACK_FILTER"
        | "ZT_SPLASH_CLOSED" => shell_command_lowering::lower_shell_command(trigger, event, input),
        "ZT_SHOWMIXER_CANCELEDIT"
        | "ZT_SHOWMIXER_DROPDOWNBACKGROUNDACTIVATED"
        | "ZT_SHOWMIXER_REQUEST_CANCELEDIT"
        | "ZT_SHOWMIXER_REQUEST_TOGGLEEDITSHOW"
        | "ZT_SHOWMIXER_SETSHOWOPEN"
        | "ZT_SHOWMIXER_SHOWMIXERPANEL"
        | "ZT_SHOWMIXER_TOGGLEEDITSHOW"
        | "ZT_SHOWPLATFORM_TRANSACTCURRENTUPGRADE"
        | "ZT_SHOWSCHEDULER_ADDBREAK"
        | "ZT_SHOWSCHEDULER_ADDSHOW"
        | "ZT_SHOWSCHEDULER_DELETESHOW"
        | "ZT_SHOWSCHEDULER_EDITSHOW"
        | "ZT_SHOWSCHEDULER_ENTRYDOUBLECLICK"
        | "ZT_SHOWSCHEDULER_MOVEDOWN"
        | "ZT_SHOWSCHEDULER_MOVEUP"
        | "ZT_SHOWSCHEDULER_REQUEST_ADDSHOW"
        | "ZT_SHOWSCHEDULER_REQUEST_DELETESHOW"
        | "ZT_SHOWSCHEDULER_SCHEDULERTABCLICKED"
        | "ZT_SHOWSCHEDULER_TOGGLEENABLE"
        | "ZT_SHOWSCHEDULER_VIEWSHOW" => {
            show_command_lowering::lower_show_command(trigger, event, input)
        }
        "ZT_PAUSE" | "ZT_PAUSE_KEY" | "ZT_TEMPORARY_PAUSE" => {
            simulation_command_lowering::lower_simulation_command(trigger, event, input)
        }
        "ZT_ASSIGN_KEEPER"
        | "ZT_ASSIGN_WORKER"
        | "ZT_DELETE_KEEPER_ASSIGNMENT"
        | "ZT_FIRE_STAFF"
        | "ZT_KEEPER_ASSIGNMENT_SELECTED"
        | "ZT_TRAINER_ASSIGNMENT_SELECTED"
        | "ZT_UNASSIGN_WORKER" => {
            staff_command_lowering::lower_staff_command(trigger, event, role, current, input)
        }
        "UI_SEND_TEXT" => {
            text_submission_command_lowering::lower_text_submission_command(trigger, event, role)
        }
        "ZT_SETMODE" => {
            tool_modes_command_lowering::lower_tool_modes_command(trigger, event, role, current)
        }
        "ZT_CHANGE_CIRCUIT_DIRECTION" | "ZT_GENERATE_VEHICLE" | "ZT_TOGGLE_CIRCUIT_STATUS" => {
            transport_command_lowering::lower_transport_command(trigger, event)
        }
        _ => Ok(None),
    }
}
