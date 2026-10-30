use bevy::{platform::collections::HashSet, prelude::*};

/// Direction cells covered by a path record; a centre record covers all four.
pub fn ground_path_tile_triangles(
    cell: IVec3,
    centre: IVec3,
    width_cells: i32,
) -> impl Iterator<Item = IVec3> {
    let whole = cell.xy() == centre.xy();
    [IVec3::Y, IVec3::X, -IVec3::Y, -IVec3::X]
        .into_iter()
        .map(move |side| centre + side * (width_cells / 2))
        .filter(move |triangle| whole || *triangle == cell)
}

/// Exposed edges, ordered with the outside on the right. Of two adjoining
/// triangles, the counter-clockwise one owns their diagonal boundary.
pub fn ground_path_triangle_curb_boundary(
    triangles: &HashSet<IVec3>,
    centre: IVec3,
    triangle: IVec3,
) -> impl Iterator<Item = (IVec3, IVec3)> {
    let side = (triangle - centre).signum();
    let half = (triangle - centre).abs().max_element();
    let width = half * 2;
    let ccw = IVec3::new(-side.y, side.x, 0);
    let occupied = |direction: IVec3| triangles.contains(&(centre + direction * half));
    let outer = (!triangles.contains(&(centre + side * (width * 2 - half))))
        .then(|| (centre + (side - ccw) * width, centre + (side + ccw) * width));
    let diagonal = (occupied(-ccw) && !occupied(ccw) && !occupied(-side))
        .then(|| (centre + (side + ccw) * width, centre - (side + ccw) * width));
    outer.into_iter().chain(diagonal)
}
