use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};
use openzt2_interface_state::{
    photos::photo_challenge_types::{
        CurrentPhotoChallengeSet, PhotoChallenge, PhotoChallengeCatalogueIndex,
        PhotoChallengeProgress, PhotoChallengeSet,
    },
    ui::{
        authored_reusable_list_and_table_runtime_types::{
            RequestedUiListRowCount, UiListPolicy, UiListRow,
        },
        authored_ui_node_projection_components::{UiDocumentMembers, UiNodeId},
    },
};
use openzt2_ui::animation::UiShowHideAnimation;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldRoot},
};

use super::photo_mode_presentation_lifecycle::{PhotoChallengeGoalList, PhotoChallengeGoals};

#[derive(Resource)]
pub(super) struct PhotoChallengeItemFragment(Handle<UiDocumentAsset>);

#[derive(Component, PartialEq, Eq)]
pub(super) struct PhotoChallengeGoalRow(Entity);

pub(super) fn load_photo_challenge_item_fragment(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(PhotoChallengeItemFragment(
        server.load("ui/layout/photochallengeitem.xml"),
    ));
}

pub(super) fn project_selected_photo_challenge_goals(
    worlds: Query<
        (Entity, Ref<PhotoChallengeCatalogueIndex>),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    selected: Query<(
        &WorldMember,
        Ref<PhotoChallengeSet>,
        Ref<CurrentPhotoChallengeSet>,
    )>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    challenges: Query<(Ref<PhotoChallenge>, Ref<PhotoChallengeProgress>)>,
    fallback_fragment: Res<PhotoChallengeItemFragment>,
    documents: Res<Assets<UiDocumentAsset>>,
    localizations: Res<Assets<LocalizationAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    mut lists: Query<
        (
            &mut UiListPolicy,
            &mut RequestedUiListRowCount,
            Option<&Children>,
        ),
        With<PhotoChallengeGoalList>,
    >,
    mut rows: Query<(
        Ref<UiListRow>,
        &UiDocumentMembers,
        Option<&mut PhotoChallengeGoalRow>,
    )>,
    mut fields: Query<
        (
            &Name,
            &UiNodeId,
            Option<&mut Text>,
            Option<&mut Node>,
            &mut Visibility,
        ),
        Without<PhotoChallengeGoals>,
    >,
    mut goals: Query<
        (&mut Visibility, Option<&mut UiShowHideAnimation>),
        With<PhotoChallengeGoals>,
    >,
    mut commands: Commands,
) {
    if goals.is_empty() && lists.is_empty() {
        return;
    }
    let source = worlds.single().ok().and_then(|(world, catalogue)| {
        let (_, set, selection) = selected.iter().find(|(member, ..)| member.root == world)?;
        let set_record = scenarios
            .get(&set.source)?
            .document
            .find_photo_challenge_set(set.id)?;
        let changed = set.is_changed() || selection.is_changed() || catalogue.is_changed();
        Some((catalogue, set_record, changed))
    });
    let members = source
        .as_ref()
        .map_or(&[][..], |(_, set, _)| set.members.as_slice());
    let count = u16::try_from(members.len()).unwrap_or(u16::MAX);
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    let data_changed = scenarios.is_changed()
        || documents.is_changed()
        || localizations.is_changed()
        || active_localization.is_changed()
        || source.as_ref().is_some_and(|(_, _, changed)| *changed);
    let mut populated = false;
    for (mut policy, mut requested, children) in &mut lists {
        if policy.row_document.is_none() {
            policy.row_document = Some(fallback_fragment.0.clone());
        }
        requested.set_if_neq(RequestedUiListRowCount(Some(count)));
        let fragment = policy
            .row_document
            .as_ref()
            .and_then(|handle| documents.get(handle));
        for row_entity in children.into_iter().flat_map(RelationshipTarget::iter) {
            let (Some((catalogue, _, _)), Ok((row, fields_in_row, current))) =
                (source.as_ref(), rows.get_mut(row_entity))
            else {
                continue;
            };
            let Some(goal) = members
                .get(usize::from(row.index))
                .and_then(|member| catalogue.rules.get(&member.challenge))
                .copied()
            else {
                continue;
            };
            let Ok((challenge, progress)) = challenges.get(goal) else {
                continue;
            };
            let Some(rule) = scenarios
                .get(&challenge.source)
                .and_then(|asset| asset.document.find_photo_challenge_rule(challenge.id))
            else {
                continue;
            };
            populated = true;
            let changed = if let Some(mut current) = current {
                current.set_if_neq(PhotoChallengeGoalRow(goal))
            } else {
                commands
                    .entity(row_entity)
                    .insert(PhotoChallengeGoalRow(goal));
                true
            };
            if !changed
                && !data_changed
                && !row.is_changed()
                && !challenge.is_changed()
                && !progress.is_changed()
            {
                continue;
            }
            let key = if progress.completed {
                rule.completion_key
            } else {
                rule.instruction_key
            };
            let caption = localization
                .as_ref()
                .and_then(|localization| localization.find_plain_localized_text(key))
                .unwrap_or_default();
            for entity in fields_in_row.iter() {
                let Ok((name, node_id, text, node, mut visibility)) = fields.get_mut(entity) else {
                    continue;
                };
                match name.as_str() {
                    "challenge description" => {
                        if let Some(mut text) = text {
                            if text.as_str() != caption {
                                text.0.clear();
                                text.0.push_str(caption);
                            }
                        }
                    }
                    "incompletelabel" => {
                        visibility.set_if_neq(if progress.completed {
                            Visibility::Hidden
                        } else {
                            Visibility::Inherited
                        });
                    }
                    "stars" => {
                        visibility.set_if_neq(if progress.completed && progress.rating_stars > 0 {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        });
                        let authored_width = fragment
                            .and_then(|fragment| {
                                fragment
                                    .canonical_ui_document()
                                    .nodes
                                    .get(node_id.index as usize)
                            })
                            .map(|record| record.rect[2]);
                        if let (Some(mut node), Some(width)) = (node, authored_width) {
                            let width =
                                Val::Px(width * f32::from(progress.rating_stars.min(5)) / 5.0);
                            if node.width != width {
                                node.width = width;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    for (mut visibility, animation) in &mut goals {
        let visible = count > 0 && populated;
        if visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }) {
            if let Some(mut animation) = animation {
                animation.start_authored_visibility_transition(visible);
            }
        }
    }
}
