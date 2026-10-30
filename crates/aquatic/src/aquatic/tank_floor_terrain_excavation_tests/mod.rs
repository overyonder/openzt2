use bevy::prelude::*;

use super::tank_floor_terrain_excavation::strictly_inside_walls;

#[test]
fn tank_floor_excavation_takes_samples_inside_the_walls_but_not_on_them() {
    let corners = [
        Vec2::new(0.0, 0.0),
        Vec2::new(3.0, 0.0),
        Vec2::new(3.0, 3.0),
        Vec2::new(0.0, 3.0),
    ];
    let walls = (0..4)
        .map(|side| [corners[side], corners[(side + 1) % 4]])
        .collect::<Vec<_>>();
    let inside = (0..=3)
        .flat_map(|z| (0..=3).map(move |x| Vec2::new(x as f32, z as f32)))
        .filter(|point| strictly_inside_walls(*point, &walls))
        .collect::<Vec<_>>();
    assert_eq!(
        inside,
        [
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 1.0),
            Vec2::new(1.0, 2.0),
            Vec2::new(2.0, 2.0),
        ]
    );
}
