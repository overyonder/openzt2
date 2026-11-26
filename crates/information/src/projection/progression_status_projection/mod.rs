use bevy::{
    ecs::{entity::EntityHashSet, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::progression::award_and_progression_fact_types::EarnedProgressionAward;
use openzt2_zoo_state::progression::research_types::ResearchProject;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use super::super::projection::text_replacement_operations::replace_projected_ui_text_if_changed;
use openzt2_progression::progression::{AuthoredAwardName, ResearchPresentationProject};
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldMembers, WorldRoot},
};

#[derive(Component, Default, PartialEq, Eq)]
pub(crate) struct ProgressionAwardSources {
    by_id: std::collections::BTreeMap<AssetId, Entity>,
    latest: Option<Entity>,
}

pub(crate) fn bind_progression_awards_to_world_status(
    mut roots: Query<
        (Entity, Option<&WorldMembers>, &mut ProgressionAwardSources),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    changed: Query<
        &WorldMember,
        (
            With<EarnedProgressionAward>,
            Or<(Changed<EarnedProgressionAward>, Changed<WorldMember>)>,
        ),
    >,
    earned: Query<(Entity, &EarnedProgressionAward)>,
    mut retired: RemovedComponents<EarnedProgressionAward>,
) {
    let affected: EntityHashSet = changed.iter().map(|member| member.root).collect();
    let awards_removed = !retired.is_empty();
    retired.clear();
    for (root, members, mut sources) in &mut roots {
        if !awards_removed && !affected.contains(&root) && !sources.is_added() {
            continue;
        }
        let mut next = ProgressionAwardSources::default();
        let mut running = std::collections::BTreeMap::<AssetId, (u64, Entity)>::new();
        let mut latest = None;
        for (entity, award) in earned
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
        {
            let candidate = (award.earned_tick, entity);
            let entry = running.entry(award.definition).or_insert(candidate);
            *entry = (*entry).max(candidate);
            latest = Some(latest.map_or(candidate, |latest: (u64, Entity)| latest.max(candidate)));
        }
        next.latest = latest.map(|(_, entity)| entity);
        next.by_id = running
            .into_iter()
            .map(|(id, (_, entity))| (id, entity))
            .collect();
        sources.set_if_neq(next);
    }
}

// Progression bindings combine calendar, fame, awards and rating values.
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_progression_status_to_authored_information_bindings(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    unlocked_catalogue_definitions: Single<Ref<UnlockedCatalogueDefinitionSet>>,
    research_selection: Query<
        (
            Ref<ResearchPresentationProject>,
            Ref<ProgressionAwardSources>,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    research_projects: Query<Ref<ResearchProject>>,
    earned_progression_awards: Query<(Ref<EarnedProgressionAward>, Option<Ref<AuthoredAwardName>>)>,
    mut authored_text: Query<(Ref<UiTextBinding>, &mut Text)>,
    mut authored_visibility: Query<(Ref<UiVisibleBinding>, &mut Visibility)>,
    mut authored_values: Query<(Ref<UiValueBinding>, &mut UiValue)>,
) {
    let unlocked_catalogue_definitions = unlocked_catalogue_definitions.into_inner();
    let Ok((selection, awards)) = research_selection.single() else {
        return;
    };
    let active_research_project = selection
        .0
        .and_then(|entity| research_projects.get(entity).ok());
    let research_changed = selection.is_changed()
        || active_research_project
            .as_ref()
            .is_some_and(Ref::is_changed);
    let latest_earned_award = awards
        .latest
        .and_then(|entity| earned_progression_awards.get(entity).ok());
    let sources_changed =
        active_world_definitions.is_changed() || world_definition_assets.is_changed();
    let locale_changed = active_localization.is_changed() || localization_assets.is_changed();

    for (property_binding, mut projected_visibility) in &mut authored_visibility {
        let should_be_visible = match &property_binding.0 {
            UiBooleanPropertyBindingSource::ResearchActive
                if research_changed || property_binding.is_changed() =>
            {
                active_research_project.is_some()
            }
            UiBooleanPropertyBindingSource::AwardEarned { award }
                if awards.is_changed() || property_binding.is_changed() =>
            {
                awards.by_id.contains_key(award)
            }
            UiBooleanPropertyBindingSource::DefinitionUnlocked { definition }
                if sources_changed
                    || unlocked_catalogue_definitions.is_changed()
                    || property_binding.is_changed() =>
            {
                active_world_definitions
                    .get(&world_definition_assets)
                    .and_then(|world_definitions| {
                        world_definitions
                            .catalogue_entry_for_definition(*definition)
                            .map(|(index, _)| index)
                    })
                    .is_some_and(|catalogue_index| {
                        catalogue_definition_is_unlocked(
                            &unlocked_catalogue_definitions,
                            catalogue_index,
                        )
                    })
            }
            _ => continue,
        };
        projected_visibility.set_if_neq(if should_be_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }

    for (property_binding, mut projected_value) in &mut authored_values {
        let value = match &property_binding.0 {
            UiIntegerPropertyBindingSource::ResearchElapsedTicks
                if research_changed || property_binding.is_changed() =>
            {
                active_research_project
                    .as_ref()
                    .and_then(|project| i64::try_from(project.elapsed_ticks).ok())
                    .unwrap_or(0)
            }
            UiIntegerPropertyBindingSource::ResearchRequiredTicks
                if research_changed || property_binding.is_changed() =>
            {
                active_research_project
                    .as_ref()
                    .and_then(|project| i64::try_from(project.required_ticks).ok())
                    .unwrap_or(0)
            }
            UiIntegerPropertyBindingSource::AwardEarnedTick { award } => {
                let earned = awards
                    .by_id
                    .get(award)
                    .and_then(|&entity| earned_progression_awards.get(entity).ok());
                if !awards.is_changed()
                    && !property_binding.is_changed()
                    && !earned.as_ref().is_some_and(|(award, _)| award.is_changed())
                {
                    continue;
                }
                earned
                    .and_then(|(award, _)| i64::try_from(award.earned_tick).ok())
                    .unwrap_or_default()
            }
            _ => continue,
        };
        projected_value.set_if_neq(UiValue(value));
    }

    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(localization_catalogue) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    for (property_binding, projected_text) in &mut authored_text {
        let localization_key = match &property_binding.0 {
            UiTextPropertyBindingSource::ResearchName
                if research_changed
                    || sources_changed
                    || locale_changed
                    || property_binding.is_changed() =>
            {
                active_research_project
                    .as_ref()
                    .and_then(|project| world_definitions.find_research(project.definition))
                    .map(|record| AssetId(record.name_key.0))
            }
            UiTextPropertyBindingSource::LatestAwardName
                if awards.is_changed()
                    || sources_changed
                    || locale_changed
                    || property_binding.is_changed() =>
            {
                latest_earned_award.as_ref().and_then(|(award, name)| {
                    name.as_ref()
                        .map(|name| name.0)
                        .filter(|&id| id != AssetId::default())
                        .or_else(|| {
                            world_definitions
                                .find_award(award.definition)
                                .map(|record| record.name_key)
                        })
                })
            }
            _ => continue,
        };
        let localized_text = localization_key
            .and_then(|key| localization_catalogue.find_plain_localized_text(key))
            .unwrap_or_default();
        replace_projected_ui_text_if_changed(projected_text, localized_text);
    }
}

fn catalogue_definition_is_unlocked(
    unlocked_catalogue_definitions: &UnlockedCatalogueDefinitionSet,
    catalogue_index: usize,
) -> bool {
    unlocked_catalogue_definitions
        .words
        .get(catalogue_index / 64)
        .is_some_and(|word| word & (1_u64 << (catalogue_index % 64)) != 0)
}
