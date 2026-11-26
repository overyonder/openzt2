use bevy::ecs::{relationship::RelationshipTarget, resource::IsResource, system::SystemParam};
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::{world_definitions::guest_simulation_definitions::{GuestMemoryKind, GuestVisitPurpose}, AssetId};
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_world_state::{
    simulation_time::deterministic_random_stream::DeterministicRng,
    world_spawn::{
        persistent_id_index::PersistentIdIndex,
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        prefab_presentation_types::PrefabPresentation,
        world_membership_types::{DefinitionId, WorldMember, WorldMembers},
    },
};
use openzt2_zoo_state::{
    behavior_task_execution_types::running_actions::GuestTourDepartureGate,
    economy::{facility_economy_types::Wallet, money_types::Money},
    guests::guest_simulation_types::*,
    locomotion::locomotion_types::{
        Destination, EntranceNavigationConnection, NavAgent, NavFlags,
        NavigationRequestGeneration,
    },
};

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct GuestContinuation {
    pub(super) id: u64,
    archetype: AssetId,
    name: Option<String>,
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
    needs: [(u16, u16); 8],
    satisfaction: (u16, u16),
    viewing_need: Option<i32>,
    favourite: Option<AssetId>,
    education: u16,
    departure_points: i32,
    visit_ticks: u64,
    phase: u8,
    wallet: f32,
    remaining_ns: u64,
    random: [u64; 2],
    tour: Option<(f32, bool)>,
    destination: Option<Destination>,
    entrance: Option<([f32; 3], bool)>,
    visit_destination: Option<(u64, GuestVisitPurpose)>,
    memories: Vec<SavedGuestMemory>,
    memory_cursor: u16,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
struct SavedGuestMemory {
    subject: u64,
    kind: GuestMemoryKind,
    value: i16,
    created_tick: u64,
}

impl GuestContinuation {
    pub(super) fn navigation_request(&self) -> u64 {
        self.destination
            .map_or(0, |destination| destination.request_id)
    }

    pub(super) fn is_valid(&self) -> bool {
        self.id != 0
            && self.archetype != AssetId::default()
            && self.random[1] & 1 != 0
            && self.phase <= 2
            && self.wallet.is_finite()
            && Vec3::from_array(self.position).is_finite()
            && Quat::from_array(self.rotation).is_finite()
            && (Quat::from_array(self.rotation).length_squared() - 1.0).abs() <= 0.001
            && Vec3::from_array(self.scale).is_finite()
            && Vec3::from_array(self.scale).min_element() > 0.0
            && self.needs.iter().all(|(value, _)| *value <= 1_000)
            && self.satisfaction.0 <= 1_000
            && self.memories.len() <= openzt2_game_data::world_definitions::GUEST_MEMORY_CAPACITY
            && self.visit_destination.is_none_or(|(id, _)| id != 0)
            && self.tour.is_none_or(|(need, _)| need.is_finite())
            && self.entrance.is_none_or(|(gate, _)| {
                self.destination.is_some() && Vec3::from_array(gate).is_finite()
            })
            && self.destination.is_none_or(|destination| {
                destination.request_id != 0
                    && destination.world.is_finite()
                    && destination.arrival_radius_m.is_finite()
                    && destination.arrival_radius_m >= 0.0
            })
    }
}

#[derive(SystemParam)]
pub(super) struct GuestContinuationCapture<'w, 's> {
    guests: Query<'w, 's, EntityRef<'static>, (With<Guest>, Without<IsResource>)>,
    identities: Query<'w, 's, &'static PersistentId>,
}

impl GuestContinuationCapture<'_, '_> {
    pub(super) fn capture(
        &self,
        members: Option<&WorldMembers>,
    ) -> Result<Vec<GuestContinuation>, Failure> {
        let records = members
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(|entity| {
                self.guests
                    .get(entity)
                    .ok()
                    .map(|guest| capture_guest(guest, &self.identities))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut records = records;
        records.sort_unstable_by_key(|record| record.id);
        Ok(records)
    }
}

fn read<T: Component>(guest: EntityRef<'_>) -> Result<&T, Failure> {
    guest.get::<T>().ok_or(Failure::CorruptSnapshotSection)
}

fn capture_guest(
    guest: EntityRef<'_>,
    identities: &Query<&PersistentId>,
) -> Result<GuestContinuation, Failure> {
    let random = guest
        .get::<GuestRng>()
        .ok_or(Failure::SnapshotTransactionInFlight)?;
    let transform = read::<Transform>(guest)?;
    macro_rules! need {
        ($ty:ty) => {{
            let need = read::<$ty>(guest)?;
            (need.value, need.residual_q16)
        }};
    }
    let satisfaction = read::<GuestSatisfaction>(guest)?;
    let destination = guest.get::<Destination>().copied();
    let memories = read::<GuestMemories>(guest)?;
    Ok(GuestContinuation {
        id: read::<PersistentId>(guest)?.0,
        archetype: read::<GuestArchetype>(guest)?.0,
        name: guest.get::<Name>().map(|name| name.as_str().to_owned()),
        position: transform.translation.to_array(),
        rotation: transform.rotation.to_array(),
        scale: transform.scale.to_array(),
        needs: [
            need!(GuestHunger),
            need!(GuestThirst),
            need!(GuestDessert),
            need!(GuestGift),
            need!(GuestEnergy),
            need!(GuestRestroom),
            need!(GuestSocial),
            need!(GuestAmusement),
        ],
        satisfaction: (satisfaction.0, satisfaction.1),
        viewing_need: guest.get::<GuestViewingNeed>().map(|need| need.value_q16),
        favourite: guest
            .get::<GuestFavouriteAnimal>()
            .and_then(|favourite| favourite.0),
        education: read::<GuestEducation>(guest)?.0,
        departure_points: read::<GuestDeparturePoints>(guest)?.0,
        visit_ticks: read::<VisitTime>(guest)?.elapsed_ticks,
        phase: match read::<GuestPhase>(guest)? {
            GuestPhase::Arriving => 0,
            GuestPhase::Visiting => 1,
            GuestPhase::Leaving => 2,
        },
        wallet: read::<Wallet>(guest)?.0.dollars(),
        remaining_ns: read::<GuestNeedAdjustmentTimer>(guest)?.remaining_ns(),
        random: random.0.to_raw(),
        tour: guest
            .get::<GuestTourDepartureGate>()
            .map(|tour| (tour.need, tour.triggered)),
        destination,
        visit_destination: guest.get::<GuestDestination>().and_then(|destination| {
            Some((
                identities.get(destination.entity).ok()?.0,
                destination.purpose,
            ))
        }),
        memories: memories
            .entries
            .iter()
            .filter_map(|memory| {
                Some(SavedGuestMemory {
                    subject: identities.get(memory.subject).ok()?.0,
                    kind: memory.kind,
                    value: memory.value_permille,
                    created_tick: memory.created_tick,
                })
            })
            .collect(),
        memory_cursor: memories.next,
        entrance: guest
            .get::<EntranceNavigationConnection>()
            .and_then(|entrance| {
                (Some(entrance.request_id) == destination.map(|destination| destination.request_id))
                    .then_some((entrance.gate.to_array(), entrance.inbound))
            }),
    })
}

pub(super) fn validate_guest_continuation(
    guests: &[GuestContinuation],
    definitions: WorldDefinitionsView<'_>,
) -> Result<(), Failure> {
    for guest in guests {
        let definition = definitions
            .find_guest(guest.archetype)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let object = definitions
            .find_object(definition.object)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        if definitions.scene(object.prefab).is_none()
            || guest.tour.is_some()
                && (definition.view_scoring.tour_need.is_none()
                    || definitions.tour_scoring().is_none())
        {
            return Err(Failure::UnknownAssetIdentifier);
        }
    }
    Ok(())
}

pub(super) fn restore_guests(
    commands: &mut Commands,
    root: Entity,
    guests: &[GuestContinuation],
    definitions: WorldDefinitionsView<'_>,
    ids: &mut PersistentIdAllocator,
) -> Result<(), Failure> {
    for saved in guests {
        let definition = definitions
            .find_guest(saved.archetype)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let object = definitions
            .find_object(definition.object)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let prefab = definitions
            .scene(object.prefab)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let id = PersistentId(saved.id);
        ids.reserve_imported(id)
            .map_err(|_| Failure::CapacityExceeded)?;
        macro_rules! need {
            ($ty:ident, $index:literal) => {
                $ty {
                    value: saved.needs[$index].0,
                    residual_q16: saved.needs[$index].1,
                }
            };
        }
        let mut guest = commands.spawn((
            Guest,
            GuestArchetype(saved.archetype),
            GuestFavouriteAnimal(saved.favourite),
            WorldMember { root },
            id,
            DefinitionId(definition.object),
            Inspectable {
                definition: definition.id,
            },
            PrefabPresentation::new(prefab),
            Transform {
                translation: Vec3::from_array(saved.position),
                rotation: Quat::from_array(saved.rotation),
                scale: Vec3::from_array(saved.scale),
            },
            Visibility::Inherited,
            NavAgent {
                radius_m: f32::from(definition.radius_cm) * 0.01,
                max_speed_mps: definition.move_speed_mps,
                acceleration_mps2: f32::MAX,
                capabilities: NavFlags::GUEST,
            },
            GuestNeedContinuation {
                hunger: need!(GuestHunger, 0),
                thirst: need!(GuestThirst, 1),
                dessert: need!(GuestDessert, 2),
                gift: need!(GuestGift, 3),
                energy: need!(GuestEnergy, 4),
                restroom: need!(GuestRestroom, 5),
                social: need!(GuestSocial, 6),
                amusement: need!(GuestAmusement, 7),
                satisfaction: GuestSatisfaction(saved.satisfaction.0, saved.satisfaction.1),
                timer: GuestNeedAdjustmentTimer::from_saved_remaining(saved.remaining_ns),
                random: GuestRng(DeterministicRng::from_raw(saved.random)),
            },
            GuestVisitContinuation {
                phase: match saved.phase {
                    0 => GuestPhase::Arriving,
                    1 => GuestPhase::Visiting,
                    _ => GuestPhase::Leaving,
                },
                visit: VisitTime {
                    elapsed_ticks: saved.visit_ticks,
                },
                wallet: Wallet(
                    Money::from_dollars(saved.wallet).ok_or(Failure::CorruptSnapshotSection)?,
                ),
            },
        ));
        guest.insert((
            GuestEducation(saved.education),
            GuestDeparturePoints(saved.departure_points),
            GuestMemories::new(definition.memory.capacity),
        ));
        if let Some(name) = &saved.name {
            guest.insert(Name::new(name.clone()));
        }
        if let Some(need) = saved.viewing_need {
            guest.insert(GuestViewingNeed::new(need));
        }
        if let Some((need, triggered)) = saved.tour {
            let policy = definition
                .view_scoring
                .tour_need
                .ok_or(Failure::UnknownAssetIdentifier)?;
            let threshold = definitions
                .tour_scoring()
                .ok_or(Failure::UnknownAssetIdentifier)?
                .need_threshold;
            guest.insert(GuestTourDepartureGate {
                need,
                threshold,
                policy,
                triggered,
            });
        }
        if let Some(destination) = saved.destination {
            let mut sequence = NavigationRequestGeneration::default();
            sequence.reserve_restored(destination.request_id);
            guest.insert((
                GuestNavigationRequest(Some(destination.request_id)),
                sequence,
                destination,
            ));
            if let Some((gate, inbound)) = saved.entrance {
                guest.insert(EntranceNavigationConnection {
                    request_id: destination.request_id,
                    gate: Vec3::from_array(gate),
                    inbound,
                });
            }
        }
    }
    Ok(())
}

#[derive(SystemParam)]
pub(super) struct GuestContinuationApplication<'w, 's> {
    guests: Query<
        'w,
        's,
        (
            &'static mut GuestMemories,
            Option<&'static mut GuestDestination>,
        ),
        With<Guest>,
    >,
}

impl GuestContinuationApplication<'_, '_> {
    pub(super) fn apply(
        &mut self,
        commands: &mut Commands,
        root: Entity,
        records: &[GuestContinuation],
        identities: &PersistentIdIndex,
    ) -> Result<(), Failure> {
        let resolve = |id| identities.get(root, PersistentId(id));
        for record in records {
            let guest = resolve(record.id).ok_or(Failure::BrokenPersistentEntityReference)?;
            let (mut memories, destination) = self
                .guests
                .get_mut(guest)
                .map_err(|_| Failure::BrokenPersistentEntityReference)?;
            memories.entries.clear();
            for saved in record
                .memories
                .iter()
                .take(memories.active_capacity as usize)
            {
                if let Some(subject) = resolve(saved.subject) {
                    memories.entries.push(GuestMemoryEntry {
                        subject,
                        kind: saved.kind,
                        value_permille: saved.value,
                        created_tick: saved.created_tick,
                    });
                }
            }
            memories.next = record
                .memory_cursor
                .min(memories.active_capacity.saturating_sub(1));
            let next = record.visit_destination.and_then(|(id, purpose)| {
                Some(GuestDestination {
                    entity: resolve(id)?,
                    purpose,
                })
            });
            super::restored_component_values::restore_optional_component(
                &mut commands.entity(guest),
                destination,
                next,
                true,
            );
        }
        Ok(())
    }
}
