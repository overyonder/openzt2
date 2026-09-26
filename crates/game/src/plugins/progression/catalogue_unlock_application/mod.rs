use bevy::prelude::*;
use openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::UnlockRequirement;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::{
    award_and_progression_fact_types::{ProgressionAwardGranted, ProgressionFactChanged},
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::mark_catalogue_definition_unlocked_at_index,
    fame_types::Fame,
    research_types::ResearchProjectCompleted,
    unlock_types::{
        ApplyCatalogueDefinitionUnlockRequest, CatalogueDefinitionUnlockApplied,
        CatalogueDefinitionUnlockRejected, UnlockedCatalogueDefinitionSet,
    },
};

pub(super) fn apply_catalogue_definition_unlock_requests(
    mut requests: MessageReader<ApplyCatalogueDefinitionUnlockRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut applied: MessageWriter<CatalogueDefinitionUnlockApplied>,
    mut rejected: MessageWriter<CatalogueDefinitionUnlockRejected>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        for request in requests.read() {
            rejected.write(CatalogueDefinitionUnlockRejected {
                operation: request.operation,
                definition: request.definition,
            });
        }
        return;
    };
    for request in requests.read() {
        let Some(index) = find_catalogue_definition_index(definitions, request.definition) else {
            rejected.write(CatalogueDefinitionUnlockRejected {
                operation: request.operation,
                definition: request.definition,
            });
            continue;
        };
        let was_changed =
            mark_catalogue_definition_unlocked_at_index(&mut unlocks, index).unwrap_or(false);
        applied.write(CatalogueDefinitionUnlockApplied {
            operation: request.operation,
            definition: request.definition,
        });
        if was_changed {
            changed.write(ProgressionFactChanged::Unlock(request.definition));
        }
    }
}

/// Applies the declarative world-definition catalogue unlock relations after
/// their authoritative progression facts change. The loaded table remains the
/// sole authored relation; this system mutates only the bounded live unlock
/// bit set.
///
/// Scenario-gated rows are deliberately not inferred here. The scenario system resolves a
/// completed scenario consequence into an
/// `ApplyCatalogueDefinitionUnlockRequest`, preserving that package's
/// operation correlation without retaining a scenario mirror in progression.
pub(super) fn apply_authored_catalogue_unlock_requirements_from_progression_facts(
    fame: Res<Fame>,
    mut research_completed: MessageReader<ResearchProjectCompleted>,
    mut awards_granted: MessageReader<ProgressionAwardGranted>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };

    if fame.is_changed() {
        for definition in definitions.unlocks() {
            if matches!(
                &definition.requirement,
                UnlockRequirement::Fame(required)
                    if u16::from(fame.half_stars) >= *required
            ) {
                mark_authored_catalogue_definition_target_unlocked(
                    definitions,
                    definition,
                    &mut unlocks,
                    &mut changed,
                );
            }
        }
    }
    for completed in research_completed.read() {
        for definition in definitions.unlocks() {
            if matches!(
                &definition.requirement,
                UnlockRequirement::Research(required)
                    if required.0 == completed.definition.0
            ) {
                mark_authored_catalogue_definition_target_unlocked(
                    definitions,
                    definition,
                    &mut unlocks,
                    &mut changed,
                );
            }
        }
    }
    for granted in awards_granted.read() {
        for definition in definitions.unlocks() {
            if matches!(
                &definition.requirement,
                UnlockRequirement::Award(required)
                    if required.0 == granted.definition.0
            ) {
                mark_authored_catalogue_definition_target_unlocked(
                    definitions,
                    definition,
                    &mut unlocks,
                    &mut changed,
                );
            }
        }
    }
}

fn mark_authored_catalogue_definition_target_unlocked(
    definitions: WorldDefinitionsView<'_>,
    definition: &openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::UnlockDefinition,
    unlocks: &mut UnlockedCatalogueDefinitionSet,
    changed: &mut MessageWriter<ProgressionFactChanged>,
) {
    let target = definition.target;
    if let Some(index) = find_catalogue_definition_index(definitions, target) {
        if mark_catalogue_definition_unlocked_at_index(unlocks, index) == Some(true) {
            changed.write(ProgressionFactChanged::Unlock(target));
        }
    }
}
