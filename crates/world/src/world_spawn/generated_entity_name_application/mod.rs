use bevy::prelude::*;
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId,
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{DefinitionId, WorldMember, WorldRoot},
};
use openzt2_zoo_state::entity_naming_types::{EntityNameRegistered, EntityNameTagCounters};

pub(super) fn initialize_world_entity_name_counters(
    mut commands: Commands,
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            Without<EntityNameTagCounters>,
        ),
    >,
) {
    for root in &roots {
        commands
            .entity(root)
            .insert(EntityNameTagCounters::default());
    }
}

#[cfg(test)]
mod tests;

pub(super) fn register_and_name_spawned_world_entities(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    server: Option<Res<AssetServer>>,
    mut roots: Query<&mut EntityNameTagCounters, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    entities: Query<
        (
            Entity,
            &PersistentId,
            &WorldMember,
            &DefinitionId,
            Option<&Name>,
        ),
        Without<EntityNameRegistered>,
    >,
) {
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    if definitions.has_pending_assets()
        || server.as_ref().is_some_and(|server| {
            localization.has_pending_catalogue_loads(server, &localization_assets)
        })
    {
        return;
    }
    let Some(localization) = localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let mut pending = entities.iter().collect::<Vec<_>>();
    pending.sort_unstable_by_key(|(_, id, member, _, _)| (member.root, id.0));
    for (entity, _, member, definition, name) in pending {
        let Ok(mut counters) = roots.get_mut(member.root) else {
            continue;
        };
        let Some(naming) = definitions.find_entity_naming(definition.0) else {
            continue;
        };
        counters.record_entity_type_tags(&naming.counted_tags);
        let mut target = commands.entity(entity);
        target.insert(EntityNameRegistered);
        if name.is_some() {
            continue;
        }
        let Some(generated_name) = naming.generated_name else {
            continue;
        };
        let Some(label) = localization
            .find_plain_localized_text(generated_name.localization_key)
            .filter(|label| !label.is_empty())
        else {
            continue;
        };
        let number = counters.value(generated_name.counter_tag).unwrap_or(0);
        let text = match i32::try_from(number) {
            Ok(number) if number > 0 => format!("{label} {number}"),
            _ => label.to_owned(),
        };
        target.insert(Name::new(text));
    }
}
