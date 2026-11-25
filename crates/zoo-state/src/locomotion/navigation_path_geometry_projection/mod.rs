use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use crate::topology::topology_graph_types::{PathTile, TileSurface};

use super::terrain_derived_navigation_graph_types::{
    NavigationPathGeometry, NavigationPathSurface,
};

type NavigationPathInputs = (
    Entity,
    &'static PathTile,
    &'static TileSurface,
    &'static GlobalTransform,
    &'static WorldMember,
    Option<&'static mut NavigationPathGeometry>,
);
type ChangedPathInputs = (
    With<PathTile>,
    Or<(
        Changed<PathTile>,
        Changed<TileSurface>,
        Changed<GlobalTransform>,
        Changed<WorldMember>,
    )>,
);
type RetiredPathInputs<'w, 's> = (
    RemovedComponents<'w, 's, PathTile>,
    RemovedComponents<'w, 's, TileSurface>,
    RemovedComponents<'w, 's, GlobalTransform>,
    RemovedComponents<'w, 's, WorldMember>,
    RemovedComponents<'w, 's, Disabled>,
);

#[derive(bevy::ecs::system::SystemParam)]
pub struct NavigationPathProjectionQueries<'w, 's> {
    paths: Query<'w, 's, NavigationPathInputs>,
    changed: Query<'w, 's, Entity, ChangedPathInputs>,
    disabled: Query<
        'w,
        's,
        Entity,
        (
            With<NavigationPathGeometry>,
            Added<Disabled>,
            Allow<Disabled>,
        ),
    >,
    projected: Query<'w, 's, (), (With<NavigationPathGeometry>, Allow<Disabled>)>,
}

pub fn project_navigation_path_geometry(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut paths: NavigationPathProjectionQueries,
    (mut removed_tiles, mut removed_surfaces, mut removed_poses, mut removed_members, mut enabled): RetiredPathInputs,
) {
    for entity in removed_tiles
        .read()
        .chain(removed_surfaces.read())
        .chain(removed_poses.read())
        .chain(removed_members.read())
        .chain(paths.disabled.iter())
    {
        if paths.projected.contains(entity) && !paths.paths.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<NavigationPathGeometry>();
        }
    }
    let refresh_all = definitions.is_changed();
    let Some(definitions) = definitions.get(&assets) else {
        enabled.clear();
        return;
    };
    if refresh_all {
        enabled.clear();
        for (entity, tile, surface, transform, _, previous) in &mut paths.paths {
            project_path(
                &mut commands,
                definitions,
                entity,
                tile,
                surface,
                transform,
                previous,
            );
        }
    } else {
        let changed: bevy::ecs::entity::EntityHashSet =
            paths.changed.iter().chain(enabled.read()).collect();
        let mut rows = paths.paths.iter_many_mut(changed).matched();
        while let Some((entity, tile, surface, transform, _, previous)) = rows.fetch_next() {
            project_path(
                &mut commands,
                definitions,
                entity,
                tile,
                surface,
                transform,
                previous,
            );
        }
    }
}

fn project_path(
    commands: &mut Commands,
    definitions: WorldDefinitionsView<'_>,
    entity: Entity,
    tile: &PathTile,
    surface: &TileSurface,
    transform: &GlobalTransform,
    previous: Option<Mut<NavigationPathGeometry>>,
) {
    let geometry = definitions
        .find_path(tile.definition)
        .and_then(|definition| {
            let geometry = NavigationPathSurface::new(
                entity,
                *transform,
                f32::from(definition.width_cm) * 0.01,
                surface.height_cm,
            )?;
            Some(if definition.elevated {
                NavigationPathGeometry::Elevated(geometry)
            } else {
                NavigationPathGeometry::Ground(geometry)
            })
        });
    match (previous, geometry) {
        (Some(mut previous), Some(geometry)) => {
            previous.set_if_neq(geometry);
        }
        (None, Some(geometry)) => {
            commands.entity(entity).insert(geometry);
        }
        (Some(_), None) => {
            commands.entity(entity).remove::<NavigationPathGeometry>();
        }
        (None, None) => {}
    }
}
