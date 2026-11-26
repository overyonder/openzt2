use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::{
    asset_source::AssetArchives,
    world_definitions::world_definition_asset_set_refresh_and_indexing::saved_world_economy_asset::SavedWorldEconomyAsset,
};
use openzt2_world_state::{
    game_session_types::WorldSessionMode,
    world_spawn::{
        selected_world_document_handles::SelectedWorldDocumentHandles,
        selected_world_identity::SelectedWorldIdentity,
        world_membership_types::{WorldMember, WorldRoot},
    },
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorEconomyTracker;

use super::behavior_transaction_state::EconomyUsage;
use openzt2_zoo_state::economy::world_economy_configuration::WorldEconomyConfiguration;

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct WorldEconomyTrackerIndex(
    pub(super) std::collections::BTreeMap<openzt2_game_data::AssetId, Entity>,
);

#[derive(Component)]
pub(super) struct AuthoredWorldEconomyTracker;

pub(super) fn load_selected_world_economy(
    roots: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &SelectedWorldDocumentHandles,
        ),
        (With<WorldRoot>, Without<WorldEconomyConfiguration>),
    >,
    server: Res<AssetServer>,
    archives: Res<AssetArchives>,
    mut commands: Commands,
) {
    if roots.is_empty() {
        return;
    }
    for (root, identity, documents) in &roots {
        let authored_path = documents
            .mode
            .as_ref()
            .and_then(Handle::path)
            .or_else(|| documents.start.path())
            .or_else(|| documents.map.path());
        let handle = if let Some(path) = authored_path {
            server.load::<SavedWorldEconomyAsset>(path.without_label().into_owned())
        } else {
            let template = match identity.mode {
                WorldSessionMode::Freeform => "config/freeform_mode.xml",
                WorldSessionMode::Challenge => "config/challenge_mode.xml",
                WorldSessionMode::Campaign => "config/campaign_mode.xml",
            };
            let Some(path) = archives.resolve_source_reference(std::path::Path::new(""), template)
            else {
                continue;
            };
            server.load::<SavedWorldEconomyAsset>(path)
        };
        commands
            .entity(root)
            .insert(WorldEconomyConfiguration(handle));
    }
}

pub(super) fn materialize_selected_world_economy_trackers(
    mut configurations: Query<(
        Entity,
        Ref<WorldEconomyConfiguration>,
        Option<&Children>,
        Option<&mut WorldEconomyTrackerIndex>,
    )>,
    assets: Res<Assets<SavedWorldEconomyAsset>>,
    mut asset_changes: MessageReader<AssetEvent<SavedWorldEconomyAsset>>,
    mut trackers: Query<&mut BehaviorEconomyTracker, With<AuthoredWorldEconomyTracker>>,
    mut commands: Commands,
) {
    let changed = asset_changes
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    for (world, source, children, mut index) in &mut configurations {
        if !source.is_changed() && !changed.contains(&source.0.id()) {
            continue;
        }
        let current = children
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(|entity| {
                trackers
                    .get(entity)
                    .ok()
                    .map(|tracker| (tracker.name, entity))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut remaining = current;
        if let Some(configuration) = assets.get(&source.0) {
            let mut next = WorldEconomyTrackerIndex::default();
            for definition in &configuration.trackers {
                let entity = if let Some(entity) = remaining.remove(&definition.name) {
                    if let Ok(mut tracker) = trackers.get_mut(entity) {
                        if tracker.transactions != definition.transactions {
                            tracker.transactions.clone_from(&definition.transactions);
                        }
                    }
                    entity
                } else {
                    commands
                        .spawn((
                            AuthoredWorldEconomyTracker,
                            BehaviorEconomyTracker {
                                name: definition.name,
                                transactions: definition.transactions.clone(),
                            },
                            EconomyUsage::default(),
                            ChildOf(world),
                            WorldMember { root: world },
                        ))
                        .id()
                };
                next.0.insert(definition.name, entity);
            }
            if let Some(index) = index.as_mut() {
                index.set_if_neq(next);
            } else {
                commands.entity(world).insert(next);
            }
        } else if index.is_some() {
            commands.entity(world).remove::<WorldEconomyTrackerIndex>();
        }
        for entity in remaining.into_values() {
            commands.entity(entity).try_despawn();
        }
    }
}
