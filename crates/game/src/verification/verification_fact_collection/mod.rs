use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    application_lifecycle::GamePhase,
    plugins::{
        animal_lifecycle::types::{Animal, AnimalSex},
        camera::camera_runtime_state_types::ZooCamera,
        construction::{
            construction_interaction_types::{
                ConstructionCursor, ConstructionPreview, PlacementValidity,
            },
            construction_tool_and_placement_policy_types::ConstructionTool,
        },
        economy::{
            facility_economy_types::ServiceFacility,
            guest_admission_types::ZooAdmissionsOpen,
            zoo_cash_types::{UnlimitedZooCash, ZooCash},
        },
        guests::guest_simulation_types::Guest,
        habitat::habitat_types::{Containment, Habitat},
        information::entity_selection_types::SelectedEntity,
        photos::{photo_album_types::CameraRollPhoto, photo_capture_types::Photo},
        placement::placed_object_types::PlacedObjectDefinitionReference,
        settings::{
            display_settings_types::{DisplayMode, DisplaySettings},
            graphics_settings_types::GraphicsSettings,
            online_message_policy_types::OnlineMessagePolicy,
            options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
        },
        simulation_time::simulation_control_types::SimulationControl,
        staff::staff_employment_types::Employment,
        topology::topology_graph_types::{FenceEdge, PathTile},
        ui::picking::UiPointerCapture,
    },
};

use super::{
    verification_game_phase_readiness::game_phase_order,
    verification_journey_state_types::{
        VerificationFactCollectionState, VerificationFacts, VerificationJourneyReport,
        VerificationJourneyRun,
    },
};

#[derive(SystemParam)]
pub(crate) struct VerificationWorldFactQueries<'w, 's> {
    phase: Res<'w, State<GamePhase>>,
    cash: Option<Res<'w, ZooCash>>,
    unlimited_cash: Option<Res<'w, UnlimitedZooCash>>,
    admissions: Option<Res<'w, ZooAdmissionsOpen>>,
    selected: Res<'w, SelectedEntity>,
    construction_tool: Res<'w, ConstructionTool>,
    ui_pointer_capture: Res<'w, UiPointerCapture>,
    display: Res<'w, DisplaySettings>,
    graphics: Res<'w, GraphicsSettings>,
    online_messages: Res<'w, OnlineMessagePolicy>,
    settings_draft: Res<'w, OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
    simulation: Option<Res<'w, SimulationControl>>,
    cursors: Query<'w, 's, &'static ConstructionCursor>,
    previews: Query<'w, 's, (&'static ConstructionPreview, &'static Visibility)>,
    fences: Query<'w, 's, (), With<FenceEdge>>,
    habitats: Query<'w, 's, (), With<Habitat>>,
    animals:
        Query<'w, 's, (Option<&'static Containment>, Option<&'static AnimalSex>), With<Animal>>,
    staff: Query<'w, 's, (), With<Employment>>,
    facilities: Query<'w, 's, (), With<ServiceFacility>>,
    guests: Query<'w, 's, (), With<Guest>>,
    placed_objects: Query<'w, 's, (), With<PlacedObjectDefinitionReference>>,
    paths: Query<'w, 's, (), With<PathTile>>,
    photos: Query<'w, 's, Has<CameraRollPhoto>, With<Photo>>,
    immersive_modes: Query<
        'w,
        's,
        (),
        With<crate::plugins::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode>,
    >,
    zoo_cameras: Query<'w, 's, (&'static GlobalTransform, &'static Camera), With<ZooCamera>>,
}

pub(crate) fn collect_requested_verification_facts(
    mut run: ResMut<VerificationJourneyRun>,
    mut report: ResMut<VerificationJourneyReport>,
    world: VerificationWorldFactQueries,
) {
    if run.fact_collection != VerificationFactCollectionState::Requested {
        return;
    }
    let mut facts = VerificationFacts::new();
    let mut fact = |name: &str, value: f64| {
        facts.insert(name.to_owned(), value);
    };
    let count = |count: usize| f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    let flag = |value: bool| f64::from(u8::from(value));

    fact("phase", f64::from(game_phase_order(*world.phase.get())));
    #[allow(clippy::cast_precision_loss)]
    fact(
        "cash_cents",
        world.cash.as_ref().map_or(0.0, |cash| cash.0 .0 as f64),
    );
    fact("unlimited_cash", flag(world.unlimited_cash.is_some()));
    fact(
        "admissions_open",
        flag(world.admissions.is_some_and(|open| open.0)),
    );
    fact("selected", flag(world.selected.0.is_some()));
    fact(
        "settings_windowed",
        flag(world.display.mode == DisplayMode::Windowed),
    );
    fact(
        "settings_highest_detail",
        flag(*world.graphics == GraphicsSettings::from_source_highest_detail_preset()),
    );
    fact("settings_motd", flag(world.online_messages.enabled));
    fact(
        "settings_draft_matches_accepted",
        flag(
            world.settings_draft.display == *world.display
                && world.settings_draft.graphics == *world.graphics
                && world.settings_draft.message_of_the_day == world.online_messages.enabled,
        ),
    );
    fact(
        "settings_draft_clean",
        flag(!world.settings_draft.dirty_display && !world.settings_draft.dirty_graphics),
    );
    if let Some(simulation) = &world.simulation {
        fact("simulation_paused", flag(simulation.paused));
    }
    fact(
        "construction_tool_active",
        flag(*world.construction_tool != ConstructionTool::Inspect),
    );
    fact("ui_over_ui", flag(world.ui_pointer_capture.over_ui));
    fact(
        "cursor_over_terrain",
        flag(world.cursors.iter().any(|cursor| cursor.over_terrain)),
    );
    fact("placement_previews", count(world.previews.iter().count()));
    fact(
        "placement_visible_previews",
        count(
            world
                .previews
                .iter()
                .filter(|(_, visibility)| **visibility != Visibility::Hidden)
                .count(),
        ),
    );
    fact(
        "placement_valid_previews",
        count(
            world
                .previews
                .iter()
                .filter(|(preview, _)| matches!(preview.validity, PlacementValidity::Valid { .. }))
                .count(),
        ),
    );
    fact(
        "placement_pending_previews",
        count(
            world
                .previews
                .iter()
                .filter(|(preview, _)| matches!(preview.validity, PlacementValidity::Pending))
                .count(),
        ),
    );
    for (preview, _) in &world.previews {
        info!(target: "openzt2_verification", validity = ?preview.validity, "placement preview");
    }
    if let Ok((transform, camera)) = world.zoo_cameras.single() {
        let forward = transform.forward();
        fact("camera_x_m", f64::from(transform.translation().x));
        fact("camera_z_m", f64::from(transform.translation().z));
        fact("camera_height_m", f64::from(transform.translation().y));
        fact(
            "camera_pitch_deg",
            f64::from((-forward.y).asin().to_degrees()),
        );
        // Read the field of view from the clip matrix so custom projections count too.
        let clip_from_view = camera.clip_from_view();
        if clip_from_view.y_axis.y > 0.0 {
            fact(
                "camera_fov_deg",
                f64::from((2.0 * (1.0 / clip_from_view.y_axis.y).atan()).to_degrees()),
            );
        }
    }
    fact(
        "immersive_mode_active",
        flag(!world.immersive_modes.is_empty()),
    );
    fact("fences", count(world.fences.iter().count()));
    fact("habitats", count(world.habitats.iter().count()));
    fact("animals", count(world.animals.iter().count()));
    fact(
        "contained_animals",
        count(
            world
                .animals
                .iter()
                .filter(|(containment, _)| containment.is_some_and(|value| value.is_contained))
                .count(),
        ),
    );
    for (name, sex) in [
        ("female_animals", openzt2_game_data::species::Sex::Female),
        ("male_animals", openzt2_game_data::species::Sex::Male),
    ] {
        fact(
            name,
            count(
                world
                    .animals
                    .iter()
                    .filter(|(_, animal_sex)| animal_sex.is_some_and(|value| value.0 == sex))
                    .count(),
            ),
        );
    }
    fact("staff", count(world.staff.iter().count()));
    fact("facilities", count(world.facilities.iter().count()));
    fact("guests", count(world.guests.iter().count()));
    fact("placed_objects", count(world.placed_objects.iter().count()));
    fact("paths", count(world.paths.iter().count()));
    fact("photos", count(world.photos.iter().count()));
    fact(
        "camera_roll_photos",
        count(world.photos.iter().filter(|in_roll| *in_roll).count()),
    );
    facts.extend(report.frame_time_facts.clone());
    report.current_facts = facts;
    run.fact_collection = VerificationFactCollectionState::Collected;
}
