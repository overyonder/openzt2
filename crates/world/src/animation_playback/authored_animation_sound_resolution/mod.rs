use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    audio::{
        audio_asset_types::{AudioAsset, AudioAssets},
        source::xml::cue_id,
    },
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
    world_spawn::world_membership_types::DefinitionId,
};
use openzt2_zoo_state::{
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_sample_grid_queries::read_materialized_terrain_sample,
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    topology::topology_graph_types::{PathTile, TopologyGrid, TopologyIndex},
};

#[derive(SystemParam)]
pub(super) struct AuthoredAnimationSoundResolution<'w, 's> {
    audio_index: Option<Res<'w, AudioAssets>>,
    audio_assets: Option<Res<'w, Assets<AudioAsset>>>,
    definitions: Option<Res<'w, WorldDefinitions>>,
    definition_assets: Option<Res<'w, Assets<WorldDefinitionAsset>>>,
    identities: Query<
        'w,
        's,
        (
            Option<&'static DefinitionId>,
            Option<&'static AnimationPresentationOwner>,
        ),
    >,
    parents: Query<'w, 's, &'static ChildOf>,
    terrain_index: Option<Res<'w, TerrainIndex>>,
    terrain_assets: Option<Res<'w, Assets<TerrainAsset>>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    topology_index: Option<Res<'w, TopologyIndex>>,
    topology_grid: Option<Res<'w, TopologyGrid>>,
    paths: Query<'w, 's, &'static PathTile>,
}

impl AuthoredAnimationSoundResolution<'_, '_> {
    pub(super) fn resolve(
        &self,
        target: Entity,
        position: Vec3,
        name: &str,
        terrain_sound: bool,
    ) -> Option<AssetId> {
        let audio = self
            .audio_index
            .as_ref()?
            .get(self.audio_assets.as_ref()?)?;
        let definitions = self
            .definitions
            .as_ref()
            .zip(self.definition_assets.as_ref())
            .and_then(|(index, assets)| index.get(assets));
        let identity = std::iter::once(target)
            .chain(self.parents.iter_ancestors(target))
            .find_map(|entity| {
                let (definition, owner) = self.identities.get(entity).ok()?;
                definition
                    .copied()
                    .or_else(|| self.identities.get(owner?.gameplay_entity).ok()?.0.copied())
            });
        let prefixes = identity
            .and_then(|identity| definitions?.find_object(identity.0))
            .map_or(&[][..], |definition| {
                definition.audio_cue_prefixes.as_slice()
            });
        let mut suffixes = vec![String::new()];
        if terrain_sound {
            let surface = self.terrain_surface(position);
            let path = self
                .topology_index
                .as_ref()
                .zip(self.topology_grid.as_ref())
                .and_then(|(index, grid)| index.paths.get(&grid.position_cell(position)))
                .and_then(|entity| self.paths.get(*entity).ok())
                .and_then(|tile| definitions?.find_object(tile.definition))
                .and_then(|definition| definition.audio_cue_prefixes.first());
            if let Some(path) = path {
                suffixes.push(format!("_{path}"));
            } else if let Some((biome, _, _)) = surface {
                suffixes.push(format!("_{biome}"));
            }
            suffixes.push("_biome".to_owned());
            if let Some((biome, covered, _)) = surface.filter(|_| path.is_none()) {
                let layer = if covered { "cover" } else { "dirt" };
                suffixes.push(format!("_{layer}"));
                suffixes.push(format!("_{biome}_{layer}"));
            }
            if let Some(water) = surface.and_then(|(_, _, water)| water) {
                suffixes.push("_water".to_owned());
                suffixes.push(format!("_{water}"));
            }
        }
        for suffix in suffixes.iter().rev() {
            for prefix in prefixes {
                let id = cue_id(&format!("{prefix}_{name}{suffix}"));
                if audio.cue(id).is_some() {
                    return Some(id);
                }
            }
        }
        for suffix in suffixes.iter().rev() {
            let id = cue_id(&format!("{name}{suffix}"));
            if audio.cue(id).is_some() {
                return Some(id);
            }
        }
        None
    }

    fn terrain_surface(&self, position: Vec3) -> Option<(&str, bool, Option<&str>)> {
        let entity = terrain_chunk_at(self.terrain_index.as_ref()?, position.xz())?;
        let (chunk, edited) = self.chunks.get(entity).ok()?;
        let asset = self.terrain_assets.as_ref()?.get(&chunk.asset)?;
        let surface = sample_terrain(chunk, asset, edited, position.xz())?;
        let sample = ((position.xz() - chunk.origin) / chunk.spacing_m).round();
        let sample = read_materialized_terrain_sample(
            chunk,
            asset,
            edited,
            sample.x as usize,
            sample.y as usize,
        )?;
        let biome = asset
            .canonical_terrain_grid()
            .biomes
            .get(surface.surface as usize)?;
        let water = surface
            .water_height_m
            .filter(|height| position.y <= *height)
            .and_then(|_| {
                asset
                    .canonical_terrain_grid()
                    .biomes
                    .get(usize::from(sample.water_biome_channel))
            })
            .map(|biome| biome.name.as_str());
        Some((&biome.name, sample.ground_cover != 0, water))
    }
}
