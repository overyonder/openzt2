pub(super) mod animal_alert_button_projection;
pub(super) mod animal_need_and_health_projection;
pub(super) mod animal_pickup_release_information_projection;
pub(super) mod entity_editor_data_root_projection;
pub(super) mod facility_status_projection;
pub(super) mod fame_presentation;
pub(super) mod guest_status_projection;
pub(super) mod habitat_and_tank_status_projection;
pub(super) mod information_list_membership;
pub(super) mod information_list_projection;
pub(super) mod information_list_row_identity_projection;
pub(super) mod information_list_row_value_projection;
pub(super) mod information_row_field_binding;
pub(super) mod maintenance_status_projection;
pub(super) mod panel_binding_families;
pub(super) mod progression_status_projection;
pub(super) mod scenario_status_projection;
pub(super) mod selected_animal_need_list_projection;
pub(super) mod selected_animal_summary_projection;
pub(super) mod selected_entity_identity_projection;
pub(super) mod show_status_projection;
pub(super) mod staff_status_projection;
pub(super) mod text_replacement_operations;
pub(super) mod transport_status_projection;
pub(super) mod zoo_status_projection;

use bevy::prelude::*;
use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;

type InformationRows<'w, 's> = Query<
    'w,
    's,
    (
        Ref<'static, InformationEntitySource>,
        Ref<'static, information_row_field_binding::InformationRowFields>,
    ),
    With<UiListRow>,
>;
