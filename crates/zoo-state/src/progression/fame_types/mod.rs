use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow, system::SystemParam},
    prelude::*,
};
use openzt2_world_state::{
    application_schedule::FixedGameSet, world_spawn::world_membership_types::WorldMember,
};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct Fame {
    pub half_stars: u8,
    pub current_percent: Option<f32>,
    pub maximum_reached: u8,
    pub maximum_percent_reached: Option<f32>,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct ZooMaximumFame(pub Option<f32>);

#[derive(Message, Debug, Clone, Copy)]
pub struct SetScenarioMaximumFame {
    pub percent: f32,
}

impl Fame {
    pub fn half_stars_for_percent(percent: f32) -> u8 {
        (percent / 10.0).floor() as u8
    }
}

/// Authored presentation request to replace the entrance appearance in place.
#[derive(Message, Clone, Copy)]
pub struct FameGateAppearanceRequested {
    pub gate: Entity,
    pub definition: openzt2_game_data::AssetId,
}

#[derive(Component, PartialEq)]
pub struct PendingFameGatePrefab {
    pub definition: openzt2_game_data::AssetId,
    pub prefab: Handle<openzt2_game_assets::scene_prefab::ScenePrefabAsset>,
    pub releasing_root_renderable: bool,
}

/// Accumulated view facts per species from the guest's accepted view events.
#[derive(Component, Default)]
#[require(GuestSpeciesFameContribution)]
pub struct GuestSpeciesFameViews(
    pub std::collections::BTreeMap<openzt2_game_data::AssetId, GuestSpeciesFameView>,
);

impl GuestSpeciesFameViews {
    pub fn restore(&mut self, saved: Self, total: Option<&mut ZooSpeciesFameViews>) {
        if self.0 == saved.0 {
            return;
        }
        if let Some(total) = total {
            total.remove_guest(self);
            total.add_guest(&saved);
        }
        *self = saved;
    }
}

/// The zoo currently receiving this guest's live contribution.
#[derive(Component, Default)]
pub struct GuestSpeciesFameContribution(Option<Entity>);

impl GuestSpeciesFameContribution {
    pub fn root(&self) -> Option<Entity> {
        self.0
    }
}

pub fn observe_guest_species_fame_membership(app: &mut App) {
    app.add_observer(guest_views_inserted)
        .add_observer(guest_views_discarded)
        .add_observer(guest_entered_world)
        .add_observer(guest_left_world)
        .add_observer(guest_disabled)
        .add_systems(FixedUpdate, guest_enabled.in_set(FixedGameSet::Clock));
}

#[derive(SystemParam)]
struct GuestFameMembership<'w, 's> {
    guests: Query<
        'w,
        's,
        (
            Option<&'static WorldMember>,
            Has<Disabled>,
            &'static GuestSpeciesFameViews,
            &'static mut GuestSpeciesFameContribution,
        ),
        Allow<Disabled>,
    >,
    zoos: Query<'w, 's, &'static mut ZooSpeciesFameViews, Allow<Disabled>>,
}

fn guest_views_inserted(event: On<Insert<GuestSpeciesFameViews>>, mut fame: GuestFameMembership) {
    fame.reconcile(event.entity, true);
}

fn guest_views_discarded(event: On<Discard<GuestSpeciesFameViews>>, mut fame: GuestFameMembership) {
    fame.reconcile(event.entity, false);
}

fn guest_entered_world(event: On<Insert<WorldMember>>, mut fame: GuestFameMembership) {
    fame.reconcile(event.entity, true);
}

fn guest_left_world(event: On<Remove<WorldMember>>, mut fame: GuestFameMembership) {
    fame.reconcile(event.entity, false);
}

fn guest_disabled(event: On<Add<Disabled>>, mut fame: GuestFameMembership) {
    fame.reconcile(event.entity, false);
}

fn guest_enabled(mut events: RemovedComponents<Disabled>, mut fame: GuestFameMembership) {
    for guest in events.read() {
        fame.reconcile(guest, true);
    }
}

impl GuestFameMembership<'_, '_> {
    fn reconcile(&mut self, guest: Entity, active: bool) {
        let Ok((member, disabled, views, contribution)) = self.guests.get_mut(guest) else {
            return;
        };
        let next = member
            .filter(|member| active && !disabled && self.zoos.contains(member.root))
            .map(|member| member.root);
        let previous = contribution.0;
        if !contribution
            .map_unchanged(|contribution| &mut contribution.0)
            .set_if_neq(next)
            || views.0.is_empty()
        {
            return;
        }
        if let Some(mut previous) = previous.and_then(|root| self.zoos.get_mut(root).ok()) {
            previous.remove_guest(views);
        }
        if let Some(mut next) = next.and_then(|root| self.zoos.get_mut(root).ok()) {
            next.add_guest(views);
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct GuestSpeciesFameView {
    pub events: u32,
    pub entertainment: f32,
    pub donation: f32,
}

#[derive(Component, Default)]
pub struct ZooSpeciesFameViews(
    pub std::collections::BTreeMap<openzt2_game_data::AssetId, ZooSpeciesFameView>,
);

#[derive(Default)]
pub struct ZooSpeciesFameView {
    pub guests_with_views: u32,
    pub events: u32,
    pub entertainment: f32,
    pub donation: f32,
    contributors: u32,
}

impl ZooSpeciesFameViews {
    pub fn replace_contribution(
        &mut self,
        species: openzt2_game_data::AssetId,
        before: GuestSpeciesFameView,
        after: GuestSpeciesFameView,
    ) {
        if before == after {
            return;
        }
        let total = self.0.entry(species).or_default();
        let empty = GuestSpeciesFameView::default();
        total.contributors = total
            .contributors
            .saturating_sub(u32::from(before != empty))
            .saturating_add(u32::from(after != empty));
        if total.contributors == 0 {
            self.0.remove(&species);
            return;
        }
        total.guests_with_views = total
            .guests_with_views
            .saturating_sub(u32::from(before.events != 0))
            .saturating_add(u32::from(after.events != 0));
        total.events = total
            .events
            .saturating_sub(before.events)
            .saturating_add(after.events);
        total.entertainment += after.entertainment - before.entertainment;
        total.donation += after.donation - before.donation;
    }

    pub fn add_guest(&mut self, guest: &GuestSpeciesFameViews) {
        for (&species, &view) in &guest.0 {
            self.replace_contribution(species, GuestSpeciesFameView::default(), view);
        }
    }

    pub fn remove_guest(&mut self, guest: &GuestSpeciesFameViews) {
        for (&species, &view) in &guest.0 {
            self.replace_contribution(species, view, GuestSpeciesFameView::default());
        }
    }
}

/// Layout values produced by the world's authored fame script.
#[derive(Component, Default, PartialEq)]
pub struct FameAuthoredPresentation {
    pub bars: std::collections::BTreeMap<openzt2_game_data::AssetId, f32>,
    pub gate_icon: Option<openzt2_game_data::AssetId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::AssetId;

    #[test]
    fn live_guest_view_totals_follow_world_and_enable_state() {
        let mut app = App::new();
        observe_guest_species_fame_membership(&mut app);
        let world = app.world_mut();
        let first = world.spawn(ZooSpeciesFameViews::default()).id();
        let second = world.spawn(ZooSpeciesFameViews::default()).id();
        let species = AssetId::from_key("tiger");
        let view = GuestSpeciesFameView {
            events: 2,
            entertainment: 0.5,
            donation: 1.0,
        };
        let guest = world
            .spawn((
                GuestSpeciesFameViews([(species, view)].into()),
                WorldMember { root: first },
            ))
            .id();
        assert_eq!(
            world.get::<ZooSpeciesFameViews>(first).unwrap().0[&species].guests_with_views,
            1
        );

        world.entity_mut(guest).insert(WorldMember { root: second });
        assert!(world
            .get::<ZooSpeciesFameViews>(first)
            .unwrap()
            .0
            .is_empty());
        assert_eq!(
            world.get::<ZooSpeciesFameViews>(second).unwrap().0[&species].guests_with_views,
            1
        );

        world.entity_mut(guest).insert(Disabled);
        assert!(world
            .get::<ZooSpeciesFameViews>(second)
            .unwrap()
            .0
            .is_empty());
        world.entity_mut(guest).remove::<Disabled>();
        world.run_schedule(FixedUpdate);
        assert_eq!(
            world.get::<ZooSpeciesFameViews>(second).unwrap().0[&species].guests_with_views,
            1
        );

        world.entity_mut(guest).despawn();
        assert!(world
            .get::<ZooSpeciesFameViews>(second)
            .unwrap()
            .0
            .is_empty());
    }
}
