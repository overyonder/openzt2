use bevy::prelude::*;

use super::{
    habitat_membership_and_containment::locate_habitat_at_world_position,
    habitat_types::HabitatIndex,
};
use crate::plugins::topology::topology_graph_types::TopologyGrid;

#[test]
fn preassigned_habitat_membership_still_initializes_containment() {
    use super::{
        habitat_membership_and_containment::update_habitat_membership_after_locatable_entities_move,
        habitat_types::{
            Containment, ContainmentChanged, HabitatLocatable, HabitatMember, HabitatSummary,
        },
    };
    for breached in [false, true] {
        let mut app = App::new();
        app.init_resource::<TopologyGrid>()
            .add_message::<ContainmentChanged>()
            .add_systems(
                Update,
                update_habitat_membership_after_locatable_entities_move,
            );
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
        index.record_habitat_for_topology_cell(IVec2::ZERO, habitat);
        app.insert_resource(index);
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
                habitat_entity: habitat,
                is_contained: !breached,
            })
        );
    }
}

#[test]
fn world_position_habitat_lookup_uses_the_topology_grid_origin_and_spacing() {
    let habitat_entity = Entity::from_bits(77);
    let mut habitat_index = HabitatIndex::default();
    habitat_index.record_habitat_for_topology_cell(IVec2::new(1, 2), habitat_entity);
    let topology_grid = TopologyGrid {
        origin: Vec2::new(10.0, -20.0),
        spacing_m: 0.75,
    };
    assert_eq!(
        locate_habitat_at_world_position(&habitat_index, Vec2::new(10.8, -17.9), topology_grid,),
        Some(habitat_entity),
    );
    assert_eq!(
        locate_habitat_at_world_position(&habitat_index, Vec2::new(10.7, -17.9), topology_grid,),
        None,
    );
    assert_eq!(
        locate_habitat_at_world_position(
            &habitat_index,
            Vec2::new(10.8, -17.9),
            TopologyGrid {
                spacing_m: 0.0,
                ..topology_grid
            },
        ),
        None,
    );
}
