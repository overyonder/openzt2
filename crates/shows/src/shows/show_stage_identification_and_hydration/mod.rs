use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::{
    donations::donation_opportunity_types::DonationOpportunity,
    economy::money_types::Money,
};

use super::show_stage_types::{ShowStage, ShowStageOpenState};

pub(super) fn hydrate_show_stages_from_active_world_definitions(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut stages: Query<(
        Entity,
        &DefinitionId,
        Option<&mut ShowStage>,
        Has<ShowStageOpenState>,
        Option<&DonationOpportunity>,
    )>,
    changed_objects: Query<Entity, Changed<DefinitionId>>,
    mut removed_definitions: RemovedComponents<DefinitionId>,
    retired_stages: Query<Option<&DonationOpportunity>, (With<ShowStage>, Without<DefinitionId>)>,
) {
    let Some(definitions) = active_definitions.get(&assets) else {
        return;
    };
    let donation_category = AssetId::from_key("show-donation");
    for entity in removed_definitions.read() {
        let Ok(donation) = retired_stages.get(entity) else {
            continue;
        };
        let mut entity = commands.entity(entity);
        entity.try_remove::<ShowStage>();
        if donation.is_some_and(|donation| donation.category == donation_category) {
            entity.try_remove::<DonationOpportunity>();
        }
    }
    let refresh_all = active_definitions.is_changed();
    let dirty = if refresh_all {
        stages.iter().map(|(entity, ..)| entity).collect::<Vec<_>>()
    } else {
        changed_objects.iter().collect::<Vec<_>>()
    };
    let mut stages = stages.iter_many_mut(dirty);
    while let Some(row) = stages.fetch_next() {
        let Ok((entity, definition_id, stage, has_open_state, donation)) = row else {
            continue;
        };
        let authored = stage
            .as_deref()
            .and_then(|stage| definitions.find_show_stage(stage.show_stage_definition))
            .filter(|definition| definition.object == definition_id.0)
            .or_else(|| definitions.find_show_stage_definition_for_world_object(definition_id.0));
        let Some(definition) = authored else {
            if stage.is_some() {
                let mut entity = commands.entity(entity);
                entity.try_remove::<ShowStage>();
                if donation.is_some_and(|donation| donation.category == donation_category) {
                    entity.try_remove::<DonationOpportunity>();
                }
            }
            continue;
        };
        let next = ShowStage {
            show_stage_definition: definition.id,
        };
        if let Some(mut stage) = stage {
            stage.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
        if donation.is_none() {
            commands.entity(entity).insert(DonationOpportunity {
                category: donation_category,
                amount: Money::ZERO,
                interaction_ticks: 0,
            });
        }
        if !has_open_state {
            commands.entity(entity).insert(ShowStageOpenState::Open);
        }
    }
}
