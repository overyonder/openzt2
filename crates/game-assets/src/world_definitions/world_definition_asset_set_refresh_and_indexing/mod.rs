pub mod game_context_binder_directory_loading;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};
use openzt2_game_data::{world_definitions::document::WorldDefinitionDocument, AssetId};

use crate::asset_source::{
    archive_entry_asset_path::{archive_entry_asset_path, archive_entry_asset_source_path},
    ArchiveCatalogue, AssetArchives,
};

use super::{
    ground_path_surface_images::GROUND_PATH_SURFACE_IMAGE_EXTENSION,
    world_definition_asset_set_state_and_borrowing_queries::{
        kind_range, DefinitionKind, SingletonKind, WorldDefinitions,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::{
        WorldDefinitionAsset, AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS,
    },
    world_definition_load_queue::{
        request_world_definition_handles_without_per_asset_frame_waits,
        MAXIMUM_ACTIVE_DEFINITION_LOADS,
    },
};

pub(crate) fn world_definition_source_path(
    path: &Path,
    binder_directories: &BTreeSet<PathBuf>,
) -> bool {
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
            | "config/favorite.xml"
            | "config/adoption.xml"
            | "config/diseasemgr.xml"
            | "config/timekeeper.xml"
            | "config/puzzlemgr.xml"
            | "config/showmixer.xml"
            | "config/status.xml"
            | "config/status2.xml"
            | "config/status3.xml"
            | "config/tankmgr.xml"
            | "config/cleantankaction.xml"
            | "config/freeform_mode.xml"
            | "config/challenge_mode.xml"
            | "config/campaign_mode.xml"
    )
}

#[derive(Default)]
pub(super) struct WorldDefinitionDiscovery {
    sources: Option<Arc<[z2f::ResolvedArchiveEntry]>>,
    supplements: Option<Arc<[z2f::ResolvedArchiveEntry]>>,
}

pub(super) fn discover_world_definition_catalogue_sources(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut definitions: ResMut<WorldDefinitions>,
    mut previous: Local<WorldDefinitionDiscovery>,
) {
    if !archives.is_changed() && previous.sources.is_some() {
        return;
    }
    let (binder_sources, sources) = archives.world_definition_sources();
    let supplements = archives.catalogue_sources(ArchiveCatalogue::WorldDefinitionSupplements);
    let changed_sources = previous
        .sources
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &sources));
    let changed_supplements = previous
        .supplements
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &supplements));
    if !changed_sources && !changed_supplements {
        return;
    }
    let paths_changed_under = |old: &[z2f::ResolvedArchiveEntry],
                               new: &[z2f::ResolvedArchiveEntry],
                               directory: &str| {
        let within =
            |entry: &&z2f::ResolvedArchiveEntry| entry.normalized_asset_path.starts_with(directory);
        old.iter().filter(within).ne(new.iter().filter(within))
    };
    let zoopedia_changed = previous.sources.as_ref().is_some_and(|old| {
        changed_sources && paths_changed_under(old, &sources, "ui/zoopedia/entries")
    });
    let changed_supplement = |path| {
        changed_supplements
            && previous
                .supplements
                .as_ref()
                .is_some_and(|old| paths_changed_under(old, &supplements, path))
    };
    let adoption_changed = changed_supplement("xpinfo");
    let biome_changed = changed_supplement("biomes/autoplacement");
    let show_timer_changed = changed_supplement("config/ztshowtimermanager.xml");
    if definitions.server.is_none() {
        definitions.server = Some(server.clone());
    }
    let mut paths = sources
        .iter()
        .map(|entry| &entry.normalized_asset_path)
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
            || additive_zoopedia_entry_collection_owner_path == Some(*path)
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
    let routed_paths = sources
        .iter()
        .map(|source| {
            (
                &source.normalized_asset_path,
                archive_entry_asset_path(source),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let paths = paths
        .into_iter()
        .filter_map(|path| routed_paths.get(path).cloned())
        .collect::<Vec<_>>();
    let wanted = paths.iter().map(PathBuf::as_path).collect::<BTreeSet<_>>();
    let binder_inputs_changed = definitions
        .binder_sources
        .as_ref()
        .is_some_and(|previous| !std::sync::Arc::ptr_eq(previous, &binder_sources));
    let mut reload_queue = std::mem::take(&mut definitions.reload_queue);
    reload_queue.retain(|_, path| wanted.contains(path.path()));
    for handle in &definitions.handles {
        let id = handle.id();
        let Some(path) = handle.path().filter(|path| wanted.contains(path.path())) else {
            continue;
        };
        let Some((source, _, _)) = archive_entry_asset_source_path(path.path()) else {
            continue;
        };
        let binder_changed = binder_inputs_changed
            && (assets
                .get(id)
                .is_some_and(|asset| asset.authored_binder_sources.is_some())
                || definitions.failed.contains(&id));
        let supplements_changed = (zoopedia_changed && source.starts_with("ui/zoopedia/entries"))
            || (adoption_changed && source == Path::new("config/adoption.xml"))
            || (biome_changed && source.parent() == Some(Path::new("biomes")))
            || (show_timer_changed
                && matches!(source.to_str(), Some("ai/ztai.xml" | "ai/staffmgr.xml")));
        if binder_changed || supplements_changed {
            reload_queue.insert(id, path.clone());
        }
    }
    definitions.reload_queue = reload_queue;
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
    definitions.binder_sources = Some(binder_sources);
    previous.sources = Some(sources);
    previous.supplements = Some(supplements);
}

pub(super) fn index_loaded_world_definition_assets_by_identifier_and_singleton_kind(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut load_failures: MessageReader<AssetLoadFailedEvent<WorldDefinitionAsset>>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut definitions: ResMut<WorldDefinitions>,
) {
    let mut discovery_finished = false;
    if let Some(task) = definitions.bypass_change_detection().loading_task.as_mut() {
        if let Some(handles) = bevy::tasks::block_on(futures_lite::future::poll_once(task)) {
            let definitions = definitions.bypass_change_detection();
            definitions.handles = handles;
            definitions.loading_task = None;
            let sources = definitions
                .handles
                .iter()
                .map(Handle::id)
                .collect::<BTreeSet<_>>();
            definitions.failed.retain(|id| sources.contains(id));
            definitions.pending.retain(|id| sources.contains(id));
            definitions.reloading.retain(|id| sources.contains(id));
            for &id in &sources {
                if definitions.reloading.contains(&id) {
                    continue;
                }
                match server.get_load_state(id) {
                    Some(LoadState::Failed(_)) if !definitions.pending.contains(&id) => {
                        definitions.failed.insert(id);
                    }
                    Some(LoadState::Loaded) if assets.contains(id) => {}
                    _ => {
                        definitions.pending.insert(id);
                    }
                }
            }
            definitions.source_ids = sources;
            discovery_finished = true;
        }
    }
    let mut settled = BTreeSet::new();
    for event in events.read() {
        let id = match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                id
            }
            AssetEvent::Unused { .. } | AssetEvent::LoadedWithDependencies { .. } => continue,
        };
        if definitions.source_ids.contains(&id) {
            let current = assets
                .get(id)
                .and_then(|asset| asset.authored_binder_sources.as_ref())
                .zip(definitions.binder_sources.as_ref())
                .is_none_or(|(loaded, expected)| std::sync::Arc::ptr_eq(loaded, expected));
            if current {
                settled.insert(id);
            }
        }
    }
    let mut failed = BTreeSet::new();
    for failure in load_failures.read() {
        if definitions.source_ids.contains(&failure.id) {
            failed.insert(failure.id);
        }
    }
    let changed = discovery_finished || !settled.is_empty() || !failed.is_empty();
    if !changed {
        return;
    }
    {
        let loading = definitions.bypass_change_detection();
        for id in settled {
            loading.pending.remove(&id);
            loading.reloading.remove(&id);
            loading.failed.remove(&id);
        }
        for id in failed {
            loading.pending.remove(&id);
            loading.reloading.remove(&id);
            loading.failed.insert(id);
        }
    }
    if definitions.loading_task.is_some() || !definitions.pending.is_empty() {
        return;
    }

    {
        let loading = definitions.bypass_change_detection();
        while loading.reloading.len() < MAXIMUM_ACTIVE_DEFINITION_LOADS {
            let Some((id, path)) = loading.reload_queue.pop_first() else {
                break;
            };
            if loading.reloading.contains(&id) {
                loading.reload_queue.insert(id, path);
                break;
            }
            if loading.source_ids.contains(&id) {
                loading.reloading.insert(id);
                server.reload(path);
            }
        }
        if !loading.reloading.is_empty() {
            return;
        }
    }

    let mut records = BTreeMap::new();
    let mut singletons = BTreeMap::new();
    for handle in &definitions.handles {
        let Some(asset) = assets.get(handle) else {
            continue;
        };
        if definitions.failed.contains(&handle.id()) {
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
    let current_source_order = definitions
        .handles
        .iter()
        .filter_map(|handle| {
            let (path, _, _) = archive_entry_asset_source_path(handle.path()?.path())?;
            Some((
                handle.id(),
                archives.first_enabled_archive_entry_order(&path)?,
            ))
        })
        .collect::<BTreeMap<_, _>>();
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
                            current_source_order.get(&handle.id()).map_or(
                                entry.authored_type_registry_source_order,
                                |[archive, ordinal]| {
                                    [
                                        *archive,
                                        *ordinal,
                                        entry.authored_type_registry_source_order[2],
                                    ]
                                },
                            ),
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
    let maximum_ground_path_width_cm = records
        .range(kind_range(DefinitionKind::Path))
        .filter_map(|((_, id), handle)| {
            assets
                .get(handle)?
                .document
                .paths
                .iter()
                .find(|path| path.id == *id)
        })
        .filter(|path| !path.elevated)
        .map(|path| path.width_cm)
        .max()
        .unwrap_or_default();
    definitions.fame_documents = definitions
        .handles
        .iter()
        .filter(|handle| {
            assets.get(*handle).is_some_and(|asset| {
                !asset.document.fame_thresholds.is_empty()
                    || !asset.document.starting_fame.is_empty()
            })
        })
        .cloned()
        .collect();
    definitions.records = records;
    definitions.catalogue_by_definition.take();
    definitions.research_by_definition.take();
    definitions.singletons = singletons;
    definitions.unlock_definition_identifiers_by_target = unlock_definition_identifiers_by_target;
    definitions.catalogue_authored_purchase_order = catalogue_authored_purchase_order;
    definitions.topology_cell_size_cm = topology_cell_size_cm;
    definitions.maximum_ground_path_width_cm = maximum_ground_path_width_cm;
    definitions.complete = true;
    refresh_ground_path_surface_image_dependencies(&mut definitions, &assets, &server);
}

fn refresh_ground_path_surface_image_dependencies(
    definitions: &mut WorldDefinitions,
    assets: &Assets<WorldDefinitionAsset>,
    server: &AssetServer,
) {
    definitions.ground_path_surface_images =
        definitions.get(assets).map_or_else(BTreeMap::new, |view| {
            view.paths()
                .filter(|definition| !definition.elevated)
                .map(|definition| definition.surface_texture)
                .chain(
                    AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS
                        .into_iter()
                        .map(AssetId::from_virtual_path),
                )
                .filter_map(|identifier| {
                    let path = view.texture_path(identifier)?;
                    let image =
                        server.load(format!("{path}.{GROUND_PATH_SURFACE_IMAGE_EXTENSION}"));
                    Some((identifier, image))
                })
                .collect()
        });
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
    index!(StaffTankCleaningRequest, staff_tank_cleaning_requests);
    index!(StaffRequest, staff_requests);
    index!(Guest, guests);
    index!(EntityNaming, entity_naming);
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
    index!(
        AnimalAdoptionSpeciesEligibility,
        animal_adoption_species_eligibility
    );
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
    singleton!(GuestFavouriteAnimalTrend, guest_favourite_animal_trend);
    singleton!(BehaviorSelection, behavior_selection_policy);
    singleton!(NeedAdjustment, need_adjustment);
    singleton!(TranquilizerMode, tranquilizer_mode);
    singleton!(AdmissionPriceBands, admission_price_bands_cents);
    singleton!(TankSurface, tank_surface_policy);
    singleton!(TankDepth, tank_depth_policy);
    singleton!(TankEdit, tank_edit_policy);
    singleton!(DiseaseExposure, disease_exposure);
    singleton!(FencePlacementMode, fence_placement_mode_policy);
    singleton!(ShowPresentation, show_presentation);
    singleton!(ShowScheduling, show_scheduling);
    singleton!(ShowScoring, show_scoring);
    singleton!(ShowEditorPresentation, show_editor_presentation);
    singleton!(TourScoring, tour_scoring);
    singleton!(StaffWaterCleaning, staff_water_cleaning);
    singleton!(StaffManager, staff_manager);
    singleton!(AnimalAdoptionOffers, animal_adoption_offer_configuration);
}
