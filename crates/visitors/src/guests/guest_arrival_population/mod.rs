use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, system::SystemParam},
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    world_entity_crating::{AnimalCrate, ScriptedCrating},
    world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    guests::guest_simulation_types::Guest,
};

#[derive(Component, Default)]
pub(super) struct ArrivalPopulation {
    pub(super) guests: u32,
    live_species: HashMap<AssetId, u32>,
}

impl ArrivalPopulation {
    fn add(&mut self, member: ArrivalPopulationMember) {
        self.guests += u32::from(member.guest);
        if let Some(species) = member.species {
            *self.live_species.entry(species).or_default() += 1;
        }
    }

    fn withdraw(&mut self, member: ArrivalPopulationMember) {
        self.guests -= u32::from(member.guest);
        if let Some(species) = member.species {
            if let Some(count) = self.live_species.get_mut(&species) {
                *count -= 1;
                if *count == 0 {
                    self.live_species.remove(&species);
                }
            }
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct ArrivalPopulationMember {
    root: Entity,
    guest: bool,
    species: Option<AssetId>,
}

pub(super) fn add_arrival_population_member(
    event: On<Add<ArrivalPopulationMember>>,
    members: Query<&ArrivalPopulationMember, Allow<Disabled>>,
    mut worlds: Query<&mut ArrivalPopulation, Allow<Disabled>>,
) {
    if let Ok(member) = members.get(event.entity) {
        if let Ok(mut world) = worlds.get_mut(member.root) {
            world.add(*member);
        }
    }
}

pub(super) fn remove_arrival_population_member(
    event: On<Remove<ArrivalPopulationMember>>,
    members: Query<&ArrivalPopulationMember, Allow<Disabled>>,
    mut worlds: Query<&mut ArrivalPopulation, Allow<Disabled>>,
) {
    if let Ok(member) = members.get(event.entity) {
        if let Ok(mut world) = worlds.get_mut(member.root) {
            world.withdraw(*member);
        }
    }
}

pub(super) fn disable_arrival_population_member(
    event: On<Add<Disabled>>,
    members: Query<(), (With<ArrivalPopulationMember>, Allow<Disabled>)>,
    mut commands: Commands,
) {
    if members.contains(event.entity) {
        commands
            .entity(event.entity)
            .remove::<ArrivalPopulationMember>();
    }
}

#[derive(SystemParam)]
pub(super) struct PopulationChanges<'w, 's> {
    changed: Query<
        'w,
        's,
        Entity,
        (
            Or<(With<Guest>, With<Animal>)>,
            Or<(
                Changed<WorldMember>,
                Changed<SpeciesHandle>,
                Added<Guest>,
                Added<Animal>,
                Added<Dead>,
                Or<(Added<AnimalCrate>, Added<ScriptedCrating>)>,
            )>,
        ),
    >,
    guests: RemovedComponents<'w, 's, Guest>,
    animals: RemovedComponents<'w, 's, Animal>,
    species: RemovedComponents<'w, 's, SpeciesHandle>,
    members: RemovedComponents<'w, 's, WorldMember>,
    dead: RemovedComponents<'w, 's, Dead>,
    crated: RemovedComponents<'w, 's, AnimalCrate>,
    script_crated: RemovedComponents<'w, 's, ScriptedCrating>,
    enabled: RemovedComponents<'w, 's, Disabled>,
}

#[allow(clippy::type_complexity)]
pub(super) fn synchronize_arrival_population(
    mut commands: Commands,
    mut changes: PopulationChanges,
    mut candidates: Query<
        (
            Option<&WorldMember>,
            Option<&SpeciesHandle>,
            Has<Guest>,
            Has<Animal>,
            Has<Dead>,
            (Has<AnimalCrate>, Has<ScriptedCrating>),
            Option<&mut ArrivalPopulationMember>,
        ),
        Or<(With<Guest>, With<Animal>, With<ArrivalPopulationMember>)>,
    >,
    mut worlds: Query<&mut ArrivalPopulation, Allow<Disabled>>,
) {
    let affected: EntityHashSet = changes
        .changed
        .iter()
        .chain(changes.guests.read())
        .chain(changes.animals.read())
        .chain(changes.species.read())
        .chain(changes.members.read())
        .chain(changes.dead.read())
        .chain(changes.crated.read().chain(changes.script_crated.read()))
        .chain(changes.enabled.read())
        .collect();
    for entity in affected {
        let Ok((member, species, guest, animal, dead, crated, applied)) =
            candidates.get_mut(entity)
        else {
            continue;
        };
        let species = species
            .filter(|_| animal && !dead && !crated.0 && !crated.1)
            .map(|handle| handle.species);
        let Some(member) =
            member.filter(|member| (guest || species.is_some()) && worlds.contains(member.root))
        else {
            if applied.is_some() {
                commands.entity(entity).remove::<ArrivalPopulationMember>();
            }
            continue;
        };
        let contribution = ArrivalPopulationMember {
            root: member.root,
            guest,
            species,
        };
        if let Some(mut applied) = applied {
            if *applied == contribution {
                continue;
            }
            if let Ok(mut previous) = worlds.get_mut(applied.root) {
                previous.withdraw(*applied);
            }
            if let Ok(mut next) = worlds.get_mut(contribution.root) {
                next.add(contribution);
            }
            applied.set_if_neq(contribution);
        } else {
            commands.entity(entity).insert(contribution);
        }
    }
}
