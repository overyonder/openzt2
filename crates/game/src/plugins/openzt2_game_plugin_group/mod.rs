//! Dependency-ordered registration of OpenZT2's Bevy gameplay plugins.

use bevy::{app::PluginGroupBuilder, prelude::*};

pub(crate) struct OpenZt2GamePluginGroup;

impl PluginGroup for OpenZt2GamePluginGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(super::input::GameInputPlugin)
            .add(super::simulation_time::SimulationTimePlugin)
            .add(super::persistence::PersistencePlugin)
            .add(super::ui::GameUiPlugin)
            .add(super::mod_manager::ModManagerPlugin)
            .add(super::shell::ShellPlugin)
            .add(super::world_spawn::WorldSpawnPlugin)
            .add(super::animation_graph::AnimationGraphPlugin)
            .add(super::animation_playback::AnimationPlaybackPlugin)
            .add(super::model_render::ModelRenderPlugin)
            .add(super::terrain::TerrainPlugin)
            .add(super::topology::TopologyPlugin)
            .add(super::habitat::HabitatPlugin)
            .add(super::physics::ZooPhysicsPlugin)
            .add(super::locomotion::LocomotionPlugin)
            .add(super::camera::ZooCameraPlugin)
            .add(super::construction::ConstructionPlugin)
            .add(super::placement::ObjectPlacementPlugin)
            .add(super::economy::EconomyPlugin)
            .add(super::animal_lifecycle::AnimalLifecyclePlugin)
            .add(super::animal_welfare::AnimalWelfarePlugin)
            .add(super::feeding::FeedingPlugin)
            .add(super::animal_behavior::AnimalBehaviorExecutionPlugin)
            .add(super::animal_health::AnimalHealthPlugin)
            .add(super::guests::GuestsPlugin)
            .add(super::donations::GuestDonationDecisionAndPaymentPlugin)
            .add(super::staff::StaffPlugin)
            .add(super::staff_request_runtime::StaffRequestRuntimePlugin)
            .add(super::information::InformationPlugin)
            .add(super::progression::ProgressionPlugin)
            .add(super::scenario::ScenarioPlugin)
            .add(super::aquatic::AquaticPlugin)
            .add(super::audio::AudioPlugin)
            .add(super::shows::ShowsPlugin)
            .add(super::transport_tours::TransportToursPlugin)
            .add(super::photos::PhotosPlugin)
            .add(super::extinct_animals::ExtinctAnimalsPlugin)
            .add(super::immersive_modes::ImmersiveModesPlugin)
            .add(super::environment::EnvironmentPlugin)
            .add(super::settings::SettingsPlugin)
            .add(super::maintenance::MaintenancePlugin)
    }
}
