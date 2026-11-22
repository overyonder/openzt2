use bevy::prelude::*;
use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use openzt2_game_assets::world_definitions::loaded_staff_behavior_capabilities::LoadedStaffBehaviorCapabilities;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

use super::staff_employment_types::{Staff, StaffJobCapabilities, StaffRole};

pub(super) fn project_loaded_behavior_capabilities_onto_staff(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    behavior_assets: Res<Assets<BehaviorDocumentAsset>>,
    behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    loaded_capabilities: Res<LoadedStaffBehaviorCapabilities>,
    mut staff: Query<(Entity, Ref<StaffRole>, Option<&mut StaffJobCapabilities>), With<Staff>>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    if behavior_documents
        .create_declaration_index_view(&behavior_assets)
        .is_none()
    {
        return;
    }
    let definitions_changed = loaded_capabilities.is_changed()
        || active_definitions.is_changed()
        || definitions.is_changed();
    for (entity, role, capabilities) in &mut staff {
        if !definitions_changed && !role.is_changed() && capabilities.is_some() {
            continue;
        }
        let Some(role) = catalogue.find_staff(role.0) else {
            if capabilities.is_some() && !catalogue.has_pending_assets() {
                commands.entity(entity).remove::<StaffJobCapabilities>();
            }
            continue;
        };
        let next = StaffJobCapabilities::new(loaded_capabilities.for_employee(catalogue, role));
        if let Some(mut capabilities) = capabilities {
            capabilities.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}
