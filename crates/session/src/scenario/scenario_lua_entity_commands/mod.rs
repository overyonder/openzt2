//! Entity operations staged by one Lua invocation and applied by gameplay owners.
mod placement;
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_animals::animal_lifecycle::scripted_animal_spawning::{
    resolve_scripted_animal_spawn, ScriptedAnimalSpawnRequest,
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::{
        deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
        simulation_clock_types::ZooClock,
    },
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        world_entity_crating::WorldEntityIsCrated,
        world_membership_types::{DefinitionId, WorldMember, WorldRoot},
    },
};
use openzt2_zoo_state::{
    animal_lifecycle::types::Animal,
    animal_welfare::types::SetAnimalNeed,
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
};
use std::cell::RefCell;

#[derive(Message)]
pub(super) enum ScenarioEntityCommand {
    Spawn(ScriptedAnimalSpawnRequest),
    Need(SetAnimalNeed),
    Crate(Entity),
    Name(Entity),
}

#[derive(Default)]
pub(super) struct EntityOperations {
    pub(super) spawns: Vec<ScriptedAnimalSpawnRequest>,
    pub(super) commands: Vec<ScenarioEntityCommand>,
}

impl EntityOperations {
    pub(super) fn finish(mut self) -> Vec<ScenarioEntityCommand> {
        let mut commands = self
            .spawns
            .drain(..)
            .map(ScenarioEntityCommand::Spawn)
            .collect::<Vec<_>>();
        commands.append(&mut self.commands);
        commands
    }
}

#[derive(SystemParam)]
pub(super) struct ScenarioAnimalSpawnSources<'w, 's> {
    commands: Commands<'w, 's>,
    allocator: Option<ResMut<'w, PersistentIdAllocator>>,
    roots: Query<'w, 's, Entity, With<WorldRoot>>,
    species: Res<'w, SpeciesAssets>,
    species_assets: Res<'w, Assets<SpeciesAsset>>,
    definitions: Res<'w, WorldDefinitions>,
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    seed: Option<Res<'w, ZooSeed>>,
    clock: Option<Res<'w, ZooClock>>,
    terrain: Res<'w, TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    animals: Query<'w, 's, (&'static openzt2_zoo_state::animal_lifecycle::types::AnimalVariant, &'static Transform), With<Animal>>,
    localization: Res<'w, openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex>,
    localized_assets: Res<'w, Assets<openzt2_game_assets::localization::localization_asset_types::LocalizationAsset>>,
    obstacles: placement::ScriptedAnimalPlacementObstacles<'w, 's>,
}

impl ScenarioAnimalSpawnSources<'_, '_> {
    pub(super) fn resolve(
        &mut self,
        kind: AssetId,
        source_position: [f32; 3],
        rotation: f32,
        reserved: &[ScriptedAnimalSpawnRequest],
    ) -> mlua::Result<ScriptedAnimalSpawnRequest> {
        let unavailable = || {
            mlua::Error::runtime(
                "scripted animal placement requires a loaded zoo and animal catalogue",
            )
        };
        if !source_position.iter().all(|v| v.is_finite()) || !rotation.is_finite() {
            return Err(mlua::Error::runtime(
                "animal placement coordinates must be finite",
            ));
        }
        let root = self.roots.single().map_err(|_| unavailable())?;
        let world = WorldMember { root };
        let allocator = self.allocator.as_mut().ok_or_else(unavailable)?;
        if !allocator.owns_world(root) {
            return Err(unavailable());
        }
        let species_view = self
            .species
            .get(&self.species_assets)
            .ok_or_else(unavailable)?;
        let definitions = self
            .definitions
            .get(&self.definition_assets)
            .ok_or_else(unavailable)?;
        let mut rng = DeterministicRng::from_entity(
            **self.seed.as_ref().ok_or_else(unavailable)?,
            PersistentId(allocator.next()),
            RngDomain::Reproduction,
        );
        let (species, components) = resolve_scripted_animal_spawn(
            species_view,
            kind,
            definitions.timing().ticks_per_day,
            &mut rng,
        )
        .ok_or_else(|| {
            mlua::Error::runtime("placement object is not an available animal variant")
        })?;
        let mut position = Vec3::from_array(openzt2_game_assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates(source_position));
        let variant = species_view
            .find_variant(components.variant)
            .ok_or_else(unavailable)?;
        let policy = variant
            .navigation_collision_policy
            .as_ref()
            .ok_or_else(|| {
                mlua::Error::runtime("animal placement has no authored collision policy")
            })?;
        let radius = policy.radius_m * components.scale;
        let height = policy.height_m * components.scale;
        if !radius.is_finite() || radius <= 0.0 || !height.is_finite() || height <= 0.0 {
            return Err(mlua::Error::runtime(
                "animal placement collision dimensions are invalid",
            ));
        }
        let occupied = self
            .animals
            .iter()
            .filter_map(|(variant, transform)| {
                let other = species_view
                    .find_variant(variant.0)?
                    .navigation_collision_policy
                    .as_ref()?;
                Some((
                    transform.translation.xz(),
                    other.radius_m * transform.scale.max_element(),
                ))
            })
            .chain(reserved.iter().filter_map(|request| {
                let other = species_view
                    .find_variant(request.components.variant)?
                    .navigation_collision_policy
                    .as_ref()?;
                Some((
                    request.transform.translation.xz(),
                    other.radius_m * request.components.scale,
                ))
            }))
            .collect::<Vec<_>>();
        let origin = position.xz();
        // A finite outward square spiral keeps a bad authored site from monopolizing a script tick.
        let mut offset = IVec2::ZERO;
        let mut direction = IVec2::X;
        let mut run_length = 1;
        let mut run_progress = 0;
        let mut turns = 0;
        let mut chosen = None;
        for _ in 0..256 {
            let candidate = origin + offset.as_vec2() * (2.0 * radius);
            if occupied.iter().all(|(center, other)| {
                candidate.distance_squared(*center) >= (radius + other).powi(2)
            }) {
                if let Some((chunk, edited)) = terrain_chunk_at(&self.terrain, candidate)
                    .and_then(|entity| self.chunks.get(entity).ok())
                {
                    if let Some(point) = self
                        .terrain_assets
                        .get(&chunk.asset)
                        .and_then(|asset| sample_terrain(chunk, asset, edited, candidate))
                    {
                        let grounded =
                            Vec3::new(candidate.x, point.height_m + position.y, candidate.y);
                        if point.water_height_m.is_none()
                            && point.normal.y >= policy.maximum_slope_radians.cos()
                            && animal_footprint_has_dry_terrain(
                                candidate,
                                radius,
                                policy.maximum_slope_radians,
                                &self.terrain,
                                &self.terrain_assets,
                                &self.chunks,
                            )
                            && !self.obstacles.overlaps(
                                grounded,
                                radius,
                                height,
                                definitions,
                                &variant.behavior_subject_type_identifiers,
                            )
                        {
                            chosen = Some(grounded);
                            break;
                        }
                    }
                }
            }
            offset += direction;
            run_progress += 1;
            if run_progress == run_length {
                direction = IVec2::new(-direction.y, direction.x);
                run_progress = 0;
                turns += 1;
                if turns % 2 == 0 {
                    run_length += 1;
                }
            }
        }
        position = chosen.ok_or_else(|| {
            mlua::Error::runtime("no dry unoccupied animal placement site near authored position")
        })?;
        let born_tick = self.clock.as_ref().ok_or_else(unavailable)?.tick;
        let name = definitions
            .find_object(components.definition)
            .and_then(|definition| {
                self.localization
                    .borrow_loaded_localization_view(&self.localized_assets)?
                    .find_plain_localized_text(AssetId(definition.name_key.0))
            })
            .ok_or_else(|| {
                mlua::Error::runtime("animal placement definition has no localized name")
            })?;
        let persistent_id = allocator.allocate(root).map_err(|error| {
            mlua::Error::runtime(format!("animal identity allocation failed: {error:?}"))
        })?;
        let name = Some(format!("{name} {}", persistent_id.0));
        let entity = self.commands.spawn(world).id();
        Ok(ScriptedAnimalSpawnRequest {
            entity,
            species,
            components,
            persistent_id,
            world,
            transform: Transform::from_translation(position)
                .with_rotation(Quat::from_rotation_y(rotation.to_radians())),
            born_tick,
            initial_needs: Vec::new(),
            crated: false,
            name,
        })
    }
}

pub(super) fn register(app: &mut App) {
    app.add_message::<ScenarioEntityCommand>().add_systems(
        Update,
        apply_entity_commands.run_if(in_state(GamePhase::InGame)),
    );
}

fn apply_entity_commands(
    mut commands: Commands,
    mut operations: MessageReader<ScenarioEntityCommand>,
    mut spawns: MessageWriter<ScriptedAnimalSpawnRequest>,
    mut needs: MessageWriter<SetAnimalNeed>,
    animals: Query<(), (With<Animal>, With<WorldMember>)>,
    entities: Query<(&DefinitionId, &PersistentId, Option<&Name>), With<WorldMember>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    localization: Res<openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex>,
    localized_assets: Res<
        Assets<openzt2_game_assets::localization::localization_asset_types::LocalizationAsset>,
    >,
) {
    let definitions = definitions.get(&assets);
    let localization = localization.borrow_loaded_localization_view(&localized_assets);
    for operation in operations.read() {
        match operation {
            ScenarioEntityCommand::Spawn(request) => {
                spawns.write(request.clone());
            }
            ScenarioEntityCommand::Need(request) if animals.contains(request.animal) => {
                needs.write(*request);
            }
            ScenarioEntityCommand::Crate(entity) if animals.contains(*entity) => {
                commands.entity(*entity).insert(WorldEntityIsCrated);
            }
            ScenarioEntityCommand::Name(entity) => {
                if let Ok((definition, id, None)) = entities.get(*entity) {
                    if let Some(label) = definitions
                        .and_then(|v| v.find_object(definition.0))
                        .and_then(|v| {
                            localization?.find_plain_localized_text(AssetId(v.name_key.0))
                        })
                    {
                        commands
                            .entity(*entity)
                            .insert(Name::new(format!("{label} {}", id.0)));
                    }
                }
            }
            _ => {}
        }
    }
}

pub(super) type PendingEntityOperations = RefCell<EntityOperations>;

fn animal_footprint_has_dry_terrain(
    center: Vec2,
    radius: f32,
    maximum_slope: f32,
    index: &TerrainIndex,
    assets: &Assets<TerrainAsset>,
    chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> bool {
    let Some(spacing) = chunks
        .iter()
        .map(|(chunk, _)| chunk.spacing_m)
        .filter(|spacing| spacing.is_finite() && *spacing > 0.0)
        .reduce(f32::min)
    else {
        return false;
    };
    // Half-sample spacing checks the shoreline classification on both sides of
    // every terrain sample under the animal, as well as its outer boundary.
    let steps = (4.0 * radius / spacing).ceil().max(1.0) as u32;
    if steps > 256 {
        return false;
    }
    for y in 0..=steps {
        for x in 0..=steps {
            let point = center + Vec2::new(x as f32, y as f32) * (2.0 * radius / steps as f32)
                - Vec2::splat(radius);
            let sample = terrain_chunk_at(index, point)
                .and_then(|entity| chunks.get(entity).ok())
                .and_then(|(chunk, edited)| {
                    sample_terrain(chunk, assets.get(&chunk.asset)?, edited, point)
                });
            if sample.is_none_or(|sample| {
                sample.water_height_m.is_some() || sample.normal.y < maximum_slope.cos()
            }) {
                return false;
            }
        }
    }
    true
}
