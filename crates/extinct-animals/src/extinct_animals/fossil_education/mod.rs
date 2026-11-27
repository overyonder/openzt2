use std::{io, time::Duration};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
        ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::{
    information::award_presentation_types::AwardProgramEffect,
    ui::notification_contracts::{NotificationMessage, ShowNotification},
};
use openzt2_world_state::{
    application_schedule::FixedGameSet,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    authored_float_attributes::AuthoredFloatAttributes,
    extinct_animals::extinct_animal_observable_fact_types::FossilEducationProgress,
};

const CENTER_BINDER: &str = "entities/objects/buildings/ai/fossileducationcenter.xml";

#[derive(Asset, TypePath)]
struct FossilSkeletonRewards {
    attribute: AssetId,
    delay: Duration,
    message: Option<AssetId>,
    rewards: Vec<AssetId>,
}

#[derive(Resource)]
struct SkeletonRewardSource(Handle<FossilSkeletonRewards>);

#[derive(Default, TypePath)]
struct SkeletonRewardLoader;

impl AssetLoader for SkeletonRewardLoader {
    type Asset = FossilSkeletonRewards;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> io::Result<FossilSkeletonRewards> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let document = parse_blue_fang_source_document(AssetPath::new(CENTER_BINDER), &bytes)
            .map_err(io::Error::other)?;
        let component = find_node(&document.root, "ZTFossilSkeletonComponent")
            .ok_or_else(|| io::Error::other("fossil skeleton reward component is missing"))?;
        let attribute = component
            .attribute_named_any(&["entityAttribute"])
            .map(lower_key)
            .ok_or_else(|| io::Error::other("fossil skeleton reward attribute is missing"))?;
        let delay = component
            .attribute_named_any(&["rewardDelay"])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f64>)
            .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
            .ok_or_else(|| io::Error::other("fossil skeleton reward delay is invalid"))?;
        let message = component
            .attribute_named_any(&["rewardMessage"])
            .map(lower_key);
        let rewards = find_node(component, "rewardObjects")
            .into_iter()
            .flat_map(OrderedSourceDocumentNode::element_children)
            .map(|object| lower_key(object.name.as_str()))
            .collect();
        Ok(FossilSkeletonRewards {
            attribute,
            delay,
            message,
            rewards,
        })
    }
}

fn lower_key(value: &str) -> AssetId {
    AssetId::from_key(&value.trim().to_ascii_lowercase())
}

fn find_node<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    if node.name.eq_ignore_ascii_case(name) {
        return Some(node);
    }
    node.element_children()
        .find_map(|child| find_node(child, name))
}

#[derive(Component, Clone, Debug)]
pub struct FossilSkeletonRewardTimer {
    pub timer: Timer,
    pub reward: AssetId,
    pub message: Option<AssetId>,
}

pub(super) fn register(app: &mut App) {
    app.init_asset::<FossilSkeletonRewards>()
        .init_asset_loader::<SkeletonRewardLoader>()
        .add_message::<AwardProgramEffect>()
        .add_message::<ShowNotification>()
        .add_systems(
            Startup,
            |assets: Res<AssetServer>, mut commands: Commands| {
                commands.insert_resource(SkeletonRewardSource(assets.load(CENTER_BINDER)));
            },
        )
        .add_systems(
            FixedUpdate,
            (
                award_finished_skeleton_rewards,
                project_center_bone_levels.after(award_finished_skeleton_rewards),
            )
                .in_set(FixedGameSet::Act)
                .run_if(in_state(GamePhase::InGame)),
        );
}

fn project_center_bone_levels(
    mut commands: Commands,
    active: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    source: Res<SkeletonRewardSource>,
    policies: Res<Assets<FossilSkeletonRewards>>,
    mut work: ParamSet<(
        Query<Entity, (With<WorldMember>, With<DefinitionId>)>,
        Query<
            Entity,
            (
                With<WorldMember>,
                Or<(Changed<DefinitionId>, Changed<AuthoredFloatAttributes>)>,
            ),
        >,
        Query<
            (
                &DefinitionId,
                Option<&mut AuthoredFloatAttributes>,
                Option<&mut FossilEducationProgress>,
                Has<FossilSkeletonRewardTimer>,
            ),
            With<WorldMember>,
        >,
    )>,
) {
    let Some(view) = active.get(&definitions) else {
        return;
    };
    let Some(policy) = policies.get(&source.0) else {
        return;
    };
    let center = lower_key("FossilEducationCenter");
    let levels_key = lower_key("f_NumLevels");
    let assets_changed = active.is_changed() || definitions.is_changed() || policies.is_changed();
    let changed = if assets_changed {
        work.p0().iter().collect::<Vec<_>>()
    } else {
        work.p1().iter().collect()
    };
    let mut centers = work.p2();
    for entity in changed {
        let Ok((definition, attributes, progress, waiting)) = centers.get_mut(entity) else {
            continue;
        };
        let Some(object) = view.find_object(definition.0).filter(|object| {
            object.id == center || object.behavior_type_identifiers.contains(&center)
        }) else {
            if progress.is_some() {
                commands
                    .entity(entity)
                    .remove::<(FossilEducationProgress, FossilSkeletonRewardTimer)>();
            }
            continue;
        };
        let level_count = object
            .authored_runtime
            .numeric_behavior_properties
            .iter()
            .find_map(|&(key, value)| {
                (key == levels_key && value.is_finite() && value >= 1.0).then_some(value)
            });
        let Some(level_count) = level_count else {
            continue;
        };
        let level = attributes
            .as_ref()
            .and_then(|values| values.get(policy.attribute))
            .unwrap_or_default();
        let old_level = progress
            .as_ref()
            .map_or(0.0, |progress| progress.bone_level);
        if waiting {
            if let Some(mut attributes) = attributes {
                if level != old_level {
                    attributes.set(policy.attribute, old_level);
                }
            }
            continue;
        }
        let next = FossilEducationProgress { bone_level: level };
        if let Some(mut progress) = progress {
            progress.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
        if level != old_level {
            if let Some(&reward) = completed_skeleton_index(level, level_count)
                .and_then(|index| policy.rewards.get(index))
            {
                commands.entity(entity).insert(FossilSkeletonRewardTimer {
                    timer: Timer::new(policy.delay, TimerMode::Once),
                    reward,
                    message: policy.message,
                });
            }
        }
    }
}

fn completed_skeleton_index(level: f32, levels_per_skeleton: f32) -> Option<usize> {
    (level >= 0.0 && level.rem_euclid(levels_per_skeleton) == levels_per_skeleton - 1.0)
        .then_some((level / levels_per_skeleton) as usize)
}

fn award_finished_skeleton_rewards(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut centers: Query<(Entity, &WorldMember, &mut FossilSkeletonRewardTimer)>,
    mut rewards: MessageWriter<AwardProgramEffect>,
    mut notices: MessageWriter<ShowNotification>,
) {
    for (entity, member, mut reward) in &mut centers {
        if reward.timer.tick(time.delta()).is_finished() {
            rewards.write(AwardProgramEffect::PlaceCratedObject {
                root: member.root,
                definition: reward.reward,
            });
            if let Some(message) = reward.message {
                notices.write(ShowNotification {
                    operation: entity,
                    message: NotificationMessage::Localized(message),
                });
            }
            commands
                .entity(entity)
                .remove::<FossilSkeletonRewardTimer>();
        }
    }
}
