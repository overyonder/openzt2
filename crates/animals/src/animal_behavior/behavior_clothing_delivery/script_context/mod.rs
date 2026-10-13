use super::state::{ClothingScriptTexture, PendingClothingDelivery};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    asset_source::AssetArchives,
    {
        texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image,
        world_definitions::{
            world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
            world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        },
    },
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;

#[derive(SystemParam)]
pub(crate) struct ClothingScriptContext<'w, 's> {
    definitions: Res<'w, WorldDefinitions>,
    assets: Res<'w, Assets<WorldDefinitionAsset>>,
    guests: Query<'w, 's, &'static GuestArchetype>,
    archives: Res<'w, AssetArchives>,
    server: Res<'w, AssetServer>,
}

impl ClothingScriptContext<'_, '_> {
    pub(crate) fn attribute(&self, actor: Entity, name: &str) -> Option<String> {
        let index = match name {
            "s_Torso" => 0,
            "s_Waist" => 1,
            _ => return None,
        };
        let guest = self.guests.get(actor).ok()?;
        self.definitions
            .get(&self.assets)?
            .find_guest(guest.0)?
            .clothing_materials[index]
            .clone()
    }

    pub(crate) fn prepare(
        &self,
        task: &BehaviorTaskExecutionState,
        textures: Vec<ClothingScriptTexture>,
    ) -> mlua::Result<PendingClothingDelivery> {
        let textures = textures
            .into_iter()
            .map(|texture| {
                let path = self
                    .archives
                    .resolve_model_texture_reference(
                        std::path::Path::new(&texture.image),
                        &texture.image,
                    )
                    .ok_or_else(|| {
                        mlua::Error::runtime(format!("missing clothing image {}", texture.image))
                    })?;
                let path = select_bevy_image_asset_path_for_blue_fang_source_image(
                    &path.to_string_lossy(),
                );
                Ok((
                    texture.material,
                    self.server
                        .load_builder()
                        .with_settings(|settings: &mut bevy::image::ImageLoaderSettings| {
                            settings.is_srgb = false;
                        })
                        .load(path),
                ))
            })
            .collect::<mlua::Result<Vec<_>>>()?;
        Ok(PendingClothingDelivery {
            execution_id: task.execution_id,
            origin: task.create_return_frame_after_action(task.action),
            stack_depth: task.stack.len(),
            target: task.target(),
            textures,
        })
    }
}
