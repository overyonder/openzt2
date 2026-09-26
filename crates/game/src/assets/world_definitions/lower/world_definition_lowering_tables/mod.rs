use super::world_definition_source_value_reading_and_conversion::simple_error;
use crate::assets::source_document::resolved_source_record_index::BindError;
use openzt2_game_data::world_definitions::animal_shows_and_training::ShowPlatformUpgradePolicy;
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
use openzt2_game_data::world_definitions::environment::EnvironmentDefinition;
use openzt2_game_data::world_definitions::facilities_and_maintenance::CleanlinessPolicy;
use openzt2_game_data::world_definitions::fences_and_gates::FenceGatePolicy;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestViewingPolicy;
use openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition;
use openzt2_game_data::world_definitions::transportation_and_tours::{
    TourCategoryScore, TourRatingRange,
};
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct WorldDefinitionLoweringTables {
    pub(super) document: WorldDefinitionDocument,
    pub(super) show_presentation_interval_ns: Option<u64>,
    pub(super) show_editor_interval_ns: Option<u64>,
    pub(super) tour_category_scores: Vec<TourCategoryScore>,
    pub(super) tour_rating_ranges: Vec<TourRatingRange>,
}

impl WorldDefinitionLoweringTables {
    pub(super) fn environment_definition(&mut self, id: AssetId) -> &mut EnvironmentDefinition {
        if let Some(index) = self
            .document
            .environments
            .iter()
            .position(|environment| environment.id == id)
        {
            return &mut self.document.environments[index];
        }
        self.document.environments.push(EnvironmentDefinition {
            id,
            family: AssetId::default(),
            light_keyframes: Vec::new(),
            fog_keyframes: Vec::new(),
            sky_keyframes: Vec::new(),
            weather: Vec::new(),
            transitions: Vec::new(),
            ambient: Vec::new(),
            fog_samples: Vec::new(),
            map_samples: Vec::new(),
            visual_samples: Vec::new(),
            light_samples: Vec::new(),
            initial_weather: AssetId::default(),
            wind_mps: [0.0; 2],
        });
        let index = self.document.environments.len() - 1;
        &mut self.document.environments[index]
    }

    pub(super) fn append_resolved_world_definition_lowering_shard(
        &mut self,
        mut shard: Self,
    ) -> Result<(), BindError> {
        macro_rules! append {
            ($($field:ident),+ $(,)?) => { $(self.document.$field.append(&mut shard.document.$field);)+ };
        }
        append!(
            objects,
            placeables,
            facilities,
            maintenance_definitions,
            staff,
            staff_jobs,
            staff_requests,
            guests,
            viewing_opportunities,
            fences,
            paths,
            biomes,
            locations,
            brushes,
            diseases,
            treatments,
            tranquilizers,
            rampage_rules,
            catalogue,
            zoopedia,
            research,
            unlocks,
            rating_definitions,
            fame_thresholds,
            awards,
            tanks,
            aquatic_requirements,
            show_stages,
            tricks,
            trick_outcome_tokens,
            show_rules,
            stations,
            tracks,
            vehicles,
            vehicle_seats,
            tour_views,
            fossil_sets,
            fossil_pieces,
            fossil_slots,
            cloning_centers,
            immersive_mode_policies,
            cameras,
            show_audio_cues,
            show_presentation_icons,
            person_name_pools,
            weather,
            ambient_spawns,
        );

        self.tour_category_scores
            .append(&mut shard.tour_category_scores);
        self.tour_rating_ranges
            .append(&mut shard.tour_rating_ranges);

        for incoming in shard.document.environments {
            let environment = self.environment_definition(incoming.id);
            if incoming.family != AssetId::default() {
                environment.family = incoming.family;
            }
            environment.light_keyframes.extend(incoming.light_keyframes);
            environment.fog_keyframes.extend(incoming.fog_keyframes);
            environment.sky_keyframes.extend(incoming.sky_keyframes);
            environment.weather.extend(incoming.weather);
            environment.transitions.extend(incoming.transitions);
            environment.ambient.extend(incoming.ambient);
            environment.fog_samples.extend(incoming.fog_samples);
            environment.map_samples.extend(incoming.map_samples);
            environment.visual_samples.extend(incoming.visual_samples);
            environment.light_samples.extend(incoming.light_samples);
            if incoming.initial_weather != AssetId::default() {
                environment.initial_weather = incoming.initial_weather;
            }
            if incoming.wind_mps != [0.0; 2] {
                environment.wind_mps = incoming.wind_mps;
            }
        }

        macro_rules! merge_option {
            ($($field:ident),+ $(,)?) => { $(
                if let Some(value) = shard.document.$field.take() {
                    if self.document.$field.replace(value).is_some() {
                        return Err(simple_error(concat!("duplicate resolved ", stringify!($field))));
                    }
                }
            )+ };
        }
        merge_option!(
            staff_manager,
            fossil_placement,
            guest_generation,
            behavior_selection_policy,
            tank_surface_policy,
            tank_depth_policy,
            tank_edit_policy,
            show_presentation,
            show_scheduling,
            show_scoring,
            show_editor_presentation,
            tour_scoring,
            staff_water_cleaning,
            animal_adoption_offer_configuration,
        );
        if let Some(value) = shard.document.biome_detail_placement {
            if self
                .document
                .biome_detail_placement
                .replace(value)
                .is_some()
            {
                return Err(simple_error("duplicate resolved biome detail placement"));
            }
        }
        if let Some(value) = shard.show_presentation_interval_ns {
            if self.show_presentation_interval_ns.replace(value).is_some() {
                return Err(simple_error(
                    "duplicate resolved show presentation interval",
                ));
            }
        }
        if let Some(value) = shard.show_editor_interval_ns {
            if self.show_editor_interval_ns.replace(value).is_some() {
                return Err(simple_error("duplicate resolved show editor interval"));
            }
        }
        Ok(())
    }

    pub(super) fn sort_tables_and_reject_duplicate_identifiers(&mut self) -> Result<(), BindError> {
        macro_rules! sort {
            ($field:ident) => {{
                self.document
                    .$field
                    .sort_unstable_by_key(|record| record.id.0);
                if self
                    .document
                    .$field
                    .windows(2)
                    .any(|pair| pair[0].id == pair[1].id)
                {
                    return Err(simple_error(concat!(
                        "duplicate ",
                        stringify!($field),
                        " id"
                    )));
                }
            }};
        }
        sort!(objects);
        sort!(placeables);
        sort!(facilities);
        sort!(maintenance_definitions);
        sort!(staff);
        sort!(staff_jobs);
        // Multiple authored controllers can belong to one definition. Preserve
        // their source order within that owner instead of rejecting its rows.
        self.document.staff_requests.sort_by_key(|record| record.id);
        sort!(guests);
        sort!(viewing_opportunities);
        let gate_presentations = self
            .document
            .fences
            .iter()
            .map(|gate| {
                let mut policy = gate.gate_policy;
                if policy.prefab == AssetId::default() {
                    policy.prefab = gate.segments.cardinal_straight;
                }
                (gate.id, policy)
            })
            .collect::<BTreeMap<_, _>>();
        for fence in &mut self.document.fences {
            if fence.gate == AssetId::default() {
                continue;
            }
            if let Some(policy) = gate_presentations.get(&fence.gate) {
                fence.gate_policy = *policy;
            } else {
                fence.gate = AssetId::default();
                fence.gate_policy = FenceGatePolicy {
                    prefab: AssetId::default(),
                    open_animation: AssetId::default(),
                    close_animation: AssetId::default(),
                    trigger_distance_cm: 0,
                    auto_close_ticks: 0,
                };
            }
        }
        sort!(fences);
        sort!(paths);
        sort!(biomes);
        sort!(locations);
        sort!(brushes);
        sort!(diseases);
        sort!(treatments);
        sort!(tranquilizers);
        sort!(rampage_rules);
        sort!(catalogue);
        sort!(zoopedia);
        sort!(research);
        sort!(unlocks);
        sort!(rating_definitions);
        sort!(awards);
        sort!(tanks);
        sort!(show_stages);
        sort!(tricks);
        sort!(trick_outcome_tokens);
        sort!(show_rules);
        sort!(show_audio_cues);
        sort!(show_presentation_icons);
        sort!(stations);
        sort!(tracks);
        sort!(vehicles);
        sort!(vehicle_seats);
        sort!(tour_views);
        sort!(fossil_sets);
        sort!(fossil_pieces);
        sort!(fossil_slots);
        sort!(cloning_centers);
        sort!(immersive_mode_policies);
        sort!(cameras);
        sort!(environments);
        sort!(weather);
        sort!(ambient_spawns);
        sort!(person_name_pools);
        Ok(())
    }

    pub(super) fn finish_canonical_world_definition_document(
        mut self,
        simulation_timing: Option<SimulationTimingDefinition>,
        cleanliness_policy: Option<CleanlinessPolicy>,
        show_platform_upgrades: Option<ShowPlatformUpgradePolicy>,
        guest_viewing: Option<GuestViewingPolicy>,
    ) -> WorldDefinitionDocument {
        if let Some(policy) = self.document.show_scheduling.as_mut() {
            policy.presentation_update_interval_ns =
                self.show_presentation_interval_ns.unwrap_or_default();
            policy.editor_update_interval_ns = self.show_editor_interval_ns.unwrap_or_default();
        }
        if let Some(policy) = self.document.tour_scoring.as_mut() {
            policy.categories = self.tour_category_scores;
            policy.rating_ranges = self.tour_rating_ranges;
        }
        self.document.simulation_timing = simulation_timing;
        self.document.cleanliness_policy = cleanliness_policy;
        self.document.show_platform_upgrades = show_platform_upgrades;
        self.document.guest_viewing = guest_viewing;
        self.document
    }
}
