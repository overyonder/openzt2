use bevy::{
    ecs::relationship::{Relationship, RelationshipTarget},
    prelude::*,
};
use openzt2_game_data::ui_document::action::information::{
    InformationListCategory, InformationViewCategory,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::{
    animal_lifecycle::types::Animal, donations::donation_opportunity_types::DonationAcceptor,
    guests::guest_simulation_types::Guest, staff::staff_employment_types::Staff,
    transport_tours::transport_vehicle_types::TransportVehicle,
};

use super::entity_editor_data_root_projection::InspectableInWorld;
use crate::information_view_types::InformationViewClass;

macro_rules! source_group {
    ($link:ident, $target:ident) => {
        #[derive(Component)]
        #[relationship(relationship_target = $target)]
        pub(crate) struct $link(Entity);

        #[derive(Component)]
        #[relationship_target(relationship = $link)]
        pub(crate) struct $target(Vec<Entity>);
    };
}

source_group!(AnimalOfManagementList, AnimalManagementSubjects);
source_group!(GuestOfManagementList, GuestManagementSubjects);
source_group!(StaffOfManagementList, StaffManagementSubjects);
source_group!(BuildingOfManagementList, BuildingManagementSubjects);
source_group!(DonationBoxOfManagementList, DonationBoxManagementSubjects);
source_group!(VehicleOfManagementList, VehicleManagementSubjects);

pub(super) fn link_tagged_management_subjects<T: Component, R: Relationship>(
    tagged: Query<
        (Entity, &InspectableInWorld, Option<&R>),
        (With<T>, Or<(Added<T>, Changed<InspectableInWorld>)>),
    >,
    mut commands: Commands,
) {
    for (subject, world, previous) in &tagged {
        let root = world.get();
        if previous.is_none_or(|previous| previous.get() != root) {
            commands.entity(subject).insert(R::from(root));
        }
    }
}

pub(super) fn link_buildings_for_management_lists(
    tagged: Query<
        (
            Entity,
            &InspectableInWorld,
            &InformationViewClass,
            Option<&BuildingOfManagementList>,
        ),
        Or<(Changed<InspectableInWorld>, Changed<InformationViewClass>)>,
    >,
    mut commands: Commands,
) {
    for (subject, world, class, previous) in &tagged {
        if class.0 == InformationViewCategory::Buildings {
            if previous.is_none_or(|previous| previous.get() != world.get()) {
                commands
                    .entity(subject)
                    .insert(BuildingOfManagementList(world.get()));
            }
        } else if previous.is_some() {
            commands
                .entity(subject)
                .remove::<BuildingOfManagementList>();
        }
    }
}

pub(super) fn unlink_retired_management_subject<T: Component, R: Relationship>(
    removed: On<Remove<(T, InspectableInWorld)>>,
    linked: Query<(), With<R>>,
    mut commands: Commands,
) {
    if linked.contains(removed.entity) {
        commands.entity(removed.entity).try_remove::<R>();
    }
}

pub(crate) fn install_management_list_memberships(app: &mut App) {
    use super::{
        entity_editor_data_root_projection::link_inspectable_world_subjects,
        information_list_projection::project_live_world_entities_to_visible_information_list_rows,
    };
    app.add_observer(unlink_retired_management_subject::<Animal, AnimalOfManagementList>)
        .add_observer(unlink_retired_management_subject::<Guest, GuestOfManagementList>)
        .add_observer(unlink_retired_management_subject::<Staff, StaffOfManagementList>)
        .add_observer(
            unlink_retired_management_subject::<InformationViewClass, BuildingOfManagementList>,
        )
        .add_observer(
            unlink_retired_management_subject::<DonationAcceptor, DonationBoxOfManagementList>,
        )
        .add_observer(
            unlink_retired_management_subject::<TransportVehicle, VehicleOfManagementList>,
        )
        .add_systems(
            Update,
            (
                link_tagged_management_subjects::<Animal, AnimalOfManagementList>,
                link_tagged_management_subjects::<Guest, GuestOfManagementList>,
                link_tagged_management_subjects::<Staff, StaffOfManagementList>,
                link_tagged_management_subjects::<DonationAcceptor, DonationBoxOfManagementList>,
                link_tagged_management_subjects::<TransportVehicle, VehicleOfManagementList>,
                link_buildings_for_management_lists,
            )
                .after(link_inspectable_world_subjects)
                .before(project_live_world_entities_to_visible_information_list_rows),
        );
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ManagementListMembers<'w, 's> {
    groups: Query<
        'w,
        's,
        (
            Option<Ref<'static, AnimalManagementSubjects>>,
            Option<Ref<'static, GuestManagementSubjects>>,
            Option<Ref<'static, StaffManagementSubjects>>,
            Option<Ref<'static, BuildingManagementSubjects>>,
            Option<Ref<'static, DonationBoxManagementSubjects>>,
            Option<Ref<'static, VehicleManagementSubjects>>,
        ),
        With<WorldRoot>,
    >,
    removed: (
        RemovedComponents<'w, 's, AnimalManagementSubjects>,
        RemovedComponents<'w, 's, GuestManagementSubjects>,
        RemovedComponents<'w, 's, StaffManagementSubjects>,
        RemovedComponents<'w, 's, BuildingManagementSubjects>,
        RemovedComponents<'w, 's, DonationBoxManagementSubjects>,
        RemovedComponents<'w, 's, VehicleManagementSubjects>,
    ),
}

impl ManagementListMembers<'_, '_> {
    pub(super) fn removals(&mut self) -> [bool; 6] {
        let (animals, guests, staff, buildings, donations, vehicles) = &mut self.removed;
        [
            animals.read().count() != 0,
            guests.read().count() != 0,
            staff.read().count() != 0,
            buildings.read().count() != 0,
            donations.read().count() != 0,
            vehicles.read().count() != 0,
        ]
    }

    pub(super) fn members(
        &self,
        category: InformationListCategory,
        removed: [bool; 6],
    ) -> (&[Entity], bool) {
        let Some((animals, guests, staff, buildings, donations, vehicles)) =
            self.groups.iter().next()
        else {
            return (&[], removed.into_iter().any(|removed| removed));
        };
        match category {
            InformationListCategory::Animals => group_members(animals, removed[0]),
            InformationListCategory::Guests => group_members(guests, removed[1]),
            InformationListCategory::Staff => group_members(staff, removed[2]),
            InformationListCategory::Buildings => group_members(buildings, removed[3]),
            InformationListCategory::DonationBoxes => group_members(donations, removed[4]),
            InformationListCategory::Vehicles => group_members(vehicles, removed[5]),
        }
    }
}

fn group_members<T: RelationshipTarget<Collection = Vec<Entity>>>(
    group: Option<Ref<'_, T>>,
    removed: bool,
) -> (&[Entity], bool) {
    group.map_or((&[], removed), |group| {
        let changed = group.is_changed();
        (
            group.into_inner().collection().as_slice(),
            changed || removed,
        )
    })
}
