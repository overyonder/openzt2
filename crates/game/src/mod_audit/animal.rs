use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    prefab_presentation_types::PrefabModel, world_membership_types::WorldRoot,
};
use openzt2_zoo_state::{
    animal_lifecycle::{
        animal_adoption_contracts::{AdoptAnimal, AnimalAdopted},
        types::AnimalPresentationOwner,
    },
    terrain::{terrain_chunk_types::TerrainChunk, terrain_world_sampling::sample_terrain},
};

#[derive(Resource)]
pub(super) struct AnimalProbe {
    species: AssetId,
    requested: bool,
    adopted: Option<Entity>,
    presented: bool,
}

impl AnimalProbe {
    pub(super) fn presented(&self) -> bool {
        self.presented
    }

    pub(super) fn stage(&self) -> &'static str {
        if self.adopted.is_some() {
            "waiting for model"
        } else if self.requested {
            "waiting for adoption"
        } else {
            "finding a dry placement point"
        }
    }
}

pub(super) fn install(app: &mut App) {
    let Ok(species) = std::env::var("OPENZT2_MOD_AUDIT_ANIMAL") else {
        return;
    };
    app.insert_resource(AnimalProbe {
        species: AssetId::from_key(&species),
        requested: false,
        adopted: None,
        presented: false,
    })
    .add_systems(Last, exercise_animal.run_if(in_state(GamePhase::InGame)));
}

#[allow(clippy::too_many_arguments)]
#[expect(clippy::needless_pass_by_value)]
fn exercise_animal(
    mut probe: ResMut<AnimalProbe>,
    worlds: Query<Entity, With<WorldRoot>>,
    chunks: Query<&TerrainChunk>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    models: Query<(&AnimalPresentationOwner, &PrefabModel)>,
    server: Res<AssetServer>,
    mut adopt: MessageWriter<AdoptAnimal>,
    mut adopted: MessageReader<AnimalAdopted>,
) {
    if probe.presented {
        return;
    }
    if !probe.requested {
        let Ok(world) = worlds.single() else {
            return;
        };
        let position = chunks
            .iter()
            .filter_map(|chunk| {
                let asset = terrain_assets.get(&chunk.asset)?;
                let position =
                    chunk.origin + Vec2::splat(f32::from(chunk.side / 2) * chunk.spacing_m);
                let sample = sample_terrain(chunk, asset, None, position)?;
                (sample.water_height_m.is_none()).then_some(Vec3::new(
                    position.x,
                    sample.height_m,
                    position.y,
                ))
            })
            .min_by(|left, right| left.length_squared().total_cmp(&right.length_squared()));
        let Some(position) = position else {
            return;
        };
        adopt.write(AdoptAnimal {
            species: probe.species,
            variant: None,
            sex: None,
            offer_slot_index: None,
            world,
            habitat: None,
            transform: Transform::from_translation(position),
        });
        probe.requested = true;
        println!("mod-audit\tanimal-request\t{:?}", probe.species);
    }
    let species = probe.species;
    for event in adopted.read().filter(|event| event.species == species) {
        probe.adopted = Some(event.animal);
        println!("mod-audit\tanimal-adopted\t{:?}", event.animal);
    }
    if let Some(animal) = probe.adopted {
        if let Some((_, model)) = models.iter().find(|(owner, model)| {
            owner.0 == animal
                && model
                    .handle
                    .as_ref()
                    .is_some_and(|handle| server.is_loaded_with_dependencies(handle.id()))
        }) {
            probe.presented = true;
            println!("mod-audit\tanimal-model-loaded\t{}", model.model_path);
        }
    }
}
