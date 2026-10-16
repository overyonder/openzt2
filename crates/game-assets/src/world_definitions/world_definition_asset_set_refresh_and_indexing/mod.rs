pub mod game_context_binder_directory_loading;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use bevy::{asset::AssetLoadFailedEvent, prelude::*};
use openzt2_game_data::{world_definitions::document::WorldDefinitionDocument, AssetId};

use crate::asset_source::AssetArchives;

use super::{
    world_definition_asset_set_state_and_borrowing_queries::{
        kind_range, DefinitionKind, SingletonKind, WorldDefinitions,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    world_definition_load_queue::request_world_definition_handles_without_per_asset_frame_waits,
};

fn world_definition_source_path(path: &Path, binder_directories: &BTreeSet<PathBuf>) -> bool {
    let supported = path.extension().is_some_and(|extension| {
        ["xml", "dl", "maxml", "zt2", "trk", "old"]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    });
    if !supported {
        return false;
    }
    let path = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    // Sky-track definitions load this procedural rope component as a dependency.
    if path == "entities/transportation/track/ai/testropeobj.xml" {
        return false;
    }
    if Path::new(&path)
        .parent()
        .is_some_and(|parent| binder_directories.contains(parent))
    {
        return true;
    }
    if path.starts_with("entities/") {
        return false;
    }
    if path.starts_with("ai/tricks/") {
        return true;
    }
    if path.starts_with("biomes/") {
        let relative = &path["biomes/".len()..];
        return !relative.contains('/') && relative != "biome.xml" && relative.ends_with(".xml");
    }
    if path.starts_with("locations/") {
        return path.ends_with("initiallocs.xml");
    }
    if matches!(
        path.as_str(),
        "ai/guestmgr.xml" | "ai/staffmgr.xml" | "ai/ztai.xml"
    ) {
        return true;
    }
    if path.starts_with("ui/help/") || path.starts_with("ui/zoopedia/entries/") {
        return true;
    }
    if matches!(
        path.as_str(),
        "ui/modes/modes.xml" | "ui/modes/tankmanip.xml"
    ) {
        return true;
    }
    if path.starts_with("world/environments/") {
        return !path.starts_with("world/environments/sky/")
            && !path.starts_with("world/environments/skirt/");
    }
    if path.starts_with("world/cameras/") {
        return true;
    }
    if path.starts_with("world/lights/")
        || matches!(
            path.as_str(),
            "world/worldlights.xml" | "world/retroworldlights.xml"
        )
    {
        return true;
    }
    matches!(
        path.as_str(),
        "config/traversability.xml"
            | "config/adoption.xml"
            | "config/timekeeper.xml"
            | "config/puzzlemgr.xml"
            | "config/showmixer.xml"
            | "config/status.xml"
            | "config/status2.xml"
            | "config/status3.xml"
            | "config/tankmgr.xml"
            | "config/cleantankaction.xml"
    )
}

pub(super) fn refresh_world_definition_asset_set_after_archive_revision(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut definitions: ResMut<WorldDefinitions>,
) {
    // Mutable access marks the resource changed, and many systems redo their
    // work when it changes, so write only when the server is missing.
    if definitions.server.is_none() {
        definitions.server = Some(server.clone());
    }
    let revision = archives.revision();
    if definitions.revision == Some(revision) {
        return;
    }
    let binder_directories =
        game_context_binder_directory_loading::load_game_context_binder_directories(&archives)
            .unwrap_or_else(|error| {
                warn!(%error, "could not read game context binder directories");
                BTreeSet::new()
            });
    let mut paths = archives
        .resolved_paths()
        .1
        .iter()
        .cloned()
        .filter(|path| world_definition_source_path(path, &binder_directories))
        .collect::<Vec<_>>();
    let additive_zoopedia_entry_collection_owner_path = paths
        .iter()
        .filter(|path| {
            path.to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase()
                .starts_with("ui/zoopedia/entries/")
        })
        .min_by_key(|path| {
            path.to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase()
        })
        .cloned();
    paths.retain(|path| {
        !path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
            .starts_with("ui/zoopedia/entries/")
            || additive_zoopedia_entry_collection_owner_path.as_ref() == Some(path)
    });
    paths.sort_by_key(|path| {
        let path = path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if path == "config/timekeeper.xml" {
            0
        } else if path.starts_with("config/") {
            1
        } else {
            2
        }
    });
    for (identifier, asset) in assets.iter() {
        if !asset
            .authored_binder_source_index_revision
            .is_some_and(|loaded_revision| loaded_revision != revision)
            || !matches!(
                server.get_load_state(identifier),
                Some(bevy::asset::LoadState::Loaded)
            )
        {
            continue;
        }
        let Some(handle) = server.get_id_handle(identifier) else {
            continue;
        };
        if let Some(path) = handle.path().filter(|path| {
            paths
                .iter()
                .any(|candidate| candidate.as_path() == path.path())
        }) {
            // New, removed or replaced unselected binders can change uniqueness
            // without a watcher event for any registered file dependency.
            server.reload(path.clone());
            if !definitions
                .handles
                .iter()
                .any(|previous| previous.id() == identifier)
            {
                definitions.handles.push(handle);
            }
        }
    }
    // Keep old handles alive until the replacement queue acquires its handles,
    // including while Bevy's asynchronous reload requests begin.
    definitions.complete = false;
    definitions.total_loads = paths.len();
    definitions.completed_loads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    definitions.loading_task = Some(
        request_world_definition_handles_without_per_asset_frame_waits(
            server.clone(),
            paths,
            std::sync::Arc::clone(&definitions.completed_loads),
        ),
    );
    definitions.revision = Some(revision);
    definitions.dirty = true;
}

pub(super) fn index_loaded_world_definition_assets_by_identifier_and_singleton_kind(
    server: Res<AssetServer>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut load_failures: MessageReader<AssetLoadFailedEvent<WorldDefinitionAsset>>,
    mut definitions: ResMut<WorldDefinitions>,
) {
    let mut binder_consumer_failed = false;
    for failure in load_failures.read() {
        binder_consumer_failed |= assets
            .get(failure.id)
            .is_some_and(|asset| asset.authored_binder_source_index_revision.is_some());
    }
    if definitions.dirty {
        if let Some(task) = definitions.loading_task.as_mut() {
            let Some(handles) = bevy::tasks::block_on(futures_lite::future::poll_once(task)) else {
                return;
            };
            definitions.handles = handles;
            definitions.loading_task = None;
        }
        // A load guard completes before Bevy necessarily inserts its asset
        // into the main world. Index only once those insertions have landed.
        if definitions.handles.iter().any(|handle| {
            !matches!(
                server.get_load_state(handle.id()),
                Some(bevy::asset::LoadState::Loaded | bevy::asset::LoadState::Failed(_))
            )
        }) {
            return;
        }
        if definitions.handles.iter().any(|handle| {
            !matches!(
                server.get_load_state(handle.id()),
                Some(bevy::asset::LoadState::Failed(_))
            ) && assets.get(handle).is_some_and(|asset| {
                asset
                    .authored_binder_source_index_revision
                    .is_some_and(|revision| Some(revision) != definitions.revision)
            })
        }) {
            // A cached load guard can finish before reload starts. Publication
            // of the current binder-index revision, or failure, is required.
            return;
        }
    } else if !assets.is_changed() && !binder_consumer_failed {
        return;
    }

    let mut records = BTreeMap::new();
    let mut singletons = BTreeMap::new();
    for handle in &definitions.handles {
        let Some(asset) = assets.get(handle) else {
            continue;
        };
        if asset.authored_binder_source_index_revision.is_some()
            && matches!(
                server.get_load_state(handle.id()),
                Some(bevy::asset::LoadState::Failed(_))
            )
        {
            // Bevy retains the previous asset on reload failure; an ambiguous
            // binder resolution must not leave its old definitions selectable.
            continue;
        }
        index_world_definition_document_records_and_singletons(
            &asset.document,
            handle,
            &mut records,
            &mut singletons,
        );
    }
    let retained = records
        .values()
        .chain(singletons.values())
        .map(Handle::id)
        .collect::<std::collections::BTreeSet<_>>();
    // Dependency winners come only from retained documents, as the views'
    // lookups never see a document that owns no record or singleton.
    for handle in definitions
        .handles
        .iter()
        .filter(|handle| retained.contains(&handle.id()))
    {
        let Some(asset) = assets.get(handle) else {
            continue;
        };
        for (kind, dependencies) in [
            (DefinitionKind::TextureDependency, &asset.textures),
            (DefinitionKind::ModelDependency, &asset.models),
            (DefinitionKind::EffectDependency, &asset.effects),
            (DefinitionKind::SceneDependency, &asset.scenes),
        ] {
            for dependency in dependencies {
                records.insert((kind, dependency.id), handle.clone());
            }
        }
    }
    let mut unlock_definition_identifiers_by_target = BTreeMap::<AssetId, Vec<AssetId>>::new();
    for ((kind, identifier), handle) in &records {
        if *kind != DefinitionKind::Unlock {
            continue;
        }
        let Some(unlock) = assets.get(handle).and_then(|asset| {
            asset
                .document
                .unlocks
                .iter()
                .find(|unlock| unlock.id == *identifier)
        }) else {
            continue;
        };
        unlock_definition_identifiers_by_target
            .entry(unlock.target)
            .or_default()
            .push(*identifier);
    }
    let mut catalogue_authored_purchase_order = records
        .range(kind_range(DefinitionKind::Catalogue))
        .enumerate()
        .filter_map(|(index, ((_, id), handle))| {
            assets.get(handle).and_then(|asset| {
                asset
                    .document
                    .catalogue
                    .iter()
                    .find(|entry| entry.id == *id)
                    .map(|entry| {
                        (
                            index,
                            *id,
                            entry.authored_purchase_sort_key.as_str(),
                            entry.authored_purchase_sort_fallback_type_name.as_str(),
                            entry.authored_type_registry_source_order,
                        )
                    })
            })
        })
        .collect::<Vec<_>>();
    catalogue_authored_purchase_order.sort_by(|left, right| {
        let authored_comparison = match (left.2.is_empty(), right.2.is_empty()) {
            (true, true) => left.3.cmp(right.3),
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => left.2.cmp(right.2),
        };
        authored_comparison.then_with(|| left.4.cmp(&right.4))
    });
    let catalogue_authored_purchase_order = catalogue_authored_purchase_order
        .into_iter()
        .map(|(index, id, _, _, _)| (index, id))
        .collect();
    let topology_cell_size_cm = definitions
        .handles
        .iter()
        .filter_map(|handle| assets.get(handle))
        .flat_map(|asset| asset.document.paths.iter())
        .map(|path| path.width_cm / 2)
        .min()
        .unwrap_or_default();
    definitions.handles.retain(|handle| {
        retained.contains(&handle.id())
            || !matches!(
                server.get_load_state(handle.id()),
                Some(bevy::asset::LoadState::Loaded | bevy::asset::LoadState::Failed(_))
            )
    });
    if definitions.catalogue_authored_purchase_order != catalogue_authored_purchase_order
        || definitions.unlock_definition_identifiers_by_target
            != unlock_definition_identifiers_by_target
    {
        definitions.catalogue_revision = definitions.catalogue_revision.wrapping_add(1);
    }
    definitions.records = records;
    definitions.singletons = singletons;
    definitions.unlock_definition_identifiers_by_target = unlock_definition_identifiers_by_target;
    definitions.catalogue_authored_purchase_order = catalogue_authored_purchase_order;
    definitions.topology_cell_size_cm = topology_cell_size_cm;
    definitions.pending_assets = definitions
        .handles
        .iter()
        .any(|handle| assets.get(handle).is_none());
    definitions.dirty = false;
    definitions.complete = true;
}

/// Later documents in precedence order replace earlier winners for the same key.
pub(super) fn index_world_definition_document_records_and_singletons(
    document: &WorldDefinitionDocument,
    handle: &Handle<WorldDefinitionAsset>,
    records: &mut BTreeMap<(DefinitionKind, AssetId), Handle<WorldDefinitionAsset>>,
    singletons: &mut BTreeMap<SingletonKind, Handle<WorldDefinitionAsset>>,
) {
    macro_rules! index {
        ($kind:ident, $field:ident) => {
            document.$field.iter().for_each(|record| {
                records.insert((DefinitionKind::$kind, record.id), handle.clone());
            });
        };
    }
    index!(Object, objects);
    index!(Placeable, placeables);
    index!(Facility, facilities);
    index!(Maintenance, maintenance_definitions);
    index!(Staff, staff);
    index!(StaffJob, staff_jobs);
    index!(StaffRequest, staff_requests);
    index!(Guest, guests);
    index!(ViewingOpportunity, viewing_opportunities);
    index!(Fence, fences);
    index!(Path, paths);
    index!(Biome, biomes);
    index!(Location, locations);
    index!(Brush, brushes);
    index!(Disease, diseases);
    index!(Treatment, treatments);
    index!(Tranquilizer, tranquilizers);
    index!(Rampage, rampage_rules);
    index!(Catalogue, catalogue);
    index!(Zoopedia, zoopedia);
    index!(Research, research);
    index!(Unlock, unlocks);
    index!(Rating, rating_definitions);
    document.fame_thresholds.iter().for_each(|record| {
        records.insert(
            (
                DefinitionKind::FameThreshold,
                AssetId::from_key(&format!("fame-threshold:{}", record.level)),
            ),
            handle.clone(),
        );
    });
    index!(Award, awards);
    index!(Tank, tanks);
    document.aquatic_requirements.iter().for_each(|record| {
        records.insert((DefinitionKind::Aquatic, record.species), handle.clone());
    });
    index!(ShowStage, show_stages);
    index!(Trick, tricks);
    index!(TrickOutcome, trick_outcome_tokens);
    index!(ShowRule, show_rules);
    index!(ShowAudioCue, show_audio_cues);
    index!(ShowIcon, show_presentation_icons);
    index!(Station, stations);
    index!(Track, tracks);
    index!(Vehicle, vehicles);
    index!(VehicleSeat, vehicle_seats);
    index!(TourView, tour_views);
    index!(FossilSet, fossil_sets);
    index!(FossilPiece, fossil_pieces);
    index!(FossilSlot, fossil_slots);
    index!(CloningCenter, cloning_centers);
    index!(ImmersiveModePolicy, immersive_mode_policies);
    index!(Camera, cameras);
    index!(PersonNames, person_name_pools);
    index!(Weather, weather);
    index!(Ambient, ambient_spawns);
    index!(Environment, environments);
    // Keyed by the record's object; a lookup takes the winning document's
    // first record for that object.
    macro_rules! index_by_object {
        ($kind:ident, $field:ident) => {
            document.$field.iter().for_each(|record| {
                records.insert((DefinitionKind::$kind, record.object), handle.clone());
            });
        };
    }
    index_by_object!(MaintenanceByObject, maintenance_definitions);
    index_by_object!(FacilityByObject, facilities);
    index_by_object!(StaffByObject, staff);

    macro_rules! singleton {
        ($kind:ident, $field:ident) => {
            if document.$field.is_some() {
                singletons.insert(SingletonKind::$kind, handle.clone());
            }
        };
    }
    singleton!(Cleanliness, cleanliness_policy);
    singleton!(GuestViewing, guest_viewing);
    singleton!(BiomeDetailPlacement, biome_detail_placement);
    singleton!(ShowPlatformUpgrades, show_platform_upgrades);
    singleton!(FossilPlacement, fossil_placement);
    singleton!(SimulationTiming, simulation_timing);
    singleton!(GuestGeneration, guest_generation);
    singleton!(BehaviorSelection, behavior_selection_policy);
    singleton!(NeedAdjustment, need_adjustment);
    singleton!(TranquilizerMode, tranquilizer_mode);
    singleton!(AdmissionPriceBands, admission_price_bands_cents);
    singleton!(TankSurface, tank_surface_policy);
    singleton!(TankDepth, tank_depth_policy);
    singleton!(TankEdit, tank_edit_policy);
    singleton!(ShowPresentation, show_presentation);
    singleton!(ShowScheduling, show_scheduling);
    singleton!(ShowScoring, show_scoring);
    singleton!(ShowEditorPresentation, show_editor_presentation);
    singleton!(TourScoring, tour_scoring);
    singleton!(StaffWaterCleaning, staff_water_cleaning);
    singleton!(StaffManager, staff_manager);
    singleton!(AnimalAdoptionOffers, animal_adoption_offer_configuration);
}
