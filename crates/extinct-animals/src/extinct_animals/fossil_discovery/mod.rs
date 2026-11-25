use bevy::{
    ecs::{
        entity::{EntityHashMap, EntityHashSet},
        query::QueryData,
        relationship::RelationshipTarget,
    },
    platform::collections::HashSet,
    prelude::*,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{WorldDefinitions, WorldDefinitionsView};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{
    AssetId,
    world_definitions::{extinct_animal_recovery::FossilSetDefinition, immersive_mode_policy::ImmersiveModeKind},
};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::{
    ActiveImmersiveMode, EquippedInteractionTool, ImmersiveCamera, ImmersiveMode,
};
use openzt2_world_state::{
    simulation_time::deterministic_random_stream::DeterministicRng,
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        prefab_presentation_types::PrefabPresentation,
        world_membership_types::{DefinitionId, WorldMember},
    },
};
use openzt2_zoo_state::{
    economy::{
        account_transaction_types::{Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest},
        general_economy_types::GeneralEconomyTransaction,
        money_types::Money,
    },
    extinct_animals::{
        fossil_collection_and_assembly_types::{CollectedFossilPiece, FossilPieceOfWorld, WorldFossilPieces},
        fossil_recovery_types::{DiscoverFossilAtSiteRequest, FossilDiscoveryLevel, FossilDiscoveryRandomStream},
    },
    placement::object_placement_definition_queries::object_placement_prefab_identifier,
};

use super::fossil_definition_index::{FossilDefinitionIndex, FossilRewardCandidate};
use crate::fossil_site_types::{DigPit, FossilSiteKind, RequestedFossilPuzzle};

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct DigSite {
    kind: &'static FossilSiteKind,
    pose: &'static GlobalTransform,
    member: &'static WorldMember,
    id: &'static PersistentId,
    random: &'static mut FossilDiscoveryRandomStream,
}

type FossilDigZoo = (
    &'static FossilDiscoveryLevel,
    Option<&'static RequestedFossilPuzzle>,
    Option<&'static WorldFossilPieces>,
    &'static mut PersistentIdAllocator,
);

#[allow(clippy::too_many_arguments)]
pub(super) fn discover_requested_fossils(
    mut requests: MessageReader<DiscoverFossilAtSiteRequest>,
    modes: Query<(
        Entity,
        &ActiveImmersiveMode,
        &ImmersiveCamera,
        &EquippedInteractionTool,
    )>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    index: Res<FossilDefinitionIndex>,
    cameras: Query<&GlobalTransform, With<ZooCamera>>,
    mut sites: Query<DigSite>,
    mut zoos: Query<FossilDigZoo>,
    pieces: Query<&CollectedFossilPiece>,
    mut commands: Commands,
) {
    let Ok((controller, mode, immersive_camera, tool)) = modes.single() else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    if mode.mode != ImmersiveMode::FossilSearch
        || !definitions
            .find_immersive_mode_policy(tool.policy)
            .is_some_and(|policy| policy.mode == ImmersiveModeKind::FossilSearch)
    {
        return;
    }
    let (Some(policy), Ok(camera), Some(pit)) = (
        definitions.fossil_placement(),
        cameras.get(immersive_camera.0),
        definitions.find_object(AssetId::from_key("digpit")),
    ) else {
        return;
    };
    let Some(pit_presentation) = definitions.scene(pit.prefab) else {
        return;
    };

    let mut dug = EntityHashSet::default();
    let mut collections = EntityHashMap::<HashSet<AssetId>>::default();
    for request in requests.read() {
        if request.interaction_tool != controller || dug.contains(&request.fossil_site) {
            continue;
        }
        let Ok(mut site) = sites.get_mut(request.fossil_site) else {
            continue;
        };
        let position = site.pose.translation();
        if !super::fossil_dig_target_geometry::is_fossil_site_within_authored_dig_area(
            camera,
            position,
            policy.dig_distance_m,
        ) {
            continue;
        }
        let root = site.member.root;
        let Ok((level, requested, collection, mut ids)) = zoos.get_mut(root) else {
            continue;
        };
        let kind = *site.kind;
        dug.insert(request.fossil_site);
        commands.entity(request.fossil_site).try_despawn();
        commands.spawn((
            *site.member,
            *site.id,
            DefinitionId(pit.id),
            site.pose.compute_transform().with_scale(Vec3::splat(pit.prefab_scale)),
            Visibility::Inherited,
            PrefabPresentation::new(pit_presentation.clone()),
            DigPit,
        ));
        match kind {
            FossilSiteKind::Puzzle => {
                let found = collections.entry(root).or_insert_with(|| {
                    pieces
                        .iter_many(collection.into_iter().flat_map(RelationshipTarget::iter))
                        .flatten()
                        .map(|piece| piece.0)
                        .collect::<HashSet<_>>()
                });
                let Some(set) = select_puzzle(
                    definitions,
                    &index,
                    level.0,
                    requested,
                    found,
                    &mut site.random.0,
                ) else {
                    continue;
                };
                let mut awarded = 0;
                for &piece in &set.pieces {
                    if awarded == set.pieces_per_location {
                        break;
                    }
                    if found.contains(&piece) || index.piece(piece).is_none() {
                        continue;
                    }
                    let Ok(id) = ids.allocate() else {
                        break;
                    };
                    commands.spawn((
                        CollectedFossilPiece(piece),
                        FossilPieceOfWorld(root),
                        WorldMember { root },
                        id,
                        Transform::from_translation(position),
                        Visibility::default(),
                    ));
                    found.insert(piece);
                    awarded += 1;
                }
                if awarded > 0 {
                    commands.write_message(super::fossil_find_effect::ShowFossilFindEffect {
                        camera: immersive_camera.0,
                        species: set.species,
                    });
                    commands.trigger(FossilFindFeedback {
                        root,
                        kind: FossilFind::Puzzle {
                            object: set.species,
                            collected: set.pieces.iter().filter(|piece| found.contains(*piece)).count(),
                            total: set.pieces.len(),
                        },
                    });
                }
            }
            FossilSiteKind::PlaceableObject => {
                let Some(object) = draw_reward(&index.placeable_rewards, &mut site.random.0)
                    .and_then(|reward| definitions.find_object(reward)) else {
                    continue;
                };
                let Some(prefab) = object_placement_prefab_identifier(object)
                    .and_then(|prefab| definitions.scene(prefab)) else {
                    continue;
                };
                let Ok(id) = ids.allocate() else {
                    continue;
                };
                commands.spawn((
                    WorldMember { root },
                    DefinitionId(object.id),
                    id,
                    Transform::from_translation(position)
                        .with_scale(Vec3::splat(object.prefab_scale)),
                    Visibility::Inherited,
                    PrefabPresentation::new(prefab),
                ));
                commands.trigger(FossilFindFeedback {
                    root,
                    kind: FossilFind::Object(object.name_key),
                });
            }
            FossilSiteKind::Cash => {
                let Some(reward_name) = draw_reward(&index.cash_rewards, &mut site.random.0)
                    .and_then(|reward| definitions.find_object(reward))
                    .map(|object| object.name_key) else {
                    continue;
                };
                let Some(dollars) = site.random.0.range_u32(21).map(|draw| i64::from(draw) + 5)
                else {
                    continue;
                };
                let operation = commands
                    .spawn((
                        WorldMember { root },
                        PendingFossilCashReward { dollars, reward_name },
                        GeneralEconomyTransaction::CashGrant,
                    ))
                    .id();
                commands.write_message(TransactionRequest {
                    operation,
                    debit: Account::External,
                    credit: Account::Zoo,
                    amount: Money(dollars * 100),
                    kind: TransactionKind::Reward,
                    subject: None,
                });
            }
        }
    }
}

fn draw_reward(groups: &[Vec<FossilRewardCandidate>], random: &mut DeterministicRng) -> Option<AssetId> {
    let group = random.range_u32(u32::try_from(groups.len()).ok()?)?;
    let candidates = groups.get(group as usize)?;
    let first = random.range_u32(u32::try_from(candidates.len()).ok()?)? as usize;
    let candidate = candidates.get(first)?;
    if !candidate.developer_only {
        return Some(candidate.definition);
    }
    let mut candidates = candidates.to_vec();
    candidates.remove(first);
    while let Some(choice) = random.range_u32(u32::try_from(candidates.len()).ok()?) {
        let candidate = candidates.remove(choice as usize);
        if !candidate.developer_only {
            return Some(candidate.definition);
        }
    }
    None
}

#[derive(Component)]
pub(super) struct PendingFossilCashReward {
    dollars: i64,
    reward_name: AssetId,
}

pub(super) fn settle_fossil_cash_feedback(
    rewards: Query<(&WorldMember, &PendingFossilCashReward)>,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    mut commands: Commands,
) {
    for settlement in completed.read() {
        if let Ok((member, reward)) = rewards.get(settlement.operation) {
            commands.trigger(FossilFindFeedback {
                root: member.root,
                kind: FossilFind::Cash { dollars: reward.dollars, reward_name: reward.reward_name },
            });
            commands.entity(settlement.operation).try_despawn();
        }
    }
    for settlement in rejected.read() {
        if rewards.contains(settlement.operation) {
            commands.entity(settlement.operation).try_despawn();
        }
    }
}

fn select_puzzle<'a>(
    definitions: WorldDefinitionsView<'a>,
    index: &FossilDefinitionIndex,
    level: u8,
    requested: Option<&RequestedFossilPuzzle>,
    found: &HashSet<AssetId>,
    random: &mut DeterministicRng,
) -> Option<&'a FossilSetDefinition> {
    let incomplete =
        |set: &FossilSetDefinition| set.pieces.iter().any(|piece| !found.contains(piece));
    if let Some(set) = requested
        .and_then(|requested| definitions.find_fossil_set(requested.0))
        .filter(|set| incomplete(set))
    {
        return Some(set);
    }
    let candidates = index.sets.iter().filter_map(|&id| {
        let set = definitions.find_fossil_set(id)?;
        if set.discovery_level != level || !incomplete(set) {
            return None;
        }
        let found_count = u32::try_from(
            set.pieces
                .iter()
                .filter(|piece| found.contains(*piece))
                .count(),
        )
        .ok()?;
        Some((set, found_count.checked_add(1)?))
    });
    let total = candidates
        .clone()
        .try_fold(0_u32, |total, (_, weight)| total.checked_add(weight))?;
    let mut draw = random.range_u32(total)?;
    for (set, weight) in candidates {
        if draw < weight {
            return Some(set);
        }
        draw -= weight;
    }
    None
}

#[derive(Event)]
pub(super) struct FossilFindFeedback {
    root: Entity,
    kind: FossilFind,
}

impl FossilFindFeedback {
    pub(super) const fn tier(root: Entity, unlocked: bool) -> Self {
        Self {
            root,
            kind: FossilFind::Tier(unlocked),
        }
    }
}

#[derive(Clone, Copy)]
enum FossilFind {
    Puzzle { object: AssetId, collected: usize, total: usize },
    Object(AssetId),
    Cash { dollars: i64, reward_name: AssetId },
    Tier(bool),
}

pub(super) fn present_fossil_find_feedback(
    event: On<FossilFindFeedback>,
    mut messages: Query<&mut openzt2_interface_state::information::zoo_message_types::ZooMessages>,
    localization: Res<openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex>,
    localizations: Res<
        Assets<openzt2_game_assets::localization::localization_asset_types::LocalizationAsset>,
    >,
    time: Res<Time<Real>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
) {
    use openzt2_game_data::localization::LocalizationFormatArgument;
    use openzt2_interface_state::information::zoo_message_types::ZooMessage;

    let (Ok(mut messages), Some(localization)) = (
        messages.get_mut(event.root),
        localization.borrow_loaded_localization_view(&localizations),
    ) else {
        return;
    };
    let progress;
    let (key, arguments) = match event.kind {
        FossilFind::Puzzle { object, collected, total } => {
            let key = if collected == total {
                "fpfossildisplay:puzzle_ready"
            } else {
                "fpfossildisplay:piece_found"
            };
            let name = definitions
                .get(&definition_assets)
                .and_then(|view| view.find_object(object))
                .and_then(|object| localization.find_plain_localized_text(object.name_key))
                .unwrap_or_default();
            progress = format!("{collected}/{total}");
            (key, [LocalizationFormatArgument::Text(name), LocalizationFormatArgument::Text(&progress)])
        }
        FossilFind::Object(name) => (
            "fpfossildisplay:placeable_object_found",
            [LocalizationFormatArgument::Text(
                localization
                    .find_plain_localized_text(name)
                    .unwrap_or_default(),
            ), LocalizationFormatArgument::Text("")],
        ),
        FossilFind::Cash { dollars, reward_name } => (
            "fpfossildisplay:nonplaceable_object_found",
            [LocalizationFormatArgument::Text(localization.find_plain_localized_text(reward_name).unwrap_or_default()), LocalizationFormatArgument::CurrencyCents(dollars * 100)],
        ),
        FossilFind::Tier(unlocked) => (
            if unlocked {
                "fpfossildisplay:next_level_unlocked"
            } else {
                "fpfossildisplay:next_level_unavailable"
            },
            [LocalizationFormatArgument::Text(""), LocalizationFormatArgument::Text("")],
        ),
    };
    let mut text = String::new();
    if localization
        .write_localized_text_with_format_arguments(AssetId::from_key(key), &arguments, &mut text)
        .is_ok()
    {
        messages.messages.push(ZooMessage {
            text,
            expires_at_seconds: time.elapsed_secs_f64() + 10.0,
        });
    }
}
