use avian3d::parry::shape::SharedShape;
use avian3d::parry::utils::Array2;
use avian3d::prelude::*;
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

use crate::placement::object_placement_preview_permission_projection::ObjectPlacementHeadroomQueries;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_collision_rebuilding_types::TerrainChunkCollisionSurface;

fn spawn_terrain_surface(world: &mut World, root: Entity, heights: Vec<f32>) -> (Entity, Entity) {
    let chunk = world
        .spawn((
            TerrainChunk {
                source_coord: IVec2::ZERO,
                coord: IVec2::ZERO,
                asset: Handle::default(),
                origin: Vec2::ZERO,
                spacing_m: 2.0,
                side: 2,
            },
            WorldMember { root },
            Transform::IDENTITY,
            RigidBody::Static,
        ))
        .id();
    let surface = world
        .spawn((
            TerrainChunkCollisionSurface,
            ChildOf(chunk),
            Transform::from_xyz(1.0, 0.0, 1.0),
            Collider::from(SharedShape::heightfield(
                Array2::new(2, 2, heights),
                Vec3::new(2.0, 1.0, 2.0),
            )),
        ))
        .id();
    (chunk, surface)
}

fn create_headroom_test_application() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        PhysicsPlugins::default(),
    ))
    .init_asset::<Mesh>()
    .init_resource::<TerrainIndex>()
    .insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
    app.finish();
    app.cleanup();
    app
}

// This crosses the ECS ownership / Avian geometry boundary: the nearest
// surface must change without losing the obstruction behind excluded ground.
#[test]
fn flattening_occupied_ground_preserves_overhead_and_unrelated_terrain_obstructions(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = create_headroom_test_application();
    let world = app.world_mut().spawn_empty().id();
    let other_world = app.world_mut().spawn_empty().id();
    let preview = app.world_mut().spawn_empty().id();
    let (chunk, ground) = spawn_terrain_surface(app.world_mut(), world, vec![8.0, 8.5, 8.0, 8.5]);
    {
        let mut index = app.world_mut().resource_mut::<TerrainIndex>();
        index.chunk_span_m = 2.0;
        index.chunks.insert(IVec2::ZERO, chunk);
    }
    app.update();
    app.update();

    // At the centre, the independently specified plane is 8.25m high.
    // The unchanged ray starts at placement height 8m plus 1cm.
    let mut queries = SystemState::<ObjectPlacementHeadroomQueries>::new(app.world_mut());
    let cast =
        |app: &App, queries: &mut SystemState<ObjectPlacementHeadroomQueries>, flattening_world| {
            queries.get(app.world()).map(|headroom| {
                headroom.cast_occupied_cell_headroom_ray(
                    Vec3::new(1.0, 8.01, 1.0),
                    5.0,
                    preview,
                    None,
                    flattening_world,
                    None,
                )
            })
        };
    let hit = cast(&app, &mut queries, None)?.ok_or("expected ground intersection")?;
    assert_eq!(hit.entity, ground);
    assert!((hit.distance - 0.24).abs() < 0.0001);
    assert!(cast(&app, &mut queries, Some(world))?.is_none());
    assert_eq!(
        cast(&app, &mut queries, Some(other_world))?.map(|hit| hit.entity),
        Some(ground)
    );

    app.world_mut()
        .entity_mut(ground)
        .remove::<TerrainChunkCollisionSurface>();
    assert_eq!(
        cast(&app, &mut queries, Some(world))?.map(|hit| hit.entity),
        Some(ground)
    );
    app.world_mut()
        .entity_mut(ground)
        .insert(TerrainChunkCollisionSurface);

    app.world_mut().entity_mut(chunk).remove::<WorldMember>();
    assert_eq!(
        cast(&app, &mut queries, Some(world))?.map(|hit| hit.entity),
        Some(ground)
    );
    app.world_mut()
        .entity_mut(chunk)
        .insert(WorldMember { root: world });
    app.world_mut()
        .resource_mut::<TerrainIndex>()
        .chunks
        .clear();
    assert_eq!(
        cast(&app, &mut queries, Some(world))?.map(|hit| hit.entity),
        Some(ground)
    );
    app.world_mut()
        .resource_mut::<TerrainIndex>()
        .chunks
        .insert(IVec2::ZERO, chunk);

    let overhead = app
        .world_mut()
        .spawn((
            RigidBody::Static,
            Transform::from_xyz(1.0, 10.0, 1.0),
            Collider::cuboid(1.0, 1.0, 1.0),
        ))
        .id();
    app.update();
    let hit = cast(&app, &mut queries, Some(world))?.ok_or("expected obstruction")?;
    assert_eq!(hit.entity, overhead);
    assert!((hit.distance - 1.49).abs() < 0.0001);

    // A marked terrain surface belonging to another chunk must also remain
    // an obstruction, even when that chunk belongs to this same world.
    let (_, unrelated_surface) = spawn_terrain_surface(app.world_mut(), world, vec![9.0; 4]);
    app.update();
    let hit = cast(&app, &mut queries, Some(world))?.ok_or("expected obstruction")?;
    assert_eq!(hit.entity, unrelated_surface);
    assert!((hit.distance - 0.99).abs() < 0.0001);
    Ok(())
}
