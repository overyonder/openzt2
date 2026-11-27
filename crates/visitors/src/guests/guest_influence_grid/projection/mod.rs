use bevy::{ecs::entity::EntityHashSet, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::world_definitions::{
    interaction_policies::InfluenceSource,
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::{
    authored_float_attributes::AuthoredFloatAttributes,
    guests::guest_simulation_types::{Guest, GuestArchetype},
    locomotion::locomotion_types::SpatialGrid,
};

use super::{
    super::guest_influence_watcher::{
        GuestInfluenceEntry, GuestInfluenceRange, GuestInfluenceWatchRow, GuestInfluenceWatcher,
        GuestInfluenceWatcherOf,
    },
    DirtyGuestInfluenceRegion, GuestInfluenceEmission, GuestInfluenceEmitter,
    GuestInfluenceRegionWork, GuestInfluenceUpdatePolicy, GuestTerrainInfluence,
    InfluenceRadiusDropoff, UpdateGuestInfluenceCells,
};

#[derive(Component, Default)]
pub(in crate::guests) struct GuestInfluenceBindings(HashMap<AssetId, Entity>);

pub(in crate::guests) fn bind_guest_influence(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut roots: Query<
        (
            Entity,
            &mut GuestInfluenceBindings,
            Option<&mut GuestInfluenceUpdatePolicy>,
            Option<&mut GuestTerrainInfluence>,
        ),
        With<SpatialGrid>,
    >,
    mut guests: Query<
        (
            Entity,
            Ref<GuestArchetype>,
            Ref<WorldMember>,
            Option<&GuestInfluenceWatcherOf>,
        ),
        With<Guest>,
    >,
    mut watchers: Query<&mut GuestInfluenceWatcher>,
    mut commands: Commands,
) {
    let refresh = active.is_changed() || assets.is_changed();
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    let Some(policy) = definitions.guest_influence_policy() else {
        return;
    };
    let new_policy = GuestInfluenceUpdatePolicy {
        delay_seconds: policy.update_delay_seconds,
        cells_per_update: policy.cells_per_update,
    };
    let terrain = || {
        GuestTerrainInfluence(
            policy
                .sources
                .iter()
                .filter_map(|source| {
                    let emission = emitter(source, None)?;
                    super::emission_at(emission, 0.0)
                })
                .collect(),
        )
    };
    for (root, _, current, current_terrain) in &mut roots {
        if !refresh && current.is_some() && current_terrain.is_some() {
            continue;
        }
        if let Some(mut current) = current {
            current.set_if_neq(new_policy);
        } else {
            commands.entity(root).insert(new_policy);
        }
        if let Some(mut current) = current_terrain {
            current.set_if_neq(terrain());
        } else {
            commands.entity(root).insert(terrain());
        }
    }
    let mut refreshed = EntityHashSet::default();
    for (guest, archetype, member, watcher) in &mut guests {
        if !refresh && !archetype.is_changed() && !member.is_changed() && watcher.is_some() {
            continue;
        }
        let Some(authored) = definitions
            .guest_influence_watches(archetype.0)
            .filter(|rows| !rows.is_empty())
        else {
            if watcher.is_some() {
                commands.entity(guest).remove::<GuestInfluenceWatcherOf>();
            }
            continue;
        };
        let Ok((root, mut bindings, ..)) = roots.get_mut(member.root) else {
            continue;
        };
        let create_rows = |current: Option<&GuestInfluenceWatcher>| {
            GuestInfluenceWatcher(
                authored
                    .iter()
                    .map(|row| GuestInfluenceWatchRow {
                        category: Some(row.category),
                        update_seconds: row.update_seconds,
                        feedback_chance: row.feedback_chance_percent.ceil().clamp(0.0, 100.0)
                            as u16,
                        sensitivities: row.sensitivities.clone(),
                        ranges: row
                            .ranges
                            .iter()
                            .map(|range| GuestInfluenceRange {
                                minimum: range.minimum.unwrap_or(-1_000_000.0),
                                maximum: range.maximum.unwrap_or(1_000_000.0),
                                feedback: range.feedback.clone(),
                                happiness_adjustment_q16: (range.happiness_adjustment * 65_536.0)
                                    .round()
                                    as i32,
                                departure_adjustment_q16: (range.departure_adjustment * 65_536.0)
                                    .round()
                                    as i32,
                            })
                            .collect(),
                        next_due_seconds: current
                            .and_then(|current| {
                                current
                                    .0
                                    .iter()
                                    .find(|old| old.category == Some(row.category))
                            })
                            .map_or(0.0, |existing| existing.next_due_seconds),
                    })
                    .collect(),
            )
        };
        let binding = match bindings.0.get(&archetype.0).copied() {
            Some(binding) => {
                if refresh && refreshed.insert(binding) {
                    if let Ok(mut current) = watchers.get_mut(binding) {
                        *current = create_rows(Some(&current));
                    }
                }
                binding
            }
            None => {
                let binding = commands.spawn((ChildOf(root), create_rows(None))).id();
                bindings.0.insert(archetype.0, binding);
                binding
            }
        };
        if watcher.is_none_or(|watcher| watcher.0 != binding) {
            commands
                .entity(guest)
                .insert(GuestInfluenceWatcherOf(binding));
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(in crate::guests) struct GuestInfluenceExtent {
    root: Entity,
    bounds: IRect,
}

pub(in crate::guests) fn retire_guest_influence_extent(
    event: On<Remove<GuestInfluenceExtent>>,
    extents: Query<&GuestInfluenceExtent, Allow<bevy::ecs::entity_disabling::Disabled>>,
    mut dirty: MessageWriter<DirtyGuestInfluenceRegion>,
) {
    if let Ok(extent) = extents.get(event.entity) {
        dirty.write(DirtyGuestInfluenceRegion {
            root: extent.root,
            bounds: extent.bounds,
        });
    }
}

pub(in crate::guests) fn disable_guest_influence_extent(
    event: On<Add<bevy::ecs::entity_disabling::Disabled>>,
    extents: Query<
        (),
        (
            With<GuestInfluenceExtent>,
            Allow<bevy::ecs::entity_disabling::Disabled>,
        ),
    >,
    mut commands: Commands,
) {
    if extents.contains(event.entity) {
        commands
            .entity(event.entity)
            .remove::<GuestInfluenceExtent>();
    }
}

fn emitter(
    row: &InfluenceSource,
    attributes: Option<&AuthoredFloatAttributes>,
) -> Option<GuestInfluenceEmission> {
    let intensity = row.strength
        * row
            .scaling_attribute
            .and_then(|key| attributes.and_then(|attributes| attributes.get(key)))
            .unwrap_or(1.0);
    let dropoff = match row.radius_dropoff {
        key if key == AssetId::from_key("linear") => InfluenceRadiusDropoff::Linear,
        key if key == AssetId::from_key("logarithmic") => InfluenceRadiusDropoff::Logarithmic,
        key if key == AssetId::from_key("quadratic") => InfluenceRadiusDropoff::Quadratic,
        _ => InfluenceRadiusDropoff::Constant,
    };
    Some(GuestInfluenceEmission {
        value: GuestInfluenceEntry {
            category: row.category,
            key: row.key?,
            intensity,
        },
        radius: row.radius_m,
        dropoff,
    })
}

pub(in crate::guests) fn project_guest_influence_emitters(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    grids: Query<Ref<SpatialGrid>>,
    mut objects: Query<(
        Entity,
        Ref<DefinitionId>,
        Ref<WorldMember>,
        Ref<GlobalTransform>,
        Option<Ref<AuthoredFloatAttributes>>,
        Option<&mut GuestInfluenceEmitter>,
        Option<&mut GuestInfluenceExtent>,
    )>,
    mut lost_definition: RemovedComponents<DefinitionId>,
    mut lost_world: RemovedComponents<WorldMember>,
    mut dirty: MessageWriter<DirtyGuestInfluenceRegion>,
    mut commands: Commands,
) {
    let refresh = active.is_changed() || assets.is_changed();
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    for entity in lost_definition.read().chain(lost_world.read()) {
        if !objects.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<(GuestInfluenceExtent, GuestInfluenceEmitter)>();
        }
    }
    for (entity, definition, member, pose, attributes, current, extent) in &mut objects {
        let Ok(grid) = grids.get(member.root) else {
            continue;
        };
        if !refresh
            && !definition.is_changed()
            && !member.is_changed()
            && !pose.is_changed()
            && !grid.is_changed()
            && !attributes.as_ref().is_some_and(DetectChanges::is_changed)
            && (current.is_none() || extent.is_some())
        {
            continue;
        }
        let next: Vec<_> = definitions
            .object_influences(definition.0)
            .into_iter()
            .flatten()
            .filter_map(|row| emitter(row, attributes.as_deref()))
            .collect();
        let radius = next.iter().map(|row| row.radius).fold(0.0, f32::max);
        if radius <= 0.0 || grid.cell_size_m <= 0.0 {
            if current.is_some() || extent.is_some() {
                commands
                    .entity(entity)
                    .remove::<(GuestInfluenceExtent, GuestInfluenceEmitter)>();
            }
            continue;
        }
        let centre = (pose.translation().xz() - grid.origin) / grid.cell_size_m;
        let radius = radius / grid.cell_size_m;
        let bounds = IRect::from_corners(
            (centre - Vec2::splat(radius)).floor().as_ivec2(),
            (centre + Vec2::splat(radius)).floor().as_ivec2(),
        );
        if let Some(mut current) = current {
            current.set_if_neq(GuestInfluenceEmitter(next));
        } else {
            commands.entity(entity).insert(GuestInfluenceEmitter(next));
        }
        let next = GuestInfluenceExtent {
            root: member.root,
            bounds,
        };
        if let Some(mut extent) = extent {
            if *extent != next {
                dirty.write(DirtyGuestInfluenceRegion {
                    root: extent.root,
                    bounds: extent.bounds,
                });
                *extent = next;
            }
        } else {
            commands.entity(entity).insert(next);
        }
        dirty.write(DirtyGuestInfluenceRegion {
            root: member.root,
            bounds,
        });
    }
}

pub(in crate::guests) fn advance_guest_influence_regions(
    time: Res<Time<Fixed>>,
    roots: Query<(Entity, &GuestInfluenceRegionWork)>,
    mut updates: MessageWriter<UpdateGuestInfluenceCells>,
) {
    for (root, work) in &roots {
        if work.pending.is_some() || work.active.is_some() {
            updates.write(UpdateGuestInfluenceCells {
                root,
                delta_seconds: time.delta_secs(),
            });
        }
    }
}
