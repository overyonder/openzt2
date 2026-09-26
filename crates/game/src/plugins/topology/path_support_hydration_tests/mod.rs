use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{
    path_support_hydration::remove_path_supports_after_their_owning_path_is_removed,
    topology_graph_types::{PathSupport, PathTile},
};

#[test]
fn orphan_support_is_removed_only_after_its_path() {
    let mut app = App::new();
    app.add_systems(
        Update,
        remove_path_supports_after_their_owning_path_is_removed,
    );
    let path = app
        .world_mut()
        .spawn(PathTile {
            definition: AssetId::default(),
            cell: IVec3::Y,
        })
        .id();
    let support = app
        .world_mut()
        .spawn(PathSupport {
            path,
            ground_height_m: 0.0,
        })
        .id();
    app.update();
    assert!(app.world().get_entity(support).is_ok());
    app.world_mut().despawn(path);
    app.update();
    app.update();
    assert!(app.world().get_entity(support).is_err());
}
