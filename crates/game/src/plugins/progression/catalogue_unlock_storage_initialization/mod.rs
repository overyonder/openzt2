use super::research_types::ResearchProjectAvailability;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::UnlockRequirement;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::mark_catalogue_definition_unlocked_at_index,
    unlock_types::{CatalogueUnlockStorageInitialized, UnlockedCatalogueDefinitionSet},
};

pub(super) fn initialize_catalogue_unlock_storage_from_active_world_definitions(
    mut commands: Commands,
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            Without<CatalogueUnlockStorageInitialized>,
        ),
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    research_availability: Query<&ResearchProjectAvailability>,
    mut identifiers: ResMut<PersistentIdAllocator>,
    mut unlocked_catalogue_definitions: ResMut<UnlockedCatalogueDefinitionSet>,
) {
    let Some(root) = roots.iter().next() else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let definition_count = definitions.catalogue().count() as u32;
    let required_words = (definition_count as usize).div_ceil(u64::BITS as usize);
    if unlocked_catalogue_definitions.definition_count != definition_count
        || unlocked_catalogue_definitions.words.len() != required_words
    {
        unlocked_catalogue_definitions.words.clear();
        unlocked_catalogue_definitions
            .words
            .resize(required_words, 0);
        unlocked_catalogue_definitions.definition_count = definition_count;
    }
    for authored_unlock in definitions.unlocks() {
        if matches!(&authored_unlock.requirement, UnlockRequirement::Always) {
            let target = authored_unlock.target;
            if let Some(index) = find_catalogue_definition_index(definitions, target) {
                let _ = mark_catalogue_definition_unlocked_at_index(
                    &mut unlocked_catalogue_definitions,
                    index,
                );
            }
        }
    }
    for research in definitions.research() {
        if research_availability
            .iter()
            .any(|availability| availability.item == research.id)
        {
            continue;
        }
        let Ok(identifier) = identifiers.allocate(root) else {
            return;
        };
        commands.spawn((
            ResearchProjectAvailability {
                item: research.id,
                available: false,
            },
            WorldMember { root },
            identifier,
        ));
    }
    commands
        .entity(root)
        .insert(CatalogueUnlockStorageInitialized);
}
