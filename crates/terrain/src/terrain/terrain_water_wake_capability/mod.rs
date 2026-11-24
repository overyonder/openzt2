use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_zoo_state::terrain::{
    terrain_water_geometric_wave_shader_state::{
        NativeTerrainWaterGeometricWaveShaderState, WaterWakePresentationPath,
    },
    terrain_water_presentation_types::{
        TerrainWaterRegionEdit, TerrainWaterRegionQuery, TerrainWaterRegionRetired,
        TerrainWaterRegionSource,
    },
    terrain_water_renderer_types::{
        TerrainWaterRegionDraws, TerrainWaterSurfacePassKind, TerrainWaterSurfaceRegion,
        TerrainWaterWaveStateKey,
    },
};

#[derive(Message)]
pub(super) struct WaterWakeCapabilityInvalidated(Entity);

pub(super) fn invalidate_discarded_water_surface_group(
    discarded: On<Discard<TerrainWaterSurfaceRegion>>,
    groups: Query<&TerrainWaterSurfaceRegion>,
    mut invalidated: MessageWriter<WaterWakeCapabilityInvalidated>,
) {
    if let Ok(group) = groups.get(discarded.entity) {
        invalidated.write(WaterWakeCapabilityInvalidated(group.0));
    }
}

pub(super) fn invalidate_discarded_water_surface_pass(
    discarded: On<Discard<(TerrainWaterSurfacePassKind, ChildOf)>>,
    parents: Query<&ChildOf, With<TerrainWaterSurfacePassKind>>,
    groups: Query<&TerrainWaterSurfaceRegion>,
    mut invalidated: MessageWriter<WaterWakeCapabilityInvalidated>,
) {
    if let Ok(group) = parents
        .get(discarded.entity)
        .and_then(|parent| groups.get(parent.parent()))
    {
        invalidated.write(WaterWakeCapabilityInvalidated(group.0));
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn reconcile_affected_water_wake_capabilities(
    mut commands: Commands,
    mut invalidated: MessageReader<WaterWakeCapabilityInvalidated>,
    mut material_events: MessageReader<AssetEvent<EffectPassMaterial>>,
    sources: TerrainWaterRegionQuery,
    mut regions: Query<(
        Option<&TerrainWaterRegionDraws>,
        Option<&mut WaterWakePresentationPath>,
    )>,
    changed_regions: Query<
        Entity,
        Or<(
            Changed<TerrainWaterRegionSource>,
            Changed<TerrainWaterRegionEdit>,
            Added<TerrainWaterRegionRetired>,
            Added<NativeTerrainWaterGeometricWaveShaderState>,
        )>,
    >,
    (mut removed_waves, mut restored, mut removed_sources, mut removed_edits): (
        RemovedComponents<NativeTerrainWaterGeometricWaveShaderState>,
        RemovedComponents<TerrainWaterRegionRetired>,
        RemovedComponents<TerrainWaterRegionSource>,
        RemovedComponents<TerrainWaterRegionEdit>,
    ),
    changed_groups: Query<&TerrainWaterSurfaceRegion, Changed<TerrainWaterSurfaceRegion>>,
    changed_passes: Query<
        &ChildOf,
        (
            With<TerrainWaterSurfacePassKind>,
            Or<(
                Changed<ChildOf>,
                Changed<TerrainWaterSurfacePassKind>,
                Changed<MeshMaterial3d<EffectPassMaterial>>,
                Changed<TerrainWaterWaveStateKey>,
            )>,
        ),
    >,
    groups: Query<(&TerrainWaterSurfaceRegion, Option<&Children>)>,
    passes: Query<(
        &ChildOf,
        &TerrainWaterSurfacePassKind,
        &TerrainWaterWaveStateKey,
        &MeshMaterial3d<EffectPassMaterial>,
    )>,
    waves: Query<(), With<NativeTerrainWaterGeometricWaveShaderState>>,
    materials: Res<Assets<EffectPassMaterial>>,
) {
    let mut affected: EntityHashSet = changed_regions
        .iter()
        .chain(changed_groups.iter().map(|group| group.0))
        .chain(invalidated.read().map(|change| change.0))
        .chain(removed_waves.read())
        .chain(restored.read())
        .chain(removed_sources.read())
        .chain(removed_edits.read())
        .collect();
    affected.extend(
        changed_passes
            .iter()
            .filter_map(|parent| groups.get(parent.parent()).ok().map(|(group, _)| group.0)),
    );
    let changed_materials: HashSet<_> = material_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(id),
            _ => None,
        })
        .collect();
    if !changed_materials.is_empty() {
        affected.extend(passes.iter().filter_map(|(parent, _, _, material)| {
            if !changed_materials.contains(&material.id()) {
                return None;
            }
            groups.get(parent.parent()).ok().map(|(group, _)| group.0)
        }));
    }
    for owner in affected {
        let Ok((draws, current)) = regions.get_mut(owner) else {
            continue;
        };
        let next = sources.get(owner).and_then(|_| {
            draws?
                .iter()
                .filter_map(|group| groups.get(group).ok()?.1)
                .flat_map(|children| passes.iter_many(children.iter()).flatten())
                .filter_map(|(_, kind, key, material)| {
                    let material = materials.get(material)?;
                    waves.get(key.0).ok()?;
                    if kind.uses_stitch_geometry() {
                        Some(WaterWakePresentationPath::TerrainStitch)
                    } else {
                        ["Frequency", "Phase", "Amplitude", "PosX", "PosY", "K"]
                            .iter()
                            .all(|name| material.has_vertex_float_effect_semantic(name))
                            .then_some(WaterWakePresentationPath::GeometricWave)
                    }
                })
                .last()
        });
        match (current, next) {
            (Some(mut current), Some(next)) => {
                current.set_if_neq(next);
            }
            (None, Some(next)) => {
                commands.entity(owner).insert(next);
            }
            (Some(_), None) => {
                commands.entity(owner).remove::<WaterWakePresentationPath>();
            }
            (None, None) => {}
        }
    }
}
