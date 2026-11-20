use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::topology::path_support_hydration::remove_supports_when_path_tile_is_removed;
use openzt2_zoo_state::topology::topology_graph_types::PathSupport;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;

#[test]
fn orphan_support_is_removed_only_after_its_path() {
    let mut app = App::new();
    app.add_observer(remove_supports_when_path_tile_is_removed);
    let path = app
        .world_mut()
        .spawn(PathTile {
            definition: AssetId::default(),
            cell: IVec3::Y,
        })
        .id();
    let support = app
        .world_mut()
        .spawn((
            PathSupport {
                ground_height_m: 0.0,
            },
            ChildOf(path),
        ))
        .id();
    app.update();
    assert!(app.world().get_entity(support).is_ok());
    app.world_mut().entity_mut(path).remove::<PathTile>();
    app.update();
    assert!(app.world().get_entity(support).is_err());
}
