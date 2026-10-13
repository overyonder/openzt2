use bevy::prelude::*;
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueCategory;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_is_available;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::information::catalogue_types::PurchaseChoice;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushKind;

use openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::BiomeSurface;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::SelectConstructionTool;

/// Hands a catalogue purchase to the construction owner only when the selected
/// canonical definition is an authored object, fence, path, biome, or transport
/// track. Animal adoption, staff, and other catalogue choices remain available
/// to independent readers. Choosing a locked entry ends the placement begun by
/// an earlier choice, since the selection has moved on to research.
#[allow(clippy::too_many_arguments)]
pub(super) fn route_construction_purchase_choices(
    mut purchases: MessageReader<PurchaseChoice>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    worlds: Query<(&SelectedWorldIdentity, Option<&ScenarioContentAvailability>), With<WorldRoot>>,
    mut policy: ResMut<ConstructionPlacementPolicy>,
    mut tool: ResMut<ConstructionTool>,
    mut selections: MessageWriter<SelectConstructionTool>,
    mut cancel: MessageWriter<CancelConstruction>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let world = worlds.iter().next();
    for purchase in purchases.read() {
        if catalogue
            .catalogue()
            .enumerate()
            .find(|(_, entry)| entry.definition == purchase.definition)
            .is_some_and(|(index, entry)| {
                !catalogue_entry_is_available(
                    catalogue,
                    index,
                    entry,
                    world.map(|(identity, _)| identity.mode),
                    world.and_then(|(_, scenario)| scenario),
                    &unlocks,
                )
            })
        {
            // Selecting the purchase tool directly would also clear the
            // catalogue selection, so leave the placement the way Escape does.
            if matches!(
                *tool,
                ConstructionTool::Place(_)
                    | ConstructionTool::Fence(_)
                    | ConstructionTool::Path(_)
                    | ConstructionTool::GroundTrackPlacement(Some(_))
                    | ConstructionTool::SkyTrackPlacement(Some(_))
                    | ConstructionTool::Terrain(TerrainBrushKind::Paint { .. })
            ) {
                cancel.write(CancelConstruction);
                *tool = ConstructionTool::Inspect;
            }
            continue;
        }

        if catalogue
            .catalogue()
            .find(|entry| openzt2_game_data::AssetId(entry.definition.0) == purchase.definition)
            .is_some_and(|entry| {
                matches!(
                    &entry.category,
                    CatalogueCategory::Animals | CatalogueCategory::Staff
                )
            })
        {
            continue;
        }
        if catalogue.find_vehicle(purchase.definition).is_some() {
            continue;
        }
        let tool = if catalogue.find_biome(purchase.definition).is_some() {
            policy.selected_biome = purchase.definition;
            Some(ConstructionTool::Terrain(TerrainBrushKind::Paint {
                biome: purchase.definition,
                ground_cover: match policy.biome_surface {
                    BiomeSurface::Ground => Some(false),
                    BiomeSurface::GroundCover => Some(true),
                    _ => None,
                },
            }))
        } else if catalogue.find_fence(purchase.definition).is_some() {
            Some(ConstructionTool::Fence(purchase.definition))
        } else if catalogue.find_path(purchase.definition).is_some() {
            Some(ConstructionTool::Path(purchase.definition))
        } else if let Some(track) = catalogue.find_track(purchase.definition) {
            Some(match track.kind {
                TransportationTrackKind::Ground => {
                    ConstructionTool::GroundTrackPlacement(Some(purchase.definition))
                }
                TransportationTrackKind::Sky => {
                    ConstructionTool::SkyTrackPlacement(Some(purchase.definition))
                }
            })
        } else if catalogue.find_placeable(purchase.definition).is_some() {
            Some(ConstructionTool::Place(purchase.definition))
        } else {
            None
        };
        if let Some(tool) = tool {
            selections.write(SelectConstructionTool(tool));
        }
    }
}
