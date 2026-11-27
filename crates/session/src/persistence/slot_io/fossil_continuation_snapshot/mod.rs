use openzt2_extinct_animals::extinct_animals::FossilAssemblyPiecePose;
use std::collections::{BTreeMap, BTreeSet};

use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::deterministic_random_stream::DeterministicRng,
    world_spawn::{
        persistent_id_index::PersistentIdIndex,
        persistent_id_types::PersistentId,
        prefab_presentation_types::PrefabPresentation,
        world_membership_types::{DefinitionId, WorldMember, WorldMembers},
    },
};
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::{
    DigPit, FossilSiteKind, RequestedFossilPuzzle,
};
use openzt2_zoo_state::extinct_animals::{
    fossil_collection_and_assembly_types::{
        CollectedFossilPiece, FossilAssemblyCompleted, FossilAssemblySlot, FossilAssemblySlotOf,
        FossilAssemblySlots, FossilPieceInSlot, FossilPieceOfWorld, FossilSetAssembly,
    },
    fossil_recovery_types::{FossilDiscoveryLevel, FossilDiscoveryRandomStream},
};
use serde::{Deserialize, Serialize};

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct FossilContinuation {
    level: Option<u8>,
    sites: Vec<FossilSiteSnapshot>,
    pieces: Vec<FossilPieceSnapshot>,
    assemblies: Vec<FossilAssemblySnapshot>,
    #[serde(skip)]
    pub(super) details: FossilSiteDetails,
    #[serde(skip)]
    pub(super) arranged_pieces: BTreeSet<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct FossilSiteDetails {
    requested: Option<AssetId>,
    kinds: BTreeMap<u64, u8>,
}

impl FossilSiteDetails {
    pub(super) fn is_empty(&self) -> bool {
        self.requested.is_none() && self.kinds.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FossilSiteSnapshot {
    id: u64,
    set: AssetId,
    position: [f32; 3],
    random: [u64; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FossilPieceSnapshot {
    id: u64,
    piece: AssetId,
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
    slot: Option<(u64, AssetId)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FossilAssemblySnapshot {
    table: u64,
    set: AssetId,
    slots: Vec<AssetId>,
    completed: bool,
}

impl FossilContinuation {
    /// The fossil sites and recovered pieces this record recreates.
    pub(super) fn persistent_identifiers(&self) -> impl Iterator<Item = u64> + '_ {
        self.sites
            .iter()
            .map(|site| site.id)
            .chain(self.pieces.iter().map(|piece| piece.id))
    }

    pub(super) fn valid(&self, next: u64) -> bool {
        let mut ids = BTreeSet::new();
        let mut occupied = BTreeSet::new();
        let assemblies: BTreeMap<_, _> = self
            .assemblies
            .iter()
            .map(|assembly| (assembly.table, assembly))
            .collect();
        let valid_asset = |id| id != AssetId::default();
        self.arranged_pieces.iter().all(|id| {
            self.pieces
                .binary_search_by_key(id, |piece| piece.id)
                .is_ok()
        }) && self.details.requested.is_none_or(valid_asset)
            && self
                .details
                .kinds
                .iter()
                .all(|(id, kind)| *kind <= 3 && self.sites.iter().any(|site| site.id == *id))
            && self.assemblies.len() == assemblies.len()
            && self.assemblies.iter().all(|assembly| {
                assembly.table != 0
                    && assembly.table < next
                    && valid_asset(assembly.set)
                    && !assembly.slots.is_empty()
                    && assembly.slots.iter().copied().all(valid_asset)
                    && assembly.slots.iter().collect::<BTreeSet<_>>().len() == assembly.slots.len()
            })
            && self.sites.iter().all(|site| {
                site.id != 0
                    && site.id < next
                    && ids.insert(site.id)
                    && site.position.iter().all(|value| value.is_finite())
            })
            && self.pieces.iter().all(|piece| {
                piece.id != 0
                    && piece.id < next
                    && ids.insert(piece.id)
                    && valid_asset(piece.piece)
                    && piece
                        .position
                        .iter()
                        .chain(&piece.rotation)
                        .chain(&piece.scale)
                        .all(|value| value.is_finite())
                    && piece.slot.is_none_or(|(table, slot)| {
                        occupied.insert((table, slot))
                            && assemblies
                                .get(&table)
                                .is_some_and(|assembly| assembly.slots.contains(&slot))
                    })
            })
            && self.assemblies.iter().all(|assembly| {
                !ids.contains(&assembly.table)
                    && (!assembly.completed
                        || assembly
                            .slots
                            .iter()
                            .all(|slot| occupied.contains(&(assembly.table, *slot))))
            })
    }
}

#[derive(SystemParam)]
pub(super) struct CaptureFossilContinuation<'w, 's> {
    level: Query<'w, 's, &'static FossilDiscoveryLevel>,
    requested: Query<'w, 's, &'static RequestedFossilPuzzle>,
    sites: Query<
        'w,
        's,
        (
            &'static PersistentId,
            Option<&'static FossilSiteKind>,
            Has<DigPit>,
            &'static Transform,
            Option<&'static FossilDiscoveryRandomStream>,
        ),
        Or<(With<FossilSiteKind>, With<DigPit>)>,
    >,
    pieces: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static CollectedFossilPiece,
            &'static Transform,
            Option<&'static FossilPieceInSlot>,
            Has<FossilAssemblyPiecePose>,
        ),
    >,
    slots: Query<'w, 's, (&'static FossilAssemblySlot, &'static FossilAssemblySlotOf)>,
    tables: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static FossilSetAssembly,
            &'static FossilAssemblySlots,
            Has<FossilAssemblyCompleted>,
        ),
    >,
}

impl CaptureFossilContinuation<'_, '_> {
    pub(super) fn capture(
        &self,
        root: Entity,
        members: Option<&WorldMembers>,
    ) -> Result<Option<FossilContinuation>, Failure> {
        let level = self.level.get(root).ok().map(|level| level.0);
        let members = || members.into_iter().flat_map(RelationshipTarget::iter);
        let mut details = FossilSiteDetails {
            requested: self.requested.get(root).ok().map(|puzzle| puzzle.0),
            ..Default::default()
        };
        let mut sites = self
            .sites
            .iter_many(members())
            .flatten()
            .map(|(id, kind, pit, pose, random)| {
                let kind = match (kind, pit) {
                    (_, true) => 3,
                    (Some(FossilSiteKind::Puzzle), false) => 0,
                    (Some(FossilSiteKind::PlaceableObject), false) => 1,
                    (Some(FossilSiteKind::Cash), false) => 2,
                    (None, false) => return Err(Failure::CorruptSnapshotSection),
                };
                let random = if pit {
                    [0, 1]
                } else {
                    random.ok_or(Failure::CorruptSnapshotSection)?.0.to_raw()
                };
                if kind != 0 {
                    details.kinds.insert(id.0, kind);
                }
                Ok(FossilSiteSnapshot {
                    id: id.0,
                    set: AssetId::default(),
                    position: pose.translation.to_array(),
                    random,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let mut assemblies = self
            .tables
            .iter_many(members())
            .flatten()
            .map(|(id, assembly, slots, completed)| {
                let slots = self
                    .slots
                    .iter_many(slots.iter())
                    .map(|slot| {
                        slot.map(|(slot, _)| slot.0)
                            .map_err(|_| Failure::BrokenPersistentEntityReference)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(FossilAssemblySnapshot {
                    table: id.0,
                    set: assembly.0,
                    slots,
                    completed,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let mut arranged_pieces = BTreeSet::new();
        let mut pieces = self
            .pieces
            .iter_many(members())
            .flatten()
            .map(|(id, piece, pose, slot, arranged)| {
                if arranged {
                    arranged_pieces.insert(id.0);
                }
                let slot = slot
                    .map(|slot| {
                        let (slot, table) = self
                            .slots
                            .get(slot.0)
                            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
                        let (table, ..) = self
                            .tables
                            .get(table.0)
                            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
                        Ok((table.0, slot.0))
                    })
                    .transpose()?;
                Ok(FossilPieceSnapshot {
                    id: id.0,
                    piece: piece.0,
                    position: pose.translation.to_array(),
                    rotation: pose.rotation.to_array(),
                    scale: pose.scale.to_array(),
                    slot,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        sites.sort_unstable_by_key(|site| site.id);
        pieces.sort_unstable_by_key(|piece| piece.id);
        assemblies.sort_unstable_by_key(|assembly| assembly.table);
        Ok((level.is_some()
            || !details.is_empty()
            || !sites.is_empty()
            || !pieces.is_empty()
            || !assemblies.is_empty())
        .then_some(FossilContinuation {
            level,
            sites,
            pieces,
            assemblies,
            details,
            arranged_pieces,
        }))
    }
}

#[derive(SystemParam)]
pub(super) struct RestoreFossilContinuation<'w, 's> {
    identities: Res<'w, PersistentIdIndex>,
    assets: Res<'w, Assets<WorldDefinitionAsset>>,
    definitions: Res<'w, WorldDefinitions>,
    levels: Query<'w, 's, &'static mut FossilDiscoveryLevel>,
    requested: Query<'w, 's, &'static mut RequestedFossilPuzzle>,
}

impl RestoreFossilContinuation<'_, '_> {
    pub(super) fn restore(
        &mut self,
        commands: &mut Commands,
        root: Entity,
        snapshot: &FossilContinuation,
    ) -> Result<(), Failure> {
        let catalogue = self
            .definitions
            .get(&self.assets)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let tables = snapshot
            .assemblies
            .iter()
            .map(|assembly| {
                let table = self
                    .identities
                    .get(root, PersistentId(assembly.table))
                    .ok_or(Failure::BrokenPersistentEntityReference)?;
                let set = catalogue
                    .find_fossil_set(assembly.set)
                    .ok_or(Failure::UnknownAssetIdentifier)?;
                if assembly.slots.iter().any(|slot| {
                    !set.pieces
                        .iter()
                        .filter_map(|piece| catalogue.find_fossil_piece(*piece))
                        .any(|piece| piece.slot == *slot)
                }) {
                    return Err(Failure::BrokenPersistentEntityReference);
                }
                Ok(table)
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        for site in &snapshot.sites {
            if self.identities.get(root, PersistentId(site.id)).is_some() {
                return Err(Failure::BrokenPersistentEntityReference);
            }
            if site.set != AssetId::default() && catalogue.find_fossil_set(site.set).is_none() {
                return Err(Failure::UnknownAssetIdentifier);
            }
        }
        for piece in &snapshot.pieces {
            if self.identities.get(root, PersistentId(piece.id)).is_some() {
                return Err(Failure::BrokenPersistentEntityReference);
            }
            let definition = catalogue
                .find_fossil_piece(piece.piece)
                .ok_or(Failure::UnknownAssetIdentifier)?;
            if piece.slot.is_some_and(|(_, slot)| definition.slot != slot) {
                return Err(Failure::BrokenPersistentEntityReference);
            }
        }
        let site_assets = snapshot
            .sites
            .iter()
            .map(|site| {
                let kind = snapshot
                    .details
                    .kinds
                    .get(&site.id)
                    .copied()
                    .unwrap_or_default();
                let definition = catalogue
                    .find_object(AssetId::from_key(if kind == 3 {
                        "digpit"
                    } else {
                        "fossilsite"
                    }))
                    .ok_or(Failure::UnknownAssetIdentifier)?;
                let scene = catalogue
                    .scene(definition.prefab)
                    .ok_or(Failure::UnknownAssetIdentifier)?;
                Ok((kind, definition, scene))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let requested = snapshot
            .details
            .requested
            .map(|puzzle| {
                catalogue
                    .find_fossil_set(puzzle)
                    .ok_or(Failure::UnknownAssetIdentifier)?;
                Ok(RequestedFossilPuzzle(puzzle))
            })
            .transpose()?;
        super::restored_component_values::restore_optional_component(
            &mut commands.entity(root),
            self.requested.get_mut(root).ok(),
            requested,
            true,
        );

        if let Some(level) = snapshot.level {
            if let Ok(mut current) = self.levels.get_mut(root) {
                current.set_if_neq(FossilDiscoveryLevel(level));
            } else {
                commands.entity(root).insert(FossilDiscoveryLevel(level));
            }
        }
        for (site, (kind, marker, scene)) in snapshot.sites.iter().zip(site_assets) {
            let mut entity = commands.spawn((
                WorldMember { root },
                PersistentId(site.id),
                DefinitionId(marker.id),
                Transform::from_translation(Vec3::from_array(site.position))
                    .with_scale(Vec3::splat(marker.prefab_scale)),
                if kind == 3 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                PrefabPresentation::new(scene.clone()),
            ));
            if kind == 3 {
                entity.insert(DigPit);
            } else {
                let kind = match kind {
                    1 => FossilSiteKind::PlaceableObject,
                    2 => FossilSiteKind::Cash,
                    _ => FossilSiteKind::Puzzle,
                };
                entity.insert((
                    kind,
                    FossilDiscoveryRandomStream(DeterministicRng::from_raw(site.random)),
                ));
            }
        }
        let mut slots = BTreeMap::new();
        for (assembly, table) in snapshot.assemblies.iter().zip(tables) {
            let mut table_commands = commands.entity(table);
            table_commands.insert(FossilSetAssembly(assembly.set));
            if assembly.completed {
                table_commands.insert(FossilAssemblyCompleted);
            }
            for &slot in &assembly.slots {
                let entity = commands
                    .spawn((FossilAssemblySlot(slot), FossilAssemblySlotOf(table)))
                    .id();
                slots.insert((assembly.table, slot), entity);
            }
        }
        for piece in &snapshot.pieces {
            let mut entity = commands.spawn((
                WorldMember { root },
                PersistentId(piece.id),
                CollectedFossilPiece(piece.piece),
                FossilPieceOfWorld(root),
                Visibility::default(),
                Transform {
                    translation: Vec3::from_array(piece.position),
                    rotation: Quat::from_array(piece.rotation),
                    scale: Vec3::from_array(piece.scale),
                },
            ));
            if snapshot.arranged_pieces.contains(&piece.id) {
                entity.insert(FossilAssemblyPiecePose);
            }
            if let Some(slot) = piece.slot {
                entity.insert(FossilPieceInSlot(slots[&slot]));
            }
        }
        Ok(())
    }
}
