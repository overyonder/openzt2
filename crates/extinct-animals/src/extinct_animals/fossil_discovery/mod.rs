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
        persistent_id_types::PersistentIdAllocator,
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
};

use super::fossil_definition_index::FossilDefinitionIndex;
use crate::fossil_site_types::{DigPit, FossilSiteKind, RequestedFossilPuzzle};

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct DigSite {
    kind: &'static FossilSiteKind,
    pose: &'static GlobalTransform,
    member: &'static WorldMember,
    random: &'static mut FossilDiscoveryRandomStream,
    definition: &'static mut DefinitionId,
    transform: &'static mut Transform,
    visibility: &'static mut Visibility,
    presentation: Option<&'static mut PrefabPresentation>,
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
        site.definition.set_if_neq(DefinitionId(pit.id));
        site.transform
            .reborrow()
            .map_unchanged(|pose| &mut pose.scale)
            .set_if_neq(Vec3::splat(pit.prefab_scale));
        site.visibility.set_if_neq(Visibility::Inherited);
        let mut entity = commands.entity(request.fossil_site);
        entity
            .remove::<(FossilSiteKind, FossilDiscoveryRandomStream)>()
            .insert(DigPit);
        if let Some(presentation) = site.presentation {
            presentation
                .map_unchanged(|presentation| &mut presentation.0)
                .set_if_neq(pit_presentation.clone());
        } else {
            entity.insert(PrefabPresentation::new(pit_presentation.clone()));
        }
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
                    if awarded == set.pieces_per_location.max(1) {
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
                    commands.trigger(FossilFindFeedback {
                        root,
                        kind: FossilFind::Puzzle {
                            object: set.species,
                            complete: set.pieces.iter().all(|piece| found.contains(piece)),
                        },
                    });
                }
            }
            FossilSiteKind::PlaceableObject => {
                let eligible = definitions
                    .objects()
                    .filter(|object| {
                        object.prefab != AssetId::default()
                            && policy.placeable_objects.iter().any(|id| {
                                object.id == *id || object.behavior_type_identifiers.contains(id)
                            })
                    })
                    .collect::<Vec<_>>();
                let Ok(count) = u32::try_from(eligible.len()) else {
                    continue;
                };
                let Some(choice) = site.random.0.range_u32(count) else {
                    continue;
                };
                let Some(object) = eligible.get(choice as usize) else {
                    continue;
                };
                let Some(prefab) = definitions.scene(object.prefab) else {
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
            FossilSiteKind::Cash if !policy.non_placeable_objects.is_empty() => {
                let Some(dollars) = site.random.0.range_u32(21).map(|draw| i64::from(draw) + 5)
                else {
                    continue;
                };
                let operation = commands
                    .spawn((
                        WorldMember { root },
                        PendingFossilCashReward(dollars),
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
            FossilSiteKind::Cash => {}
        }
    }
}

#[derive(Component)]
pub(super) struct PendingFossilCashReward(i64);

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
                kind: FossilFind::Cash(reward.0),
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
    Puzzle { object: AssetId, complete: bool },
    Object(AssetId),
    Cash(i64),
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
    let (key, argument) = match event.kind {
        FossilFind::Puzzle { object, complete } => {
            let key = if complete {
                "fpfossildisplay:puzzle_ready"
            } else {
                "fpfossildisplay:piece_found"
            };
            let name = definitions
                .get(&definition_assets)
                .and_then(|view| view.find_object(object))
                .and_then(|object| localization.find_plain_localized_text(object.name_key))
                .unwrap_or_default();
            (key, LocalizationFormatArgument::Text(name))
        }
        FossilFind::Object(name) => (
            "fpfossildisplay:placeable_object_found",
            LocalizationFormatArgument::Text(
                localization
                    .find_plain_localized_text(name)
                    .unwrap_or_default(),
            ),
        ),
        FossilFind::Cash(dollars) => (
            "fpfossildisplay:nonplaceable_object_found",
            LocalizationFormatArgument::Integer(dollars),
        ),
        FossilFind::Tier(unlocked) => (
            if unlocked {
                "fpfossildisplay:next_level_unlocked"
            } else {
                "fpfossildisplay:next_level_unavailable"
            },
            LocalizationFormatArgument::Text(""),
        ),
    };
    let mut text = String::new();
    if localization
        .write_localized_text_with_format_arguments(AssetId::from_key(key), &[argument], &mut text)
        .is_ok()
    {
        messages.messages.push(ZooMessage {
            text,
            expires_at_seconds: time.elapsed_secs_f64() + 10.0,
        });
    }
}
