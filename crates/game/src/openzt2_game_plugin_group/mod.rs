//! Dependency-ordered registration of OpenZT2's Bevy gameplay plugins.

use bevy::{app::PluginGroupBuilder, prelude::*};

pub(crate) struct OpenZt2GamePluginGroup;

impl PluginGroup for OpenZt2GamePluginGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(openzt2_player_controls::input::GameInputPlugin)
            .add(openzt2_simulation_time::SimulationTimePlugin)
            .add(openzt2_session::persistence::PersistencePlugin)
            .add(openzt2_ui::GameUiPlugin)
            .add(openzt2_session::mod_manager::ModManagerPlugin)
            .add(openzt2_session::shell::ShellPlugin)
            .add(openzt2_world::world_spawn::WorldSpawnPlugin)
            .add(openzt2_world::animation_graph::AnimationGraphPlugin)
            .add(openzt2_world::animation_playback::AnimationPlaybackPlugin)
            .add(openzt2_world::model_render::ModelRenderPlugin)
            .add(openzt2_terrain::terrain::TerrainPlugin)
            .add(openzt2_construction::topology::TopologyPlugin)
            .add(openzt2_navigation::habitat::HabitatPlugin)
            .add(openzt2_world::physics::ZooPhysicsPlugin)
            .add(openzt2_navigation::locomotion::LocomotionPlugin)
            .add(openzt2_player_controls::camera::ZooCameraPlugin)
            .add(openzt2_construction::construction::ConstructionPlugin)
            .add(openzt2_construction::placement::ObjectPlacementPlugin)
            .add(openzt2_economy::economy::EconomyPlugin)
            .add(openzt2_animals::animal_lifecycle::AnimalLifecyclePlugin)
            .add(openzt2_animals::animal_welfare::AnimalWelfarePlugin)
            .add(openzt2_animals::feeding::FeedingPlugin)
            .add(openzt2_animals::animal_behavior::AnimalBehaviorExecutionPlugin)
            .add(openzt2_animals::animal_health::AnimalHealthPlugin)
            .add(openzt2_visitors::guests::GuestsPlugin)
            .add(openzt2_visitors::donations::GuestDonationDecisionAndPaymentPlugin)
            .add(openzt2_zoo_staff::staff::StaffPlugin)
            .add(openzt2_staff_requests::StaffRequestRuntimePlugin)
            .add(openzt2_information::InformationPlugin)
            .add(openzt2_progression::progression::ProgressionPlugin)
            .add(openzt2_session::scenario::ScenarioPlugin)
            .add(openzt2_aquatic::aquatic::AquaticPlugin)
            .add(openzt2_audio::audio::AudioPlugin)
            .add(openzt2_shows::shows::ShowsPlugin)
            .add(openzt2_transport::transport_tours::TransportToursPlugin)
            .add(openzt2_photos::PhotosPlugin)
            .add(openzt2_extinct_animals::extinct_animals::ExtinctAnimalsPlugin)
            .add(openzt2_player_controls::immersive_modes::ImmersiveModesPlugin)
            .add(openzt2_environment::EnvironmentPlugin)
            .add(openzt2_settings::SettingsPlugin)
            .add(openzt2_zoo_staff::maintenance::MaintenancePlugin)
    }
}
