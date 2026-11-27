pub mod game_context_binder_directory_loading;
pub mod saved_world_economy_asset;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    prelude::*,
};
use openzt2_game_data::{world_definitions::document::WorldDefinitionDocument, AssetId};

use crate::asset_source::{
    archive_entry_asset_path::{
        archive_entry_asset_path, archive_entry_asset_source_path,
        definition_archive_entry_asset_path,
    },
    ArchiveOverlayChanged, AssetArchives,
};

use super::{
    ground_path_surface_images::GROUND_PATH_SURFACE_IMAGE_EXTENSION,
    world_definition_asset_set_state_and_borrowing_queries::{
        kind_range, DefinitionKind, DefinitionRow, SingletonKind, WorldDefinitions,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::{
        WorldDefinitionAsset, AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS,
    },
    world_definition_load_queue::{request_world_definition_loads, WorldDefinitionLoadCompletion},
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
            | "config/influence.xml"
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

pub(crate) fn is_fossil_puzzle_source(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}

pub(super) fn world_definition_sources_changed(
    archives: Res<AssetArchives>,
    mut messages: MessageReader<ArchiveOverlayChanged>,
) -> bool {
    let binders = archives.authored_binder_sources();
    let puzzles = archives.fossil_puzzle_root();
    let changed = messages
        .read()
        .flat_map(|message| &message.changes)
        .any(|change| {
            let path = &change.normalized_asset_path;
            world_definition_source_path(path, binders.directories())
                || puzzles
                    .as_deref()
                    .is_some_and(|root| is_fossil_puzzle_source(path, root))
                || path.starts_with("biomes/autoplacement")
                || path.starts_with("xpinfo")
                || matches!(
                    path.to_str(),
                    Some(
                        "config/gamectxt.xml"
                            | "config/puzzlemgr.xml"
                            | "config/ztshowtimermanager.xml"
                    )
                )
        });
    messages.clear();
    changed
}

pub(super) fn discover_world_definition_catalogue_sources(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut definitions: ResMut<WorldDefinitions>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
) {
    let (binder_sources, sources) = archives.world_definition_sources();
    let mut binder_inputs_changed = false;
    let mut zoopedia_changed = false;
    let mut adoption_changed = false;
    let mut biome_changed = false;
    let mut show_timer_changed = false;
    let mut puzzle_manager_changed = false;
    let puzzle_root = archives.fossil_puzzle_root();
    for change in overlay_changes.read().flat_map(|change| &change.changes) {
        let path = &change.normalized_asset_path;
        puzzle_manager_changed |= path == Path::new("config/puzzlemgr.xml");
        binder_inputs_changed |= path == Path::new("config/gamectxt.xml")
            || binder_sources.contains_path(path)
            || definitions
                .binder_sources
                .as_ref()
                .is_some_and(|old| old.contains_path(path));
        zoopedia_changed |= path.starts_with("ui/zoopedia/entries");
        if path.extension().is_some_and(|extension| extension == "xml") {
            adoption_changed |= path.parent() == Some(Path::new("xpinfo"));
            biome_changed |= path.starts_with("biomes/autoplacement");
            show_timer_changed |= path == Path::new("config/ztshowtimermanager.xml");
        }
    }
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
    let generation = archives.overlay_generation();
    definitions.requested_overlay_generation = generation;
    let base_sources = sources
        .iter()
        .map(|source| (&source.normalized_asset_path, source))
        .collect::<BTreeMap<_, _>>();
    let mut routed_sources = BTreeMap::new();
    let mut paths = paths
        .into_iter()
        .filter_map(|path| {
            let source = *base_sources.get(path)?;
            let base_path = archive_entry_asset_path(source);
            let supplements_changed = (zoopedia_changed && path.starts_with("ui/zoopedia/entries"))
                || (adoption_changed && path == Path::new("config/adoption.xml"))
                || (biome_changed && path.parent() == Some(Path::new("biomes")))
                || (show_timer_changed
                    && matches!(path.to_str(), Some("ai/ztai.xml" | "ai/staffmgr.xml")))
                || (puzzle_manager_changed
                    && puzzle_root
                        .as_deref()
                        .is_some_and(|root| is_fossil_puzzle_source(path, root)));
            let previous = definitions.routed_sources.get(&base_path);
            let binder_dependency_changed = binder_inputs_changed && {
                let path = previous.unwrap_or(&base_path);
                server
                    .get_handle::<WorldDefinitionAsset>(path.clone())
                    .and_then(|handle| assets.get(&handle))
                    .is_none_or(|asset| asset.authored_binder_sources.is_some())
            };
            let routed_path = if binder_dependency_changed || supplements_changed {
                definition_archive_entry_asset_path(source, generation)
            } else {
                previous.cloned().unwrap_or_else(|| base_path.clone())
            };
            routed_sources.insert(base_path, routed_path.clone());
            Some(routed_path)
        })
        .collect::<Vec<_>>();
    if let Some(root) = puzzle_root {
        paths.extend(
            archives
                .source_paths_under(&root)
                .into_iter()
                .filter(|path| {
                    is_fossil_puzzle_source(path, &root) && !base_sources.contains_key(path)
                }),
        );
    }
    definitions.routed_sources = routed_sources;
    definitions.binder_sources = Some(binder_sources);
    definitions.complete = false;
    definitions.next_load_paths = Some(paths);
    start_waiting_definition_operation(&server, &mut definitions);
}

fn start_waiting_definition_operation(server: &AssetServer, definitions: &mut WorldDefinitions) {
    if definitions.active_load.is_some() {
        return;
    }
    let Some(paths) = definitions.next_load_paths.take() else {
        return;
    };
    definitions.total_loads = paths.len();
    definitions.completed_loads = Default::default();
    definitions.active_load = Some(request_world_definition_loads(
        server,
        paths,
        std::sync::Arc::clone(&definitions.completed_loads),
    ));
}

pub(super) fn index_loaded_world_definition_assets_by_identifier_and_singleton_kind(
    archives: Res<AssetArchives>,
    server: Res<AssetServer>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    completions: Res<Assets<WorldDefinitionLoadCompletion>>,
    mut completed: MessageReader<AssetEvent<WorldDefinitionLoadCompletion>>,
    mut load_failures: MessageReader<AssetLoadFailedEvent<WorldDefinitionAsset>>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut definitions: ResMut<WorldDefinitions>,
    mut publication: MessageWriter<super::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionSourcesPublished>,
) {
    let operation = definitions.active_load.as_ref().map(Handle::id);
    let published = completed
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } if Some(id) == operation => completions.get(id),
            _ => None,
        })
        .last();
    let mut settled = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    settled.extend(load_failures.read().map(|failure| failure.id));
    let changed_sources = settled.clone();
    if let Some(completion) = published {
        definitions.active_load = None;
        if definitions.next_load_paths.is_some() {
            start_waiting_definition_operation(&server, &mut definitions);
            return;
        }
        definitions.handles.clone_from(&completion.handles);
        definitions.source_ids = completion.handles.iter().map(Handle::id).collect();
        definitions.pending.clear();
        definitions.failed.clear();
        settled.extend(definitions.source_ids.iter().copied());
    } else if definitions.active_load.is_some() {
        return;
    }
    settled.retain(|id| definitions.source_ids.contains(id));
    if published.is_none() && settled.is_empty() {
        return;
    }
    for id in settled {
        definitions.pending.remove(&id);
        definitions.failed.remove(&id);
        match server.get_load_state(id) {
            Some(LoadState::Failed(_)) => {
                definitions.failed.insert(id);
            }
            Some(LoadState::Loading) if !assets.contains(id) => {
                definitions.pending.insert(id);
            }
            _ if !assets.contains(id) => {
                definitions.failed.insert(id);
            }
            _ => {}
        }
    }
    if !definitions.pending.is_empty() {
        return;
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
        if asset.metadata.guest_influence_policy.is_some() {
            singletons.insert(SingletonKind::GuestInfluence, handle.clone());
        }
    }
    let mut animation_binder_rigs = BTreeMap::new();
    for handle in &definitions.handles {
        let Some(asset) = assets
            .get(handle)
            .filter(|_| !definitions.failed.contains(&handle.id()))
        else {
            continue;
        };
        for (index, rig) in asset.animation_binder_rigs().iter().enumerate() {
            if records
                .get(&(DefinitionKind::Object, rig.definition))
                .is_some_and(|row| row.handle.id() != handle.id())
            {
                continue;
            }
            animation_binder_rigs.insert(
                (rig.definition, rig.binder),
                DefinitionRow {
                    handle: handle.clone(),
                    first: index,
                    end: index + 1,
                },
            );
        }
    }
    let retained = records
        .values()
        .map(|row| row.handle.id())
        .chain(singletons.values().map(Handle::id))
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
            (DefinitionKind::MaterialDependency, &asset.materials),
            (
                DefinitionKind::TankWaterMaterialDependency,
                &asset.tank_water_material_sources,
            ),
            (DefinitionKind::ModelDependency, &asset.models),
            (DefinitionKind::EffectDependency, &asset.effects),
            (DefinitionKind::SceneDependency, &asset.scenes),
        ] {
            for dependency in dependencies {
                records.insert(
                    (kind, dependency.id),
                    DefinitionRow {
                        handle: handle.clone(),
                        first: 0,
                        end: 1,
                    },
                );
            }
        }
        for &id in asset.bound_scenes.keys() {
            records.insert(
                (DefinitionKind::SceneDependency, id),
                DefinitionRow {
                    handle: handle.clone(),
                    first: 0,
                    end: 1,
                },
            );
        }
    }
    let mut unlock_definition_identifiers_by_target = BTreeMap::<AssetId, Vec<AssetId>>::new();
    for ((_, identifier), row) in records.range(kind_range(DefinitionKind::Unlock)) {
        if let Some(unlock) = assets
            .get(&row.handle)
            .and_then(|asset| asset.document.unlocks.get(row.first))
        {
            unlock_definition_identifiers_by_target
                .entry(unlock.target)
                .or_default()
                .push(*identifier);
        }
    }
    let mut show_stages_by_object = BTreeMap::new();
    for (_, row) in records.range(kind_range(DefinitionKind::ShowStage)) {
        if let Some(stage) = assets
            .get(&row.handle)
            .and_then(|asset| asset.document.show_stages.get(row.first))
        {
            show_stages_by_object
                .entry(stage.object)
                .or_insert_with(|| row.clone());
        }
    }
    let mut vehicle_seats_by_index = BTreeMap::new();
    for (_, row) in records.range(kind_range(DefinitionKind::VehicleSeat)) {
        if let Some(seat) = assets
            .get(&row.handle)
            .and_then(|asset| asset.document.vehicle_seats.get(row.first))
        {
            vehicle_seats_by_index
                .entry((seat.vehicle, seat.index))
                .or_insert_with(|| row.clone());
        }
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
        .filter_map(|(index, ((_, id), row))| {
            assets.get(&row.handle).and_then(|asset| {
                asset.document.catalogue.get(row.first).map(|entry| {
                    (
                        index,
                        *id,
                        entry.authored_purchase_sort_key.as_str(),
                        entry.authored_purchase_sort_fallback_type_name.as_str(),
                        current_source_order.get(&row.handle.id()).map_or(
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
        .filter_map(|(_, row)| assets.get(&row.handle)?.document.paths.get(row.first))
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
    let mut changed_objects = BTreeSet::new();
    for kind in [DefinitionKind::Object, DefinitionKind::FacilityByObject] {
        let previous = definitions.records.range(kind_range(kind));
        let next = records.range(kind_range(kind));
        for (&(_, id), row) in previous {
            if records.get(&(kind, id)) != Some(row) || changed_sources.contains(&row.handle.id()) {
                changed_objects.insert(id);
            }
        }
        for (&(_, id), _) in next {
            if !definitions.records.contains_key(&(kind, id)) {
                changed_objects.insert(id);
            }
        }
    }
    for (&object, row) in &definitions.show_stages_by_object {
        if show_stages_by_object.get(&object) != Some(row)
            || changed_sources.contains(&row.handle.id())
        {
            changed_objects.insert(object);
        }
    }
    changed_objects.extend(
        show_stages_by_object
            .keys()
            .filter(|id| !definitions.show_stages_by_object.contains_key(id))
            .copied(),
    );
    definitions.records = records;
    definitions.show_stages_by_object = show_stages_by_object;
    definitions.animation_binder_rigs = animation_binder_rigs;
    definitions.vehicle_seats_by_index = vehicle_seats_by_index;
    definitions.catalogue_by_definition.take();
    definitions.research_by_definition.take();
    definitions.singletons = singletons;
    definitions.unlock_definition_identifiers_by_target = unlock_definition_identifiers_by_target;
    definitions.catalogue_authored_purchase_order = catalogue_authored_purchase_order;
    definitions.topology_cell_size_cm = topology_cell_size_cm;
    definitions.maximum_ground_path_width_cm = maximum_ground_path_width_cm;
    definitions.complete = true;
    refresh_ground_path_surface_image_dependencies(&mut definitions, &assets, &server);
    publication.write(super::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionSourcesPublished {
        overlay_generation: published.map(|_| definitions.requested_overlay_generation),
        changed_objects: changed_objects.into_iter().collect(),
        failed_sources: definitions.failed.iter().copied().collect(),
    });
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
    records: &mut BTreeMap<(DefinitionKind, AssetId), DefinitionRow>,
    singletons: &mut BTreeMap<SingletonKind, Handle<WorldDefinitionAsset>>,
) {
    macro_rules! index_rows {
        ($kind:ident, $field:ident, $record:ident => $key:expr) => {
            for (index, $record) in document.$field.iter().enumerate() {
                let row = records
                    .entry((DefinitionKind::$kind, $key))
                    .or_insert_with(|| DefinitionRow {
                        handle: handle.clone(),
                        first: index,
                        end: index + 1,
                    });
                if row.handle.id() != handle.id() {
                    *row = DefinitionRow {
                        handle: handle.clone(),
                        first: index,
                        end: index + 1,
                    };
                } else if row.end == index {
                    row.end += 1;
                }
            }
        };
    }
    macro_rules! index {
        ($kind:ident, $field:ident) => {
            index_rows!($kind, $field, record => record.id);
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
    index_rows!(FameThreshold, fame_thresholds, record => AssetId::from_key(&format!("fame-threshold:{}", record.level)));
    index!(Award, awards);
    index!(Tank, tanks);
    index_rows!(Aquatic, aquatic_requirements, record => record.species);
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
    index!(Weather, weather);
    index!(Ambient, ambient_spawns);
    index!(Environment, environments);
    index_rows!(MaintenanceByObject, maintenance_definitions, record => record.object);
    index_rows!(FacilityByObject, facilities, record => record.object);
    index_rows!(StaffByObject, staff, record => record.object);

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
    singleton!(TransportHighlight, transport_highlight);
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
