use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::{HabitatCellOf, HabitatIndex};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

#[test]
fn preassigned_habitat_membership_still_initializes_containment() {
    use crate::habitat::habitat_membership_and_containment::{
        resolve_changed_habitat_membership_and_containment, LostHabitatMembership,
    };
    use openzt2_zoo_state::habitat::habitat_cell_ownership_changes::HabitatCellOwnershipChanged;
    use openzt2_zoo_state::habitat::habitat_types::Containment;
    use openzt2_zoo_state::habitat::habitat_types::ContainmentChanged;
    use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
    use openzt2_zoo_state::habitat::habitat_types::HabitatLocatable;
    use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
    use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
    for breached in [false, true] {
        let mut app = App::new();
        app.add_message::<HabitatCellOwnershipChanged>()
            .add_message::<LostHabitatMembership>()
            .add_message::<HabitatChanged>()
            .add_message::<ContainmentChanged>()
            .add_systems(Update, resolve_changed_habitat_membership_and_containment);
        let habitat = app
            .world_mut()
            .spawn(HabitatSummary {
                land_area_square_metres: 1.0,
                water_area_square_metres: 0.0,
                biome_areas_square_metres: Box::default(),
                boundary_is_breached: breached,
            })
            .id();
        let mut index = HabitatIndex::default();
        let cell = app.world_mut().spawn(HabitatCellOf(habitat)).id();
        index.record_cell(IVec2::ZERO, cell);
        app.world_mut().spawn((
            TopologyGrid::default(),
            index,
            WorldRoot {
                scenario: openzt2_game_data::AssetId::from_key("test"),
            },
        ));
        let animal = app
            .world_mut()
            .spawn((
                HabitatLocatable,
                HabitatMember {
                    habitat_entity: habitat,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Containment>(animal),
            Some(&Containment {
                is_contained: !breached,
            })
        );
    }
}

#[test]
fn world_position_habitat_lookup_uses_the_topology_grid_origin_and_spacing() {
    let mut world = World::new();
    let habitat_entity = world.spawn_empty().id();
    let cell = world.spawn(HabitatCellOf(habitat_entity)).id();
    let mut habitat_index = HabitatIndex::default();
    habitat_index.record_cell(IVec2::new(1, 2), cell);
    let mut state = bevy::ecs::system::SystemState::<Query<&HabitatCellOf>>::new(&mut world);
    let owners = state.get(&world);
    let topology_grid = TopologyGrid {
        origin: Vec2::new(10.0, -20.0),
        spacing_m: 0.75,
    };
    assert_eq!(
        locate_habitat_at_world_position(
            &habitat_index,
            &owners,
            Vec2::new(10.8, -17.9),
            topology_grid,
        ),
        Some(habitat_entity),
    );
    assert_eq!(
        locate_habitat_at_world_position(
            &habitat_index,
            &owners,
            Vec2::new(10.7, -17.9),
            topology_grid,
        ),
        None,
    );
    assert_eq!(
        locate_habitat_at_world_position(
            &habitat_index,
            &owners,
            Vec2::new(10.8, -17.9),
            TopologyGrid {
                spacing_m: 0.0,
                ..topology_grid
            },
        ),
        None,
    );
}
