use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_data::ui_document::node_property_binding::UiBooleanPropertyBindingSource;

use crate::plugins::{
    animal_lifecycle::types::{Animal, AnimalSex},
    camera::camera_runtime_state_types::{OverheadRig, ZooCamera},
    construction::{
        construction_interaction_types::{
            ConstructionCursor, ConstructionPreview, PlacementValidity,
        },
        construction_tool_and_placement_policy_types::ConstructionTool,
    },
    economy::{
        facility_economy_types::{CurrentSellQuote, ServiceFacility},
        guest_admission_types::ZooAdmissionsOpen,
        zoo_cash_types::{UnlimitedZooCash, ZooCash},
    },
    guests::guest_simulation_types::Guest,
    habitat::habitat_types::{Containment, Habitat},
    immersive_modes::{
        immersive_mode_message_types::{ImmersiveModeRejected, ModeEntryFailure},
        immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode},
    },
    information::entity_selection_types::SelectedEntity,
    photos::{photo_album_types::CameraRollPhoto, photo_capture_types::Photo},
    placement::placed_object_types::PlacedObjectDefinitionReference,
    settings::{
        display_settings_types::{DisplayMode, DisplaySettings},
        online_message_policy_types::OnlineMessagePolicy,
        options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
    },
    shell::shell_selection_types::ShellSelection,
    simulation_time::simulation_control_types::SimulationControl,
    staff::staff_employment_types::Employment,
    topology::topology_graph_types::{FenceEdge, PathTile},
    ui::{
        authored_ui_selection_binding::UiSelectedBinding, authored_ui_selection_state::UiSelected,
        picking::UiPointerCapture,
    },
    world_spawn::{
        selected_world_identity::SelectedWorldIdentity, world_membership_types::WorldMember,
        world_terrain_hydration::WorldTerrainHorizontalBounds,
    },
};
use openzt2_game_assets::application_lifecycle::GamePhase;

use super::{
    verification_game_phase_readiness::game_phase_order,
    verification_journey_state_types::{
        VerificationFactCollectionState, VerificationFacts, VerificationJourneyReport,
        VerificationJourneySession,
    },
};

#[derive(SystemParam)]
pub(crate) struct VerificationWorldFactQueries<'w, 's> {
    phase: Res<'w, State<GamePhase>>,
    shell_selection: Res<'w, ShellSelection>,
    cash: Option<Res<'w, ZooCash>>,
    unlimited_cash: Option<Res<'w, UnlimitedZooCash>>,
    admissions: Option<Res<'w, ZooAdmissionsOpen>>,
    selected: Res<'w, SelectedEntity>,
    sale_quotes: Query<'w, 's, &'static CurrentSellQuote>,
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
    immersive_modes: Query<'w, 's, &'static ActiveImmersiveMode>,
    mode_buttons: Query<
        'w,
        's,
        (
            &'static UiSelectedBinding,
            &'static UiSelected,
            &'static InheritedVisibility,
        ),
    >,
    zoo_cameras: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            &'static Camera,
            &'static OverheadRig,
            &'static WorldMember,
        ),
        With<ZooCamera>,
    >,
    world_bounds: Query<
        'w,
        's,
        (
            &'static WorldTerrainHorizontalBounds,
            &'static SelectedWorldIdentity,
        ),
    >,
    ui_animations: Query<'w, 's, &'static crate::plugins::ui::animation::UiShowHideAnimation>,
}

pub(crate) fn collect_requested_verification_facts(
    mut session: ResMut<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    world: VerificationWorldFactQueries,
    mut mode_rejections: MessageReader<ImmersiveModeRejected>,
    mut busy_photo_rejections: Local<u32>,
) {
    // Observe the existing rejection message even on frames without a snapshot.
    for rejection in mode_rejections.read() {
        if rejection.mode == ImmersiveMode::Photo && rejection.reason == ModeEntryFailure::Busy {
            *busy_photo_rejections = busy_photo_rejections.saturating_add(1);
        }
    }
    if session.fact_collection != VerificationFactCollectionState::Requested {
        return;
    }
    let mut facts = VerificationFacts::new();
    let mut fact = |name: &str, value: f64| {
        facts.insert(name.to_owned(), value);
    };
    let count = |count: usize| f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    fact("busy_photo_rejections", f64::from(*busy_photo_rejections));
    fact(
        "photo_mode_active",
        count(
            world
                .immersive_modes
                .iter()
                .filter(|active| active.mode == ImmersiveMode::Photo)
                .count(),
        ),
    );
    for (name, source) in [
        (
            "overhead_button_selected",
            UiBooleanPropertyBindingSource::OverheadViewSelected,
        ),
        (
            "first_person_button_selected",
            UiBooleanPropertyBindingSource::FirstPersonViewSelected,
        ),
        (
            "photo_button_selected",
            UiBooleanPropertyBindingSource::PhotoViewSelected,
        ),
    ] {
        fact(
            name,
            count(
                world
                    .mode_buttons
                    .iter()
                    .filter(|(binding, selected, _)| binding.0 == source && selected.0)
                    .count(),
            ),
        );
    }
    fact(
        "photo_button_visible",
        count(
            world
                .mode_buttons
                .iter()
                .filter(|(binding, _, visibility)| {
                    binding.0 == UiBooleanPropertyBindingSource::PhotoViewSelected
                        && visibility.get()
                })
                .count(),
        ),
    );
    let flag = |value: bool| f64::from(u8::from(value));

    fact("phase", f64::from(game_phase_order(*world.phase.get())));
    fact(
        "map_selected",
        flag(world.shell_selection.scenario.is_some()),
    );
    // Authored show and hide animations move and fade controls, so clicks and
    // captures wait for them to finish.
    fact(
        "ui_animations_running",
        count(
            world
                .ui_animations
                .iter()
                .filter(|animation| animation.running)
                .count(),
        ),
    );
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
    // The Sell button quotes the selected object before its confirmation opens.
    let selected_sale_quote_cents = world
        .selected
        .0
        .and_then(|entity| world.sale_quotes.get(entity).ok())
        .map_or(0, |quote| quote.0 .0);
    let cash_cents_after_selling_selected = world
        .cash
        .as_ref()
        .map_or(0, |cash| cash.0 .0)
        .saturating_add(selected_sale_quote_cents);
    #[allow(clippy::cast_precision_loss)]
    fact(
        "selected_sale_quote_cents",
        selected_sale_quote_cents as f64,
    );
    #[allow(clippy::cast_precision_loss)]
    fact(
        "cash_cents_after_selling_selected",
        cash_cents_after_selling_selected as f64,
    );
    fact(
        "settings_windowed",
        flag(world.display.mode == DisplayMode::Windowed),
    );
    fact(
        "settings_highest_detail",
        flag(*world.graphics == GraphicsSettings::from_source_highest_detail_preset()),
    );
    fact(
        "settings_default_detail",
        flag(*world.graphics == GraphicsSettings::default()),
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
    if let Ok((transform, camera, rig, member)) = world.zoo_cameras.single() {
        let forward = transform.forward();
        fact("camera_x_m", f64::from(transform.translation().x));
        fact("camera_z_m", f64::from(transform.translation().z));
        fact("camera_height_m", f64::from(transform.translation().y));
        fact("camera_focus_x_m", f64::from(rig.focus.x));
        fact("camera_focus_z_m", f64::from(rig.focus.y));
        if let Ok((bounds, identity)) = world.world_bounds.get(member.root) {
            fact(
                "world_map_is_small_alpine",
                flag(identity.map == openzt2_game_data::AssetId::from_key("alpinesml")),
            );
            fact("map_min_x_m", f64::from(bounds.min.x));
            fact("map_max_x_m", f64::from(bounds.max.x));
            fact("map_min_z_m", f64::from(bounds.min.y));
            fact("map_max_z_m", f64::from(bounds.max.y));
            let horizontal = transform.translation().xz();
            let edge_distances = (horizontal - bounds.min).min(bounds.max - horizontal);
            fact(
                "camera_map_edge_distance_m",
                f64::from(edge_distances.min_element()),
            );
        }
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
    session.fact_collection = VerificationFactCollectionState::Collected;
}
