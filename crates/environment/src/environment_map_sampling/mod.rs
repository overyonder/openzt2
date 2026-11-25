use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_zoo_state::environment::environment_state_types::{
    EnvironmentMapSamples, EnvironmentMapTexture, WorldEnvironment,
};

use super::environment_state_types::Daylight;

pub(super) fn sample_changed_environment_maps(
    assets: Res<Assets<WorldDefinitionAsset>>,
    images: Res<Assets<Image>>,
    active: Res<WorldDefinitions>,
    mut definition_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut environments: Query<(
        Ref<WorldEnvironment>,
        Ref<Daylight>,
        &mut EnvironmentMapSamples,
    )>,
) {
    let definitions_changed = active.is_changed()
        | (definition_events
            .read()
            .filter(|event| !matches!(event, AssetEvent::Unused { .. }))
            .count()
            > 0);
    let changed_images: bevy::platform::collections::HashSet<_> = image_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            AssetEvent::Unused { .. } => None,
        })
        .collect();
    let definitions = active.get(&assets);
    for (environment, daylight, mut maps) in &mut environments {
        let inputs_changed =
            definitions_changed || environment.is_changed() || daylight.is_changed();
        if !inputs_changed && changed_images.is_empty() {
            continue;
        }
        let next = (|| {
            let definitions = definitions?;
            let definition = definitions.find_environment(environment.definition)?;
            let rows = definition.map_samples.as_slice();
            let selected = match rows {
                [] => {
                    return Some(EnvironmentMapSamples {
                        water: EnvironmentMapTexture::Unauthored,
                        object: EnvironmentMapTexture::Unauthored,
                    })
                }
                [row] => Some(row),
                _ => rows
                    .iter()
                    .filter(|row| row.day_fraction <= daylight.fraction)
                    .max_by(|left, right| left.day_fraction.total_cmp(&right.day_fraction)),
            };
            let Some(row) = selected else {
                return Some(EnvironmentMapSamples {
                    water: EnvironmentMapTexture::Empty,
                    object: EnvironmentMapTexture::Empty,
                });
            };
            let handles =
                [row.water_texture, row.object_texture].map(|id| definitions.texture_image(id));
            if !inputs_changed
                && !handles
                    .iter()
                    .flatten()
                    .any(|handle| changed_images.contains(&handle.id()))
            {
                return Some((*maps).clone());
            }
            let bind = |id, handle: Option<Handle<Image>>| {
                if id == AssetId::default() {
                    EnvironmentMapTexture::Empty
                } else if let Some(handle) = handle.filter(|handle| images.contains(handle.id())) {
                    EnvironmentMapTexture::Texture(handle)
                } else {
                    EnvironmentMapTexture::Unresolved
                }
            };
            let [water, object] = handles;
            Some(EnvironmentMapSamples {
                water: bind(row.water_texture, water),
                object: bind(row.object_texture, object),
            })
        })()
        .unwrap_or_default();
        maps.set_if_neq(next);
    }
}
