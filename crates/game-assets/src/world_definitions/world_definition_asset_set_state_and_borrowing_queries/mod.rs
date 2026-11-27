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
            animal_adoption_offer_definition_types::*,
            animal_adoption_species_eligibility::AnimalAdoptionSpeciesEligibility,
            catalogue_definition_types::*, research_and_unlock_definition_types::*,
            zoo_rating_fame_and_award_definition_types::*, zoopedia_entry_types::*,
        },
        environment::*,
        extinct_animal_recovery::*,
        facilities_and_maintenance::*,
        fences_and_gates::*,
        guest_favourite_animal_trend_policy::GuestFavouriteAnimalTrendPolicy,
        guest_simulation_definitions::*,
        immersive_mode_policy::*,
        need_adjustment::NeedAdjustmentPolicy,
        object_placement::{
            placement_stomp_rules::{PlacementStompPolicy, PlacementTargetFacts},
            *,
        },
        paths_and_tile_surfaces::*,
        simulation_time::*,
        staff_management::*,
        terrain_editing_brushes::*,
        transportation_and_tours::*,
        world_objects::*,
    },
    AssetId,
};

use crate::{effect::ParticleEffectDocumentAsset, scene_prefab::ScenePrefabAsset};

use super::ground_path_surface_images::GroundPathSurfaceImage;
use super::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum DefinitionKind {
    EntityNaming,
    Object,
    Placeable,
    Facility,
    Maintenance,
    Staff,
    StaffJob,
    StaffTankCleaningRequest,
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
    AnimalAdoptionSpeciesEligibility,
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
    Weather,
    Ambient,
    Environment,
    // Precedence winners keyed by a record's object rather than its own id.
    MaintenanceByObject,
    FacilityByObject,
    StaffByObject,
    // Precedence winners for demand-loaded dependency identifiers.
    TextureDependency,
    MaterialDependency,
    TankWaterMaterialDependency,
    ModelDependency,
    EffectDependency,
    SceneDependency,
}

/// Every record key of one kind, so per-kind walks visit only that kind.
pub(super) fn kind_range(
    kind: DefinitionKind,
) -> std::ops::RangeInclusive<(DefinitionKind, AssetId)> {
    (kind, AssetId([0; 16]))..=(kind, AssetId([u8::MAX; 16]))
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
    GuestInfluence,
    GuestFavouriteAnimalTrend,
    BehaviorSelection,
    NeedAdjustment,
    TranquilizerMode,
    AdmissionPriceBands,
    TransportHighlight,
    TankSurface,
    TankDepth,
    TankEdit,
    DiseaseExposure,
    FencePlacementMode,
    ShowPresentation,
    ShowScheduling,
    ShowScoring,
    ShowEditorPresentation,
    TourScoring,
    StaffWaterCleaning,
    StaffManager,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct DefinitionRow {
    pub(super) handle: Handle<WorldDefinitionAsset>,
    pub(super) first: usize,
    pub(super) end: usize,
}

#[derive(Message)]
pub struct WorldDefinitionSourcesPublished {
    pub overlay_generation: Option<u64>,
    pub changed_objects: Box<[AssetId]>,
    pub failed_sources: Box<[bevy::asset::AssetId<WorldDefinitionAsset>]>,
}

#[derive(Resource, Default)]
pub struct WorldDefinitions {
    pub(super) server: Option<AssetServer>,
    pub(super) binder_sources:
        Option<Arc<super::world_definition_asset_loading::authored_binder_reference_resolution::AuthoredBinderSources>>,
    pub(super) pending: std::collections::BTreeSet<bevy::asset::AssetId<WorldDefinitionAsset>>,
    pub(super) failed: std::collections::BTreeSet<bevy::asset::AssetId<WorldDefinitionAsset>>,
    pub(super) handles: Vec<Handle<WorldDefinitionAsset>>,
    pub(super) source_ids: std::collections::BTreeSet<bevy::asset::AssetId<WorldDefinitionAsset>>,
    pub(super) active_load: Option<Handle<super::world_definition_load_queue::WorldDefinitionLoadCompletion>>,
    pub(super) next_load_paths: Option<Vec<std::path::PathBuf>>,
    pub(super) routed_sources: BTreeMap<std::path::PathBuf, std::path::PathBuf>,
    pub(super) requested_overlay_generation: u64,
    pub(super) fame_documents: Vec<Handle<WorldDefinitionAsset>>,
    pub(super) complete: bool,
    pub(super) completed_loads: Arc<AtomicUsize>,
    pub(super) total_loads: usize,
    pub(super) records: BTreeMap<(DefinitionKind, AssetId), DefinitionRow>,
    pub(super) show_stages_by_object: BTreeMap<AssetId, DefinitionRow>,
    pub(super) animation_binder_rigs: BTreeMap<(AssetId, AssetId), DefinitionRow>,
    pub(super) vehicle_seats_by_index: BTreeMap<(AssetId, u16), DefinitionRow>,
    pub(super) singletons: BTreeMap<SingletonKind, Handle<WorldDefinitionAsset>>,
    pub(super) ground_path_surface_images: BTreeMap<AssetId, Handle<GroundPathSurfaceImage>>,
    pub(super) unlock_definition_identifiers_by_target: BTreeMap<AssetId, Vec<AssetId>>,
    pub(super) catalogue_authored_purchase_order: Vec<(usize, AssetId)>,
    pub(super) catalogue_by_definition: std::sync::OnceLock<BTreeMap<AssetId, (usize, AssetId)>>,
    pub(super) research_by_definition: std::sync::OnceLock<BTreeMap<AssetId, AssetId>>,
    pub(super) topology_cell_size_cm: u16,
    pub(super) maximum_ground_path_width_cm: u16,
}

impl WorldDefinitions {
    pub fn loading_progress(&self) -> (usize, usize) {
        (
            self.completed_loads.load(Ordering::Relaxed),
            self.total_loads,
        )
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    #[must_use]
    pub fn ground_path_surface_image(
        &self,
        id: AssetId,
    ) -> Option<&Handle<GroundPathSurfaceImage>> {
        self.ground_path_surface_images.get(&id)
    }

    #[must_use]
    pub fn owns_ground_path_surface_image(
        &self,
        id: bevy::asset::AssetId<GroundPathSurfaceImage>,
    ) -> bool {
        self.ground_path_surface_images
            .values()
            .any(|handle| handle.id() == id)
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn index_test_document(
        &mut self,
        handle: Handle<WorldDefinitionAsset>,
        document: &openzt2_game_data::world_definitions::document::WorldDefinitionDocument,
    ) {
        super::world_definition_asset_set_refresh_and_indexing::index_world_definition_document_records_and_singletons(
            document,
            &handle,
            &mut self.records,
            &mut self.singletons,
        );
        let mut indexed = document.show_stages.iter().collect::<Vec<_>>();
        indexed.sort_by_key(|stage| stage.id);
        for stage in indexed {
            if let Some(row) = self.records.get(&(DefinitionKind::ShowStage, stage.id)) {
                self.show_stages_by_object
                    .entry(stage.object)
                    .or_insert_with(|| row.clone());
            }
        }
        let mut indexed = document.vehicle_seats.iter().collect::<Vec<_>>();
        indexed.sort_by_key(|seat| seat.id);
        for seat in indexed {
            if let Some(row) = self.records.get(&(DefinitionKind::VehicleSeat, seat.id)) {
                self.vehicle_seats_by_index
                    .entry((seat.vehicle, seat.index))
                    .or_insert_with(|| row.clone());
            }
        }
        self.catalogue_by_definition.take();
        self.research_by_definition.take();
        document.unlocks.iter().for_each(|record| {
            self.unlock_definition_identifiers_by_target
                .entry(record.target)
                .or_default()
                .push(record.id);
        });
        self.catalogue_authored_purchase_order.extend(
            document
                .catalogue
                .iter()
                .enumerate()
                .map(|(index, entry)| (index, entry.id)),
        );
        // Fixtures rarely author timing, but the view reports ready only with it.
        self.singletons
            .insert(SingletonKind::SimulationTiming, handle.clone());
        self.topology_cell_size_cm = document
            .paths
            .iter()
            .map(|path| path.width_cm / 2)
            .min()
            .unwrap_or_default();
        self.maximum_ground_path_width_cm = self.maximum_ground_path_width_cm.max(
            document
                .paths
                .iter()
                .filter(|path| !path.elevated)
                .map(|path| path.width_cm)
                .max()
                .unwrap_or_default(),
        );
        if !document.fame_thresholds.is_empty() || !document.starting_fame.is_empty() {
            self.fame_documents.push(handle.clone());
        }
        self.handles.push(handle);
        self.complete = true;
    }

    /// Lets fixtures demand-load the scenes added with
    /// `WorldDefinitionAsset::with_test_scene` through `server`.
    #[cfg(any(test, feature = "testing"))]
    pub fn index_test_scenes(
        &mut self,
        server: AssetServer,
        handle: &Handle<WorldDefinitionAsset>,
        asset: &WorldDefinitionAsset,
    ) {
        for scene in &asset.scenes {
            self.records.insert(
                (DefinitionKind::SceneDependency, scene.id),
                DefinitionRow {
                    handle: handle.clone(),
                    first: 0,
                    end: 1,
                },
            );
        }
        self.server = Some(server);
    }

    pub fn get<'a>(
        &'a self,
        assets: &'a Assets<WorldDefinitionAsset>,
    ) -> Option<WorldDefinitionsView<'a>> {
        let ready = self.complete
            && !self.handles.is_empty()
            && self
                .singletons
                .contains_key(&SingletonKind::SimulationTiming);
        ready.then_some(WorldDefinitionsView {
            server: self.server.as_ref(),
            fame_documents: &self.fame_documents,
            records: &self.records,
            show_stages_by_object: &self.show_stages_by_object,
            animation_binder_rigs: &self.animation_binder_rigs,
            vehicle_seats_by_index: &self.vehicle_seats_by_index,
            singletons: &self.singletons,
            unlock_definition_identifiers_by_target: &self.unlock_definition_identifiers_by_target,
            catalogue_authored_purchase_order: &self.catalogue_authored_purchase_order,
            catalogue_by_definition: &self.catalogue_by_definition,
            research_by_definition: &self.research_by_definition,
            topology_cell_size_cm: self.topology_cell_size_cm,
            maximum_ground_path_width_cm: self.maximum_ground_path_width_cm,
            pending_assets: false,
            assets,
        })
    }
}

#[derive(Clone, Copy)]
pub struct WorldDefinitionsView<'a> {
    server: Option<&'a AssetServer>,
    fame_documents: &'a [Handle<WorldDefinitionAsset>],
    records: &'a BTreeMap<(DefinitionKind, AssetId), DefinitionRow>,
    show_stages_by_object: &'a BTreeMap<AssetId, DefinitionRow>,
    animation_binder_rigs: &'a BTreeMap<(AssetId, AssetId), DefinitionRow>,
    vehicle_seats_by_index: &'a BTreeMap<(AssetId, u16), DefinitionRow>,
    singletons: &'a BTreeMap<SingletonKind, Handle<WorldDefinitionAsset>>,
    unlock_definition_identifiers_by_target: &'a BTreeMap<AssetId, Vec<AssetId>>,
    catalogue_authored_purchase_order: &'a [(usize, AssetId)],
    catalogue_by_definition: &'a std::sync::OnceLock<BTreeMap<AssetId, (usize, AssetId)>>,
    research_by_definition: &'a std::sync::OnceLock<BTreeMap<AssetId, AssetId>>,
    topology_cell_size_cm: u16,
    maximum_ground_path_width_cm: u16,
    pending_assets: bool,
    assets: &'a Assets<WorldDefinitionAsset>,
}

macro_rules! finder {
    ($name:ident, $kind:ident, $field:ident, $record:ty) => {
        pub fn $name(self, id: AssetId) -> Option<&'a $record> {
            let row = self.records.get(&(DefinitionKind::$kind, id))?;
            self.assets.get(&row.handle)?.document.$field.get(row.first)
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
    pub fn collision_scores(
        self,
        object: AssetId,
    ) -> Option<&'a openzt2_game_data::world_definitions::world_objects::WorldObjectCollisionScores>
    {
        self.find_object(object)?.collision_scores.as_ref()
    }

    pub fn staff_interactions(
        self,
        object: AssetId,
    ) -> Option<&'a [super::interaction_policies::StaffInteraction]> {
        let row = self.records.get(&(DefinitionKind::Object, object))?;
        self.assets
            .get(&row.handle)?
            .metadata
            .staff_interactions
            .get(&object)
            .map(Vec::as_slice)
    }

    pub fn object_influences(
        self,
        object: AssetId,
    ) -> Option<&'a [super::interaction_policies::InfluenceSource]> {
        let row = self.records.get(&(DefinitionKind::Object, object))?;
        self.assets
            .get(&row.handle)?
            .metadata
            .object_influences
            .get(&object)
            .map(Vec::as_slice)
    }

    pub fn guest_influence_watches(
        self,
        guest: AssetId,
    ) -> Option<&'a [super::interaction_policies::GuestInfluenceWatch]> {
        let row = self.records.get(&(DefinitionKind::Guest, guest))?;
        self.assets
            .get(&row.handle)?
            .metadata
            .guest_influence_watches
            .get(&guest)
            .map(Vec::as_slice)
    }

    pub fn guest_influence_policy(
        self,
    ) -> Option<&'a super::interaction_policies::GuestInfluencePolicy> {
        self.assets
            .get(self.singletons.get(&SingletonKind::GuestInfluence)?)?
            .metadata
            .guest_influence_policy
            .as_ref()
    }
    pub fn guest_locomotion_tables(self, guest: AssetId) -> Option<&'a [StaffLocomotionTable]> {
        Some(&self.find_guest(guest)?.locomotion.tables)
    }

    pub fn guest_locomotion_directions(self, guest: AssetId) -> Option<&'a [openzt2_game_data::world_definitions::guest_simulation_definitions::GuestDirectionalLocomotionClip]>{
        Some(&self.find_guest(guest)?.locomotion.directions)
    }

    pub fn ambient_locomotion_tables(
        self,
        definition: AssetId,
    ) -> Option<&'a [StaffLocomotionTable]> {
        let row = self.records.get(&(DefinitionKind::Object, definition))?;
        self.assets
            .get(&row.handle)?
            .metadata
            .ambient_locomotion
            .get(&definition)
            .map(Vec::as_slice)
    }

    pub fn sky_station_stop_nodes(self, station: AssetId) -> Option<[Option<AssetId>; 4]> {
        let stops = self.find_station(station)?.sky_stop_nodes;
        stops.iter().any(Option::is_some).then_some(stops)
    }

    pub fn authored_timekeeper_policy(self) -> Option<super::timing::AuthoredTimekeeperPolicy> {
        self.assets
            .get(self.singletons.get(&SingletonKind::SimulationTiming)?)?
            .metadata
            .timekeeper
    }

    pub fn render_lod_distances(
        self,
        definition: AssetId,
        binder: AssetId,
    ) -> Option<&'a super::world_definition_document_asset_and_demand_loaded_dependency_paths::AuthoredRenderLodDistances>{
        let row = self.records.get(&(DefinitionKind::Object, definition))?;
        self.assets
            .get(&row.handle)?
            .metadata
            .render_lod_distances
            .get(&(definition, binder))
    }

    pub fn environment_map_target_node(self, definition: AssetId) -> Option<AssetId> {
        let row = self.records.get(&(DefinitionKind::Object, definition))?;
        self.assets
            .get(&row.handle)?
            .metadata
            .environment_map_nodes
            .get(&definition)
            .copied()
    }

    pub fn simulation_timestep(self) -> Option<std::time::Duration> {
        self.authored_timekeeper_policy()
            .map(|policy| policy.simulation.interval)
            .or_else(|| {
                let hertz = self.simulation_timing()?.fixed_hz;
                std::time::Duration::try_from_secs_f64(f64::from(hertz).recip()).ok()
            })
    }

    pub fn handle_for_path(self, id: AssetId) -> Option<&'a Handle<WorldDefinitionAsset>> {
        self.records
            .get(&(DefinitionKind::Path, id))
            .map(|row| &row.handle)
    }

    pub fn handle_for_fence(self, id: AssetId) -> Option<&'a Handle<WorldDefinitionAsset>> {
        self.records
            .get(&(DefinitionKind::Fence, id))
            .map(|row| &row.handle)
    }

    pub fn staff_request_rows(
        self,
        definition: AssetId,
    ) -> Option<&'a [StaffRequestControllerDefinition]> {
        let Some(row) = self
            .records
            .get(&(DefinitionKind::StaffRequest, definition))
        else {
            return (!self.has_pending_assets()).then_some(&[]);
        };
        self.assets
            .get(&row.handle)?
            .document
            .staff_requests
            .get(row.first..row.end)
    }

    pub fn animation_binder_rig_source(
        self,
        definition: AssetId,
        binder: AssetId,
    ) -> Option<(bevy::asset::AssetId<WorldDefinitionAsset>, usize)> {
        let row = self.animation_binder_rigs.get(&(definition, binder))?;
        Some((row.handle.id(), row.first))
    }

    pub fn has_object_record(self, id: AssetId) -> bool {
        self.records.contains_key(&(DefinitionKind::Object, id))
    }

    /// The authored binder type spelling of an object, e.g. `Poop_Medium`.
    pub fn object_binder_type(self, id: AssetId) -> Option<&'a str> {
        let handle = &self.records.get(&(DefinitionKind::Object, id))?.handle;
        self.assets
            .get(handle)?
            .document
            .object_binder_types
            .iter()
            .find_map(|(object, binder_type)| (*object == id).then_some(&**binder_type))
    }

    pub const fn has_pending_assets(self) -> bool {
        self.pending_assets
    }

    fn fame_documents(self) -> impl Iterator<Item = &'a WorldDefinitionAsset> {
        self.fame_documents
            .iter()
            .rev()
            .filter_map(|handle| self.assets.get(handle))
    }

    pub fn path_stomp_policy(self, id: AssetId) -> Option<&'a PlacementStompPolicy> {
        let owner = &self.records.get(&(DefinitionKind::Path, id))?.handle;
        self.assets
            .get(owner)?
            .document
            .path_stomp_policies
            .get(&id)
    }

    pub fn placement_target_facts(self, id: AssetId) -> Option<&'a PlacementTargetFacts> {
        let owner = &self.records.get(&(DefinitionKind::Object, id))?.handle;
        self.assets
            .get(owner)?
            .document
            .placement_target_facts
            .get(&id)
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
    pub fn load_staff_model_animation_set<T: Asset>(
        self,
        staff_role: AssetId,
        drawn_model_animation_set: Option<AssetId>,
    ) -> Option<Handle<T>> {
        let row = self.records.get(&(DefinitionKind::Staff, staff_role))?;
        let asset = self.assets.get(&row.handle)?;
        let model_animation_set = asset.document.staff.get(row.first)?.model_animation_set;
        asset.animation_set(
            self.server?,
            drawn_model_animation_set.unwrap_or(model_animation_set),
        )
    }
    finder!(find_staff_job, StaffJob, staff_jobs, StaffJobDefinition);
    finder!(
        find_staff_tank_cleaning_request,
        StaffTankCleaningRequest,
        staff_tank_cleaning_requests,
        StaffAquaticTankCleaningRequestPolicy
    );
    finder!(find_guest, Guest, guests, GuestDefinition);

    pub fn guest_show_need(
        self,
        guest: AssetId,
    ) -> Option<
        openzt2_game_data::world_definitions::guest_simulation_definitions::GuestShowNeedDefinition,
    > {
        self.find_guest(guest)?.show_need
    }
    finder!(
        find_entity_naming,
        EntityNaming,
        entity_naming,
        openzt2_game_data::world_definitions::entity_naming::EntityNamingDefinition
    );
    finder!(
        find_animal_adoption_species_eligibility,
        AnimalAdoptionSpeciesEligibility,
        animal_adoption_species_eligibility,
        AnimalAdoptionSpeciesEligibility
    );

    pub fn animal_adoption_species_eligibility(
        self,
    ) -> impl Iterator<Item = &'a AnimalAdoptionSpeciesEligibility> {
        self.records
            .range(kind_range(DefinitionKind::AnimalAdoptionSpeciesEligibility))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .animal_adoption_species_eligibility
                    .get(row.first)
            })
    }
    /// The guest body's actor manifest, from the asset that declares the guest.
    pub fn load_guest_model_animation_set<T: Asset>(self, guest: AssetId) -> Option<Handle<T>> {
        let row = self.records.get(&(DefinitionKind::Guest, guest))?;
        let asset = self.assets.get(&row.handle)?;
        let model_animation_set = asset.document.guests.get(row.first)?.model_animation_set?;
        asset.animation_set(self.server?, model_animation_set)
    }
    pub fn guest_definitions(self) -> impl Iterator<Item = &'a GuestDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Guest))
            .filter_map(move |(_, row)| {
                self.assets.get(&row.handle)?.document.guests.get(row.first)
            })
    }
    pub fn ordinary_guest_definitions(self) -> impl Iterator<Item = &'a GuestDefinition> {
        let test_binder = AssetId::from_key("guest_adult_m");
        self.guest_definitions()
            .filter(move |guest| guest.id != test_binder)
    }
    finder!(
        find_viewing_opportunity,
        ViewingOpportunity,
        viewing_opportunities,
        ViewingOpportunityDefinition
    );
    finder!(find_fence, Fence, fences, FenceDefinition);
    pub fn object_record_key(self, id: AssetId) -> Option<&'a str> {
        self.assets
            .get(&self.records.get(&(DefinitionKind::Object, id))?.handle)?
            .document
            .object_record_keys
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, key)| key.as_str())
    }
    /// The definition whose models and gate policy a gated edge of `fence`
    /// uses: the fence's own gate variant (`chainlink` -> `chainlinkgate`),
    /// or the fence itself when it is a gate or has no loaded variant.
    pub fn find_gate_fence(self, fence: AssetId) -> Option<&'a FenceDefinition> {
        let definition = self.find_fence(fence)?;
        Some(
            (definition.gate != fence)
                .then(|| self.find_fence(definition.gate))
                .flatten()
                .unwrap_or(definition),
        )
    }
    /// Loads the actor manifest holding a gate fence's open and close clips.
    pub fn load_fence_gate_animation_set<T: Asset>(self, fence: AssetId) -> Option<Handle<T>> {
        let fence = self.find_gate_fence(fence)?.id;
        let row = self.records.get(&(DefinitionKind::Fence, fence))?;
        let asset = self.assets.get(&row.handle)?;
        let animation_set = asset
            .document
            .fences
            .get(row.first)?
            .gate_policy
            .animation_set;
        asset.animation_set(self.server?, animation_set)
    }
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
        let row = self.records.get(&(
            DefinitionKind::FameThreshold,
            AssetId::from_key(&format!("fame-threshold:{level}")),
        ))?;
        self.assets
            .get(&row.handle)?
            .document
            .fame_thresholds
            .get(row.first)
    }
    finder!(find_award, Award, awards, AwardDefinition);
    finder!(find_tank, Tank, tanks, TankDefinition);
    finder!(find_show_stage, ShowStage, show_stages, ShowStageDefinition);
    pub fn find_show_stage_definition_for_world_object(
        self,
        world_object_definition: AssetId,
    ) -> Option<&'a ShowStageDefinition> {
        let row = self.show_stages_by_object.get(&world_object_definition)?;
        self.assets
            .get(&row.handle)?
            .document
            .show_stages
            .get(row.first)
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
            .range(kind_range(DefinitionKind::Object))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .objects
                    .get(row.first)
            })
    }

    pub fn placeables(self) -> impl Iterator<Item = &'a PlaceableDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Placeable))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .placeables
                    .get(row.first)
            })
    }

    pub fn paths(self) -> impl Iterator<Item = &'a GuestPathDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Path))
            .filter_map(move |(_, row)| self.assets.get(&row.handle)?.document.paths.get(row.first))
    }

    pub fn is_automatic_ground_path_curb(self, definition: AssetId) -> bool {
        definition != AssetId::default()
            && self
                .paths()
                .any(|path| !path.elevated && path.curb == definition)
    }

    pub fn fences(self) -> impl Iterator<Item = &'a FenceDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Fence))
            .filter_map(move |(_, row)| {
                self.assets.get(&row.handle)?.document.fences.get(row.first)
            })
    }

    pub fn scene(self, id: AssetId) -> Option<Handle<ScenePrefabAsset>> {
        let server = self.server?;
        self.assets
            .get(
                &self
                    .records
                    .get(&(DefinitionKind::SceneDependency, id))?
                    .handle,
            )?
            .scene(server, id)
    }

    pub fn catalogue(self) -> impl Iterator<Item = &'a CatalogueEntry> {
        self.records
            .range(kind_range(DefinitionKind::Catalogue))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .catalogue
                    .get(row.first)
            })
    }

    pub fn catalogue_entry_for_definition(
        self,
        definition: AssetId,
    ) -> Option<(usize, &'a CatalogueEntry)> {
        let index = self.catalogue_by_definition.get_or_init(|| {
            let mut index = BTreeMap::new();
            for (order, entry) in self.catalogue().enumerate() {
                index.entry(entry.definition).or_insert((order, entry.id));
            }
            index
        });
        let &(order, id) = index.get(&definition)?;
        Some((order, self.find_catalogue_entry(id)?))
    }

    pub fn research_for_definition(self, definition: AssetId) -> Option<AssetId> {
        self.research_by_definition
            .get_or_init(|| {
                let mut index = BTreeMap::new();
                for research in self.research() {
                    for target in std::iter::once(&research.id).chain(&research.unlocks) {
                        index.entry(*target).or_insert(research.id);
                    }
                }
                index
            })
            .get(&definition)
            .copied()
    }

    pub fn catalogue_in_authored_purchase_order(
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
            .range(kind_range(DefinitionKind::Zoopedia))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .zoopedia
                    .get(row.first)
            })
    }

    pub fn unlocks(self) -> impl Iterator<Item = &'a UnlockDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Unlock))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .unlocks
                    .get(row.first)
            })
    }

    pub fn unlock_definitions_targeting_catalogue_definition(
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
            .range(kind_range(DefinitionKind::Tranquilizer))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .tranquilizers
                    .get(row.first)
            })
    }

    pub fn diseases(self) -> impl Iterator<Item = &'a DiseaseDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Disease))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .diseases
                    .get(row.first)
            })
    }

    pub fn rampage_rules(self) -> impl Iterator<Item = &'a RampageRule> {
        self.records
            .range(kind_range(DefinitionKind::Rampage))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .rampage_rules
                    .get(row.first)
            })
    }

    pub fn staff_jobs(self) -> impl Iterator<Item = &'a StaffJobDefinition> {
        self.records
            .range(kind_range(DefinitionKind::StaffJob))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .staff_jobs
                    .get(row.first)
            })
    }

    pub fn biomes(self) -> impl Iterator<Item = &'a BiomeDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Biome))
            .filter_map(move |(_, row)| {
                self.assets.get(&row.handle)?.document.biomes.get(row.first)
            })
    }

    pub fn brushes(self) -> impl Iterator<Item = &'a TerrainEditingBrushDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Brush))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .brushes
                    .get(row.first)
            })
    }

    pub fn tricks(self) -> impl Iterator<Item = &'a TrickDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Trick))
            .filter_map(move |(_, row)| {
                self.assets.get(&row.handle)?.document.tricks.get(row.first)
            })
    }

    pub fn stations(self) -> impl Iterator<Item = &'a TransportationStationDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Station))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .stations
                    .get(row.first)
            })
    }

    pub fn tracks(self) -> impl Iterator<Item = &'a TransportationTrackDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Track))
            .filter_map(move |(_, row)| {
                self.assets.get(&row.handle)?.document.tracks.get(row.first)
            })
    }

    pub fn vehicles(self) -> impl Iterator<Item = &'a TransportationVehicleDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Vehicle))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .vehicles
                    .get(row.first)
            })
    }

    pub fn tour_views(self) -> impl Iterator<Item = &'a TourViewDefinition> {
        self.records
            .range(kind_range(DefinitionKind::TourView))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .tour_views
                    .get(row.first)
            })
    }

    pub fn fossil_slots(self) -> impl Iterator<Item = &'a FossilSlotDefinition> {
        self.records
            .range(kind_range(DefinitionKind::FossilSlot))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .fossil_slots
                    .get(row.first)
            })
    }

    pub fn cloning_centers(self) -> impl Iterator<Item = &'a CloningCenterDefinition> {
        self.records
            .range(kind_range(DefinitionKind::CloningCenter))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .cloning_centers
                    .get(row.first)
            })
    }

    pub fn research(self) -> impl Iterator<Item = &'a ResearchDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Research))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .research
                    .get(row.first)
            })
    }

    pub fn rating_definitions(self) -> impl Iterator<Item = &'a RatingDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Rating))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .rating_definitions
                    .get(row.first)
            })
    }

    pub fn starting_fame(self, mode: AssetId) -> Option<&'a StartingFamePolicy> {
        self.fame_documents()
            .flat_map(|asset| asset.document.starting_fame.iter())
            .find(|policy| policy.mode == mode)
    }

    pub fn fame_thresholds(self) -> impl Iterator<Item = &'a FameThreshold> {
        self.fame_documents()
            .flat_map(|asset| asset.document.fame_thresholds.iter())
    }

    pub fn awards(self) -> impl Iterator<Item = &'a AwardDefinition> {
        self.records
            .range(kind_range(DefinitionKind::Award))
            .filter_map(move |(_, row)| {
                self.assets.get(&row.handle)?.document.awards.get(row.first)
            })
    }

    pub fn show_audio_cues(self) -> impl Iterator<Item = &'a ShowAudioCue> {
        self.records
            .range(kind_range(DefinitionKind::ShowAudioCue))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .show_audio_cues
                    .get(row.first)
            })
    }

    pub fn fossil_pieces(self) -> impl Iterator<Item = &'a FossilPieceDefinition> {
        self.records
            .range(kind_range(DefinitionKind::FossilPiece))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .fossil_pieces
                    .get(row.first)
            })
    }

    pub fn immersive_mode_policies(self) -> impl Iterator<Item = &'a ImmersiveModePolicy> {
        self.records
            .range(kind_range(DefinitionKind::ImmersiveModePolicy))
            .filter_map(move |(_, row)| {
                self.assets
                    .get(&row.handle)?
                    .document
                    .immersive_mode_policies
                    .get(row.first)
            })
    }

    pub fn find_aquatic_requirement(self, species: AssetId) -> Option<&'a AquaticRequirement> {
        let row = self.records.get(&(DefinitionKind::Aquatic, species))?;
        self.assets
            .get(&row.handle)?
            .document
            .aquatic_requirements
            .get(row.first)
    }

    pub fn find_maintenance_by_object(self, object: AssetId) -> Option<&'a MaintenanceDefinition> {
        let row = self
            .records
            .get(&(DefinitionKind::MaintenanceByObject, object))?;
        self.assets
            .get(&row.handle)?
            .document
            .maintenance_definitions
            .get(row.first)
    }

    pub fn find_facility_by_object(self, object: AssetId) -> Option<&'a FacilityDefinition> {
        let row = self
            .records
            .get(&(DefinitionKind::FacilityByObject, object))?;
        self.assets
            .get(&row.handle)?
            .document
            .facilities
            .get(row.first)
    }

    pub fn find_staff_by_object(self, object: AssetId) -> Option<&'a StaffRoleDefinition> {
        let row = self.records.get(&(DefinitionKind::StaffByObject, object))?;
        self.assets.get(&row.handle)?.document.staff.get(row.first)
    }

    pub fn find_vehicle_seat_by_index(
        self,
        vehicle: AssetId,
        index: u16,
    ) -> Option<&'a TransportationVehicleSeatDefinition> {
        let row = self.vehicle_seats_by_index.get(&(vehicle, index))?;
        self.assets
            .get(&row.handle)?
            .document
            .vehicle_seats
            .get(row.first)
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
        guest_favourite_animal_trend,
        GuestFavouriteAnimalTrend,
        guest_favourite_animal_trend,
        GuestFavouriteAnimalTrendPolicy
    );
    singleton!(
        behavior_selection_policy,
        BehaviorSelection,
        behavior_selection_policy,
        BehaviorSelectionPolicy
    );
    singleton!(
        need_adjustment,
        NeedAdjustment,
        need_adjustment,
        NeedAdjustmentPolicy
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
        transport_highlight,
        TransportHighlight,
        transport_highlight,
        TransportHighlightPolicy
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
        disease_exposure,
        DiseaseExposure,
        disease_exposure,
        openzt2_game_data::world_definitions::animal_health::DiseaseExposurePolicy
    );
    singleton!(
        fence_placement_mode_policy,
        FencePlacementMode,
        fence_placement_mode_policy,
        FencePlacementModePolicy
    );
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

    pub fn maximum_ground_path_width_cm(self) -> u16 {
        self.maximum_ground_path_width_cm
    }

    pub fn texture_image(self, id: AssetId) -> Option<Handle<Image>> {
        let server = self.server?;
        self.assets
            .get(
                &self
                    .records
                    .get(&(DefinitionKind::TextureDependency, id))?
                    .handle,
            )?
            .texture(server, id)
    }

    pub(super) fn texture_path(self, id: AssetId) -> Option<&'a str> {
        self.assets
            .get(
                &self
                    .records
                    .get(&(DefinitionKind::TextureDependency, id))?
                    .handle,
            )?
            .texture_path(id)
    }

    pub fn material(
        self,
        id: AssetId,
    ) -> Option<Handle<crate::material::material_asset_types::MaterialAsset>> {
        let row = self
            .records
            .get(&(DefinitionKind::MaterialDependency, id))?;
        self.assets.get(&row.handle)?.material(self.server?, id)
    }

    pub fn tank_water_material_path(self, id: AssetId) -> Option<&'a str> {
        let row = self
            .records
            .get(&(DefinitionKind::TankWaterMaterialDependency, id))?;
        self.assets.get(&row.handle)?.tank_water_material_path(id)
    }

    pub fn tank_water_material_paths(self) -> impl Iterator<Item = &'a str> {
        self.records
            .range(kind_range(DefinitionKind::TankWaterMaterialDependency))
            .filter_map(move |(&(_, id), row)| {
                self.assets.get(&row.handle)?.tank_water_material_path(id)
            })
    }

    pub fn model(self, id: AssetId) -> Option<Handle<Gltf>> {
        let server = self.server?;
        self.assets
            .get(
                &self
                    .records
                    .get(&(DefinitionKind::ModelDependency, id))?
                    .handle,
            )?
            .model(server, id)
    }

    pub fn effect(self, id: AssetId) -> Option<Handle<ParticleEffectDocumentAsset>> {
        let server = self.server?;
        self.assets
            .get(
                &self
                    .records
                    .get(&(DefinitionKind::EffectDependency, id))?
                    .handle,
            )?
            .effect(server, id)
    }
}
