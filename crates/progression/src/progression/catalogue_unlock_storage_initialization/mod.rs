use super::research_types::{ResearchItemInWorld, ResearchProjectAvailability, WorldResearchItems};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::UnlockRequirement;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::{
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::mark_catalogue_definition_unlocked_at_index,
    unlock_types::{CatalogueUnlockStorageInitialized, UnlockedCatalogueDefinitionSet},
};

pub(super) fn initialize_catalogue_unlock_storage_from_active_world_definitions(
    mut commands: Commands,
    mut roots: Query<
        (Entity, &mut UnlockedCatalogueDefinitionSet),
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            Without<CatalogueUnlockStorageInitialized>,
        ),
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    profile_variables: Option<
        Res<openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables>,
    >,
) {
    let Ok((root, mut unlocked_catalogue_definitions)) = roots.single_mut() else {
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
        let unlocked = match authored_unlock.requirement {
            UnlockRequirement::Always => true,
            UnlockRequirement::Profile(key) => profile_variables
                .as_deref()
                .is_some_and(|variables| variables.get(key) == Some("true")),
            _ => false,
        };
        if unlocked {
            let target = authored_unlock.target;
            if let Some(index) = find_catalogue_definition_index(definitions, target) {
                let _ = mark_catalogue_definition_unlocked_at_index(
                    &mut unlocked_catalogue_definitions,
                    index,
                );
            }
        }
    }
    commands
        .entity(root)
        .insert(CatalogueUnlockStorageInitialized);
}

pub(super) fn reconcile_authored_research_items(
    mut commands: Commands,
    mut roots: Query<
        (
            Entity,
            &mut PersistentIdAllocator,
            Option<&mut WorldResearchItems>,
        ),
        (With<WorldLoadCompleted>, With<WorldRoot>),
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    research_availability: Query<(
        Entity,
        &WorldMember,
        &ResearchProjectAvailability,
        Has<ResearchItemInWorld>,
    )>,
) {
    let definitions_changed = active_definitions.is_changed() || definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (root, mut identifiers, mut current) in &mut roots {
        if !definitions_changed && current.is_some() {
            continue;
        }
        let mut existing: std::collections::BTreeMap<_, _> =
            if let Some(current) = current.as_deref() {
                current
                    .definitions
                    .iter()
                    .filter(|&(_, &item)| research_availability.contains(item))
                    .map(|(&definition, &item)| (definition, item))
                    .collect()
            } else {
                research_availability
                    .iter()
                    .filter(|(_, world, ..)| world.root == root)
                    .map(|(entity, _, item, _)| (item.item, entity))
                    .collect::<std::collections::BTreeMap<_, _>>()
            };
        let new_count = definitions
            .research()
            .filter(|research| !existing.contains_key(&research.id))
            .count();
        if u64::try_from(new_count)
            .ok()
            .and_then(|count| identifiers.next().checked_add(count))
            .is_none()
        {
            continue;
        }
        let mut items = WorldResearchItems::default();
        for research in definitions.research() {
            let item = if let Some(item) = existing.remove(&research.id) {
                if research_availability
                    .get(item)
                    .is_ok_and(|(_, _, _, linked)| !linked)
                {
                    commands.entity(item).insert(ResearchItemInWorld(root));
                }
                item
            } else {
                let Ok(identifier) = identifiers.allocate() else {
                    return;
                };
                commands
                    .spawn((
                        ResearchProjectAvailability {
                            item: research.id,
                            available: false,
                        },
                        ResearchItemInWorld(root),
                        WorldMember { root },
                        identifier,
                    ))
                    .id()
            };
            items.definitions.insert(research.id, item);
            for &target in &research.unlocks {
                items.unlocks.entry(target).or_default().push(item);
            }
        }
        for retired in existing.into_values() {
            commands.entity(retired).try_despawn();
        }
        if let Some(current) = current.as_mut() {
            current.set_if_neq(items);
        } else {
            commands.entity(root).insert(items);
        }
    }
}
