//! Loaded world-definition asset-set state and borrowing queries over canonical records.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use bevy::{gltf::Gltf, prelude::*};
use openzt2_game_data::{
    world_definitions::{
        animal_health::*,
        animal_shows_and_training::*,
        aquatic_habitats::*,
        behavior_selection_policy::BehaviorSelectionPolicy,
        biomes_locations_and_details::*,
        camera_definitions::*,
        catalogue_and_progression::{
            animal_adoption_offer_definition_types::*, catalogue_definition_types::*,
            research_and_unlock_definition_types::*, zoo_rating_fame_and_award_definition_types::*,
            zoopedia_entry_types::*,
        },
        environment::*,
        extinct_animal_recovery::*,
        facilities_and_maintenance::*,
        fences_and_gates::*,
        guest_simulation_definitions::*,
        immersive_mode_policy::*,
        object_placement::*,
        paths_and_tile_surfaces::*,
        simulation_time::*,
        staff_management::*,
        terrain_editing_brushes::*,
        transportation_and_tours::*,
        world_objects::*,
    },
    AssetId,
};

use crate::assets::{effect::ParticleEffectDocumentAsset, scene_prefab::ScenePrefabAsset};

use super::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum DefinitionKind {
    Object,
    Placeable,
    Facility,
    Maintenance,
    Staff,
    StaffJob,
    StaffRequest,
    Guest,
    ViewingOpportunity,
    Fence,
    Path,
    Biome,
    Location,
    Brush,
    Disease,
    Treatment,
    Tranquilizer,
    Rampage,
    Catalogue,
    Zoopedia,
    Research,
    Unlock,
    Rating,
    FameThreshold,
    Award,
    Tank,
    Aquatic,
    ShowStage,
    Trick,
    TrickOutcome,
    ShowRule,
    ShowAudioCue,
    ShowIcon,
    Station,
    Track,
    Vehicle,
    VehicleSeat,
    TourView,
    FossilSet,
    FossilPiece,
    FossilSlot,
    CloningCenter,
    ImmersiveModePolicy,
    Camera,
    PersonNames,
    Weather,
    Ambient,
    Environment,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum SingletonKind {
    AnimalAdoptionOffers,
    Cleanliness,
    GuestViewing,
    BiomeDetailPlacement,
    ShowPlatformUpgrades,
    FossilPlacement,
    SimulationTiming,
    GuestGeneration,
    BehaviorSelection,
    TranquilizerMode,
    AdmissionPriceBands,
    TankSurface,
    TankDepth,
    TankEdit,
    ShowPresentation,
    ShowScheduling,
    ShowScoring,
    ShowEditorPresentation,
    TourScoring,
    StaffWaterCleaning,
    StaffManager,
}

#[derive(Resource, Default)]
pub struct WorldDefinitions {
    pub(super) server: Option<AssetServer>,
    pub(super) revision: Option<u64>,
    pub(super) dirty: bool,
    pub(super) handles: Vec<Handle<WorldDefinitionAsset>>,
    pub(super) loading_task: Option<bevy::tasks::Task<Vec<Handle<WorldDefinitionAsset>>>>,
    pub(super) complete: bool,
    pub(super) completed_loads: Arc<AtomicUsize>,
    pub(super) total_loads: usize,
    pub(super) records: BTreeMap<(DefinitionKind, AssetId), Handle<WorldDefinitionAsset>>,
    pub(super) singletons: BTreeMap<SingletonKind, Handle<WorldDefinitionAsset>>,
    pub(super) unlock_definition_identifiers_by_target: BTreeMap<AssetId, Vec<AssetId>>,
    pub(super) catalogue_authored_purchase_order: Vec<(usize, AssetId)>,
    pub(super) catalogue_revision: u64,
    pub(super) topology_cell_size_cm: u16,
}

impl WorldDefinitions {
    pub(crate) fn loading_progress(&self) -> (usize, usize) {
        (
            self.completed_loads.load(Ordering::Relaxed),
            self.total_loads,
        )
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.complete
    }

    pub(crate) const fn catalogue_revision(&self) -> u64 {
        self.catalogue_revision
    }

    #[cfg(test)]
    pub(crate) fn index_test_document(
        &mut self,
        handle: Handle<WorldDefinitionAsset>,
        document: &openzt2_game_data::world_definitions::document::WorldDefinitionDocument,
    ) {
        document.objects.iter().for_each(|record| {
            self.records
                .insert((DefinitionKind::Object, record.id), handle.clone());
        });
        document.fences.iter().for_each(|record| {
            self.records
                .insert((DefinitionKind::Fence, record.id), handle.clone());
        });
        document.paths.iter().for_each(|record| {
            self.records
                .insert((DefinitionKind::Path, record.id), handle.clone());
        });
        // Test fixtures that exercise facility binding or guest service need
        // the same facility index the live refresh builds.
        document.facilities.iter().for_each(|record| {
            self.records
                .insert((DefinitionKind::Facility, record.id), handle.clone());
        });
        document.staff_requests.iter().for_each(|record| {
            self.records
                .insert((DefinitionKind::StaffRequest, record.id), handle.clone());
        });
        document.catalogue.iter().for_each(|record| {
            self.records
                .insert((DefinitionKind::Catalogue, record.id), handle.clone());
        });
        self.catalogue_authored_purchase_order.extend(
            document
                .catalogue
                .iter()
                .enumerate()
                .map(|(index, entry)| (index, entry.id)),
        );
        self.singletons
            .insert(SingletonKind::SimulationTiming, handle.clone());
        self.topology_cell_size_cm = document
            .paths
            .iter()
            .map(|path| path.width_cm / 2)
            .min()
            .unwrap_or_default();
        self.handles.push(handle);
    }

    pub fn get<'a>(
        &'a self,
        assets: &'a Assets<WorldDefinitionAsset>,
    ) -> Option<WorldDefinitionsView<'a>> {
        let ready = !self.dirty
            && !self.handles.is_empty()
            && self
                .singletons
                .contains_key(&SingletonKind::SimulationTiming);
        ready.then_some(WorldDefinitionsView {
            server: self.server.as_ref(),
            handles: &self.handles,
            records: &self.records,
            singletons: &self.singletons,
            unlock_definition_identifiers_by_target: &self.unlock_definition_identifiers_by_target,
            catalogue_authored_purchase_order: &self.catalogue_authored_purchase_order,
            topology_cell_size_cm: self.topology_cell_size_cm,
            assets,
        })
    }
}

#[derive(Clone, Copy)]
pub struct WorldDefinitionsView<'a> {
    server: Option<&'a AssetServer>,
    handles: &'a [Handle<WorldDefinitionAsset>],
    records: &'a BTreeMap<(DefinitionKind, AssetId), Handle<WorldDefinitionAsset>>,
    singletons: &'a BTreeMap<SingletonKind, Handle<WorldDefinitionAsset>>,
    unlock_definition_identifiers_by_target: &'a BTreeMap<AssetId, Vec<AssetId>>,
    catalogue_authored_purchase_order: &'a [(usize, AssetId)],
    topology_cell_size_cm: u16,
    assets: &'a Assets<WorldDefinitionAsset>,
}

macro_rules! finder {
    ($name:ident, $kind:ident, $field:ident, $record:ty) => {
        pub fn $name(self, id: AssetId) -> Option<&'a $record> {
            self.assets
                .get(self.records.get(&(DefinitionKind::$kind, id))?)?
                .document
                .$field
                .iter()
                .find(|record| record.id == id)
        }
    };
}

macro_rules! singleton {
    ($name:ident, $kind:ident, $field:ident, $record:ty) => {
        pub fn $name(self) -> Option<&'a $record> {
            self.assets
                .get(self.singletons.get(&SingletonKind::$kind)?)?
                .document
                .$field
                .as_ref()
        }
    };
}

impl<'a> WorldDefinitionsView<'a> {
    pub(crate) fn staff_request_rows(
        self,
        definition: AssetId,
    ) -> Option<&'a [StaffRequestControllerDefinition]> {
        let Some(handle) = self
            .records
            .get(&(DefinitionKind::StaffRequest, definition))
        else {
            return (!self.has_pending_assets()).then_some(&[]);
        };
        let rows = &self.assets.get(handle)?.document.staff_requests;
        // Lowering keeps each definition's controllers contiguous, preserving
        // authored order within that definition. Borrow the canonical rows.
        let start = rows.partition_point(|row| row.id < definition);
        let end = rows.partition_point(|row| row.id <= definition);
        Some(&rows[start..end])
    }

    pub(crate) fn has_object_record(self, id: AssetId) -> bool {
        self.records.contains_key(&(DefinitionKind::Object, id))
    }

    pub(crate) fn has_pending_assets(self) -> bool {
        self.handles
            .iter()
            .any(|handle| self.assets.get(handle).is_none())
    }

    fn documents(self) -> impl Iterator<Item = &'a WorldDefinitionAsset> {
        self.handles
            .iter()
            .rev()
            .filter_map(|handle| self.assets.get(handle))
    }

    finder!(find_object, Object, objects, WorldObjectDefinition);
    finder!(find_placeable, Placeable, placeables, PlaceableDefinition);
    finder!(find_facility, Facility, facilities, FacilityDefinition);
    finder!(
        find_maintenance,
        Maintenance,
        maintenance_definitions,
        MaintenanceDefinition
    );
    finder!(find_staff, Staff, staff, StaffRoleDefinition);
    /// Loads an employee's actor animations: its drawn look's set, or the
    /// role's own when the employee has no drawn look.
    pub(crate) fn load_staff_model_animation_set<T: Asset>(
        self,
        staff_role: AssetId,
        drawn_model_animation_set: Option<AssetId>,
    ) -> Option<Handle<T>> {
        let asset = self
            .assets
            .get(self.records.get(&(DefinitionKind::Staff, staff_role))?)?;
        let model_animation_set = asset
            .document
            .staff
            .iter()
            .find(|record| record.id == staff_role)?
            .model_animation_set;
        asset.animation_set(
            self.server?,
            drawn_model_animation_set.unwrap_or(model_animation_set),
        )
    }
    finder!(find_staff_job, StaffJob, staff_jobs, StaffJobDefinition);
    finder!(find_guest, Guest, guests, GuestDefinition);
    pub(crate) fn guest_definitions(self) -> impl Iterator<Item = &'a GuestDefinition> {
        self.records.iter().filter_map(move |((kind, id), handle)| {
            if *kind != DefinitionKind::Guest {
                return None;
            }
            self.assets
                .get(handle)?
                .document
                .guests
                .iter()
                .find(|definition| definition.id == *id)
        })
    }
    finder!(
        find_viewing_opportunity,
        ViewingOpportunity,
        viewing_opportunities,
        ViewingOpportunityDefinition
    );
    finder!(find_fence, Fence, fences, FenceDefinition);
    finder!(find_path, Path, paths, GuestPathDefinition);
    finder!(find_biome, Biome, biomes, BiomeDefinition);
    finder!(find_location, Location, locations, WorldLocationDefinition);
    finder!(find_brush, Brush, brushes, TerrainEditingBrushDefinition);
    finder!(find_disease, Disease, diseases, DiseaseDefinition);
    finder!(find_treatment, Treatment, treatments, TreatmentDefinition);
    finder!(
        find_tranquilizer,
        Tranquilizer,
        tranquilizers,
        TranquilizerDefinition
    );
    finder!(find_rampage_rule, Rampage, rampage_rules, RampageRule);
    finder!(find_catalogue_entry, Catalogue, catalogue, CatalogueEntry);
    finder!(find_zoopedia_entry, Zoopedia, zoopedia, ZoopediaEntry);
    finder!(find_research, Research, research, ResearchDefinition);
    finder!(find_unlock, Unlock, unlocks, UnlockDefinition);
    finder!(find_rating, Rating, rating_definitions, RatingDefinition);
    pub fn find_fame_threshold(self, level: u16) -> Option<&'a FameThreshold> {
        self.assets
            .get(self.records.get(&(
                DefinitionKind::FameThreshold,
                AssetId::from_key(&format!("fame-threshold:{level}")),
            ))?)?
            .document
            .fame_thresholds
            .iter()
            .find(|record| record.level == level)
    }
    finder!(find_award, Award, awards, AwardDefinition);
    finder!(find_tank, Tank, tanks, TankDefinition);
    finder!(find_show_stage, ShowStage, show_stages, ShowStageDefinition);
    pub fn find_show_stage_definition_for_world_object(
        self,
        world_object_definition: AssetId,
    ) -> Option<&'a ShowStageDefinition> {
        self.records
            .keys()
            .filter(|(kind, _)| *kind == DefinitionKind::ShowStage)
            .filter_map(|(_, show_stage_definition)| self.find_show_stage(*show_stage_definition))
            .find(|show_stage| show_stage.object == world_object_definition)
    }
    finder!(
        find_show_audio_cue,
        ShowAudioCue,
        show_audio_cues,
        ShowAudioCue
    );
    finder!(
        find_show_presentation_icon,
        ShowIcon,
        show_presentation_icons,
        ShowPresentationIcon
    );
    finder!(find_trick, Trick, tricks, TrickDefinition);
    finder!(
        find_trick_outcome_tokens,
        TrickOutcome,
        trick_outcome_tokens,
        TrickOutcomeTokens
    );
    finder!(find_show_rule, ShowRule, show_rules, ShowRuleDefinition);
    finder!(
        find_station,
        Station,
        stations,
        TransportationStationDefinition
    );
    finder!(find_track, Track, tracks, TransportationTrackDefinition);
    finder!(
        find_vehicle,
        Vehicle,
        vehicles,
        TransportationVehicleDefinition
    );
    finder!(
        find_vehicle_seat,
        VehicleSeat,
        vehicle_seats,
        TransportationVehicleSeatDefinition
    );
    finder!(find_tour_view, TourView, tour_views, TourViewDefinition);
    finder!(find_fossil_set, FossilSet, fossil_sets, FossilSetDefinition);
    finder!(
        find_fossil_piece,
        FossilPiece,
        fossil_pieces,
        FossilPieceDefinition
    );
    finder!(
        find_fossil_slot,
        FossilSlot,
        fossil_slots,
        FossilSlotDefinition
    );
    finder!(
        find_cloning_center,
        CloningCenter,
        cloning_centers,
        CloningCenterDefinition
    );
    finder!(
        find_immersive_mode_policy,
        ImmersiveModePolicy,
        immersive_mode_policies,
        ImmersiveModePolicy
    );
    finder!(find_camera, Camera, cameras, CameraTuningDefinition);
    finder!(
        find_person_name_pool,
        PersonNames,
        person_name_pools,
        PersonNamePool
    );
    finder!(find_weather, Weather, weather, WeatherDefinition);
    finder!(
        find_ambient_spawn,
        Ambient,
        ambient_spawns,
        AmbientSpawnDefinition
    );
    finder!(
        find_environment,
        Environment,
        environments,
        EnvironmentDefinition
    );

    pub fn objects(self) -> impl Iterator<Item = &'a WorldObjectDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Object)
            .filter_map(move |((_, id), _)| self.find_object(*id))
    }

    pub fn placeables(self) -> impl Iterator<Item = &'a PlaceableDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Placeable)
            .filter_map(move |((_, id), _)| self.find_placeable(*id))
    }

    pub fn paths(self) -> impl Iterator<Item = &'a GuestPathDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Path)
            .filter_map(move |((_, id), _)| self.find_path(*id))
    }

    pub fn fences(self) -> impl Iterator<Item = &'a FenceDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Fence)
            .filter_map(move |((_, id), _)| self.find_fence(*id))
    }

    pub fn scene(self, id: AssetId) -> Option<Handle<ScenePrefabAsset>> {
        let server = self.server?;
        self.documents().find_map(|asset| asset.scene(server, id))
    }

    pub fn catalogue(self) -> impl Iterator<Item = &'a CatalogueEntry> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Catalogue)
            .filter_map(move |((_, id), _)| self.find_catalogue_entry(*id))
    }

    pub(crate) fn catalogue_in_authored_purchase_order(
        self,
    ) -> impl Iterator<Item = (usize, &'a CatalogueEntry)> {
        self.catalogue_authored_purchase_order
            .iter()
            .filter_map(move |(index, id)| {
                self.find_catalogue_entry(*id).map(|entry| (*index, entry))
            })
    }

    pub fn zoopedia(self) -> impl Iterator<Item = &'a ZoopediaEntry> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Zoopedia)
            .filter_map(move |((_, id), _)| self.find_zoopedia_entry(*id))
    }

    pub fn unlocks(self) -> impl Iterator<Item = &'a UnlockDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Unlock)
            .filter_map(move |((_, id), _)| self.find_unlock(*id))
    }

    pub(crate) fn unlock_definitions_targeting_catalogue_definition(
        self,
        target: AssetId,
    ) -> impl Iterator<Item = &'a UnlockDefinition> {
        self.unlock_definition_identifiers_by_target
            .get(&target)
            .into_iter()
            .flatten()
            .filter_map(move |identifier| self.find_unlock(*identifier))
    }

    pub fn tranquilizers(self) -> impl Iterator<Item = &'a TranquilizerDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Tranquilizer)
            .filter_map(move |((_, id), _)| self.find_tranquilizer(*id))
    }

    pub fn diseases(self) -> impl Iterator<Item = &'a DiseaseDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Disease)
            .filter_map(move |((_, id), _)| self.find_disease(*id))
    }

    pub fn rampage_rules(self) -> impl Iterator<Item = &'a RampageRule> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Rampage)
            .filter_map(move |((_, id), _)| self.find_rampage_rule(*id))
    }

    pub fn staff_jobs(self) -> impl Iterator<Item = &'a StaffJobDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::StaffJob)
            .filter_map(move |((_, id), _)| self.find_staff_job(*id))
    }

    pub fn biomes(self) -> impl Iterator<Item = &'a BiomeDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Biome)
            .filter_map(move |((_, id), _)| self.find_biome(*id))
    }

    pub fn brushes(self) -> impl Iterator<Item = &'a TerrainEditingBrushDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Brush)
            .filter_map(move |((_, id), _)| self.find_brush(*id))
    }

    pub fn tricks(self) -> impl Iterator<Item = &'a TrickDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Trick)
            .filter_map(move |((_, id), _)| self.find_trick(*id))
    }

    pub fn stations(self) -> impl Iterator<Item = &'a TransportationStationDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Station)
            .filter_map(move |((_, id), _)| self.find_station(*id))
    }

    pub fn tracks(self) -> impl Iterator<Item = &'a TransportationTrackDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Track)
            .filter_map(move |((_, id), _)| self.find_track(*id))
    }

    pub fn vehicles(self) -> impl Iterator<Item = &'a TransportationVehicleDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Vehicle)
            .filter_map(move |((_, id), _)| self.find_vehicle(*id))
    }

    pub fn tour_views(self) -> impl Iterator<Item = &'a TourViewDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::TourView)
            .filter_map(move |((_, id), _)| self.find_tour_view(*id))
    }

    pub fn fossil_slots(self) -> impl Iterator<Item = &'a FossilSlotDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::FossilSlot)
            .filter_map(move |((_, id), _)| self.find_fossil_slot(*id))
    }

    pub fn cloning_centers(self) -> impl Iterator<Item = &'a CloningCenterDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::CloningCenter)
            .filter_map(move |((_, id), _)| self.find_cloning_center(*id))
    }

    pub fn research(self) -> impl Iterator<Item = &'a ResearchDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Research)
            .filter_map(move |((_, id), _)| self.find_research(*id))
    }

    pub fn rating_definitions(self) -> impl Iterator<Item = &'a RatingDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Rating)
            .filter_map(move |((_, id), _)| self.find_rating(*id))
    }

    pub fn fame_thresholds(self) -> impl Iterator<Item = &'a FameThreshold> {
        self.documents()
            .flat_map(|asset| asset.document.fame_thresholds.iter())
    }

    pub fn awards(self) -> impl Iterator<Item = &'a AwardDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::Award)
            .filter_map(move |((_, id), _)| self.find_award(*id))
    }

    pub fn show_audio_cues(self) -> impl Iterator<Item = &'a ShowAudioCue> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::ShowAudioCue)
            .filter_map(move |((_, id), _)| self.find_show_audio_cue(*id))
    }

    pub fn fossil_pieces(self) -> impl Iterator<Item = &'a FossilPieceDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::FossilPiece)
            .filter_map(move |((_, id), _)| self.find_fossil_piece(*id))
    }

    pub fn immersive_mode_policies(self) -> impl Iterator<Item = &'a ImmersiveModePolicy> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::ImmersiveModePolicy)
            .filter_map(move |((_, id), _)| self.find_immersive_mode_policy(*id))
    }

    pub fn find_aquatic_requirement(self, species: AssetId) -> Option<&'a AquaticRequirement> {
        self.assets
            .get(self.records.get(&(DefinitionKind::Aquatic, species))?)?
            .document
            .aquatic_requirements
            .iter()
            .find(|record| record.species == species)
    }

    pub fn find_maintenance_by_object(self, object: AssetId) -> Option<&'a MaintenanceDefinition> {
        self.documents()
            .flat_map(|asset| &asset.document.maintenance_definitions)
            .find(|record| record.object == object)
    }

    pub fn find_facility_by_object(self, object: AssetId) -> Option<&'a FacilityDefinition> {
        self.documents()
            .flat_map(|asset| &asset.document.facilities)
            .find(|record| record.object == object)
    }

    pub fn find_staff_by_object(self, object: AssetId) -> Option<&'a StaffRoleDefinition> {
        self.documents()
            .flat_map(|asset| &asset.document.staff)
            .find(|record| record.object == object)
    }

    pub fn find_vehicle_seat_by_index(
        self,
        vehicle: AssetId,
        index: u16,
    ) -> Option<&'a TransportationVehicleSeatDefinition> {
        self.records
            .iter()
            .filter(|((kind, _), _)| *kind == DefinitionKind::VehicleSeat)
            .filter_map(move |((_, id), _)| self.find_vehicle_seat(*id))
            .find(|seat| seat.vehicle == vehicle && seat.index == index)
    }

    singleton!(
        animal_adoption_offer_configuration,
        AnimalAdoptionOffers,
        animal_adoption_offer_configuration,
        AnimalAdoptionOfferConfiguration
    );
    singleton!(
        cleanliness_policy,
        Cleanliness,
        cleanliness_policy,
        CleanlinessPolicy
    );
    singleton!(
        guest_viewing,
        GuestViewing,
        guest_viewing,
        GuestViewingPolicy
    );
    singleton!(
        biome_detail_placement,
        BiomeDetailPlacement,
        biome_detail_placement,
        BiomeDetailPlacementPolicy
    );
    singleton!(
        show_platform_upgrades,
        ShowPlatformUpgrades,
        show_platform_upgrades,
        ShowPlatformUpgradePolicy
    );
    singleton!(
        fossil_placement,
        FossilPlacement,
        fossil_placement,
        FossilPlacementPolicy
    );
    singleton!(
        simulation_timing,
        SimulationTiming,
        simulation_timing,
        SimulationTimingDefinition
    );
    singleton!(
        guest_generation,
        GuestGeneration,
        guest_generation,
        GuestGenerationPolicy
    );
    singleton!(
        behavior_selection_policy,
        BehaviorSelection,
        behavior_selection_policy,
        BehaviorSelectionPolicy
    );
    singleton!(
        tranquilizer_mode,
        TranquilizerMode,
        tranquilizer_mode,
        TranquilizerModePolicy
    );
    singleton!(
        admission_price_bands_cents,
        AdmissionPriceBands,
        admission_price_bands_cents,
        [i32; 4]
    );
    singleton!(
        tank_surface_policy,
        TankSurface,
        tank_surface_policy,
        TankSurfacePolicy
    );
    singleton!(
        tank_depth_policy,
        TankDepth,
        tank_depth_policy,
        TankDepthPolicy
    );
    singleton!(tank_edit_policy, TankEdit, tank_edit_policy, TankEditPolicy);
    singleton!(
        show_presentation,
        ShowPresentation,
        show_presentation,
        ShowPresentationPolicy
    );
    singleton!(
        show_scheduling,
        ShowScheduling,
        show_scheduling,
        ShowSchedulingPolicy
    );
    singleton!(show_scoring, ShowScoring, show_scoring, ShowScoringPolicy);
    singleton!(
        show_editor_presentation,
        ShowEditorPresentation,
        show_editor_presentation,
        ShowEditorPresentationPolicy
    );
    singleton!(tour_scoring, TourScoring, tour_scoring, TourScoringPolicy);
    singleton!(
        staff_manager,
        StaffManager,
        staff_manager,
        StaffManagerPolicy
    );
    singleton!(
        staff_water_cleaning,
        StaffWaterCleaning,
        staff_water_cleaning,
        StaffAquaticTankWaterCleaningPolicy
    );

    pub fn timing(self) -> &'a SimulationTimingDefinition {
        self.simulation_timing()
            .expect("world-definition readiness requires simulation timing")
    }

    pub fn topology_cell_size_cm(self) -> u16 {
        self.topology_cell_size_cm
    }

    pub fn texture_image(self, id: AssetId) -> Option<Handle<Image>> {
        let server = self.server?;
        self.documents().find_map(|asset| asset.texture(server, id))
    }

    pub fn model(self, id: AssetId) -> Option<Handle<Gltf>> {
        let server = self.server?;
        self.documents().find_map(|asset| asset.model(server, id))
    }

    pub(crate) fn effect(self, id: AssetId) -> Option<Handle<ParticleEffectDocumentAsset>> {
        let server = self.server?;
        self.documents().find_map(|asset| asset.effect(server, id))
    }
}
