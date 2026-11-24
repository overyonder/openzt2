use bevy::prelude::*;
use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::{
    audio::{AudioWaterPolicy, STAGE_INDOOR_OCCLUSION},
    AssetId,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::TankGeometry,
    terrain::{
        terrain_water_presentation_types::{NaturalWaterRegion, TerrainWaterMembershipIndex},
        terrain_water_view_state::TerrainWaterCameraContact,
    },
};

use super::audio_environment_types::AudioStage;
use super::authored_room_stages::ListenerRoomStages;
use super::stage_voice_acoustics::StageAcoustics;

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct ListenerAudioEnvironment {
    pub(super) stage: AssetId,
    pub(super) lapping_gain: f32,
    pub(super) acoustics: StageAcoustics,
    pub(super) room: Option<Entity>,
    pub(super) occlude_crowd: bool,
}

pub(super) fn project_listener_audio_environment(
    mut commands: Commands,
    assets: Res<Assets<AudioAsset>>,
    catalogue: Res<AudioAssets>,
    terrain: Res<Assets<TerrainAsset>>,
    cameras: Query<(Ref<TerrainWaterCameraContact>, Option<Ref<AudioStage>>)>,
    mut worlds: Query<(
        Entity,
        Ref<WorldRoot>,
        Option<Ref<TerrainWaterMembershipIndex>>,
        Option<&mut ListenerAudioEnvironment>,
    )>,
    tanks: Query<&TankGeometry>,
    natural: Query<&NaturalWaterRegion>,
    changed_water: Query<(), Or<(Changed<TankGeometry>, Changed<NaturalWaterRegion>)>>,
    mut removed_terrain: RemovedComponents<TerrainWaterMembershipIndex>,
    mut removed_camera: RemovedComponents<TerrainWaterCameraContact>,
    mut removed_room: RemovedComponents<AudioStage>,
    mut rooms: ListenerRoomStages,
) {
    let Some(view) = catalogue.get(&assets) else {
        return;
    };
    let camera = cameras.single().ok();
    let water_changed = !changed_water.is_empty() | (removed_terrain.read().count() != 0);
    let listener_changed = camera.as_ref().is_some_and(|(contact, room)| {
        contact.is_changed() || room.as_ref().is_some_and(|room| room.is_changed())
    }) | (removed_camera.read().count() != 0)
        | (removed_room.read().count() != 0);
    let default_stage = AssetId::from_key("audio-stage:default");
    let policy = view.water();
    let changed_rooms = rooms.affected_worlds();
    for (entity, root, membership, current) in &mut worlds {
        if !root.is_added()
            && !catalogue.is_changed()
            && !water_changed
            && !listener_changed
            && !changed_rooms.contains(&entity)
            && !rooms.scope_changed(entity)
            && !membership.as_ref().is_some_and(Ref::is_changed)
        {
            continue;
        }
        let wet = camera.as_ref().and_then(|(contact, _)| contact.wet_region);
        let water_stage = policy.and_then(|policy| {
            let (region, _) = wet?;
            let area_band = |area| {
                usize::from(area >= policy.area_thresholds[0])
                    + usize::from(area >= policy.area_thresholds[1])
            };
            if let Ok(tank) = tanks.get(region) {
                policy.tank_stages[area_band(tank.area)]
            } else {
                let region = natural.get(region).ok()?;
                policy.natural_stages[area_band(region.area_m2)]
            }
        });
        let authored_room = camera
            .as_ref()
            .and_then(|(contact, _)| contact.position)
            .and_then(|position| rooms.select(entity, position))
            .filter(|(_, stage, _)| view.stage(*stage).is_some());
        let interior = camera
            .as_ref()
            .and_then(|(_, room)| room.as_ref())
            .map(|room| room.0)
            .or_else(|| authored_room.map(|(_, stage, _)| stage));
        let stage = water_stage.or(interior).unwrap_or(default_stage);
        let lapping_gain = if let Some((policy, contact)) =
            policy.zip(camera.as_ref().map(|(contact, _)| contact))
        {
            if contact.underwater_status == Some(false) && interior.is_none() {
                contact
                    .position
                    .zip(membership.as_deref())
                    .and_then(|(position, membership)| {
                        let terrain = terrain.get(membership.terrain())?;
                        Some(shore_lapping_gain(
                            position, membership, terrain, &natural, policy,
                        ))
                    })
                    .unwrap_or(0.0)
            } else {
                0.0
            }
        } else {
            0.0
        };
        let stage_definition = view.stage(stage);
        let room = if water_stage.is_none()
            && stage_definition.is_some_and(|stage| stage.flags & STAGE_INDOOR_OCCLUSION != 0)
        {
            authored_room.filter(|(_, authored_stage, _)| *authored_stage == stage)
        } else {
            None
        };
        let next = ListenerAudioEnvironment {
            stage,
            lapping_gain,
            acoustics: stage_definition.map_or_else(StageAcoustics::default, StageAcoustics::from),
            room: room.map(|(entity, ..)| entity),
            occlude_crowd: room.is_some_and(|(_, _, occlude)| occlude),
        };
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

fn shore_lapping_gain(
    listener: Vec3,
    membership: &TerrainWaterMembershipIndex,
    terrain: &TerrainAsset,
    natural: &Query<&NaturalWaterRegion>,
    policy: &AudioWaterPolicy,
) -> f32 {
    let range = policy.lapping_max_distance;
    let grid = terrain.canonical_terrain_grid();
    let spacing = grid.units.cell_size_metres;
    if !range.is_finite() || range <= 0.0 || spacing <= 0.0 || !listener.is_finite() {
        return 0.0;
    }
    let source = Vec2::new(listener.x, -listener.z) / spacing;
    let padding = Vec2::splat(range / spacing);
    let first = (source - padding).floor().max(Vec2::ZERO).as_ivec2();
    let maximum = Vec2::new(
        grid.width.saturating_sub(1) as f32,
        grid.height.saturating_sub(1) as f32,
    );
    let last = (source + padding).ceil().min(maximum).as_ivec2();
    let mut nearest = range * range;
    for y in first.y..=last.y {
        for x in first.x..=last.x {
            let point = UVec2::new(x as u32, y as u32);
            let Some(owner) = membership.point_owner(point) else {
                continue;
            };
            if !natural
                .get(owner)
                .is_ok_and(|region| region.area_m2 >= policy.lapping_minimum_area)
            {
                continue;
            }
            let shoreline = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y]
                .into_iter()
                .any(|direction| {
                    let neighbor = point.as_ivec2() + direction;
                    neighbor.min_element() < 0
                        || membership.point_owner(neighbor.as_uvec2()) != Some(owner)
                });
            if shoreline {
                nearest =
                    nearest.min((point.as_vec2() - source).length_squared() * spacing * spacing);
            }
        }
    }
    (1.0 - nearest.sqrt() / range).powi(2)
}
