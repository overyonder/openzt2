use bevy::prelude::*;
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId,
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{DefinitionId, WorldMember, WorldRoot},
};
use openzt2_zoo_state::entity_naming_types::{EntityNameRegistered, EntityNameTagCounters};

pub(super) fn initialize_world_entity_name_counters(
    mut commands: Commands,
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            Without<EntityNameTagCounters>,
        ),
    >,
) {
    for root in &roots {
        commands
            .entity(root)
            .insert(EntityNameTagCounters::default());
    }
}

#[cfg(test)]
mod tests;

/// Requests the same generated name used when this entity first enters the zoo.
#[derive(EntityEvent)]
pub struct GenerateWorldEntityName {
    pub entity: Entity,
}

#[derive(Component)]
pub(super) struct PendingWorldEntityName;

#[derive(bevy::ecs::system::SystemParam)]
pub struct WorldEntityNaming<'w, 's> {
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active: Res<'w, WorldDefinitions>,
    localization: Res<'w, LocalizationPrecedenceIndex>,
    localization_assets: Res<'w, Assets<LocalizationAsset>>,
    server: Option<Res<'w, AssetServer>>,
    roots: Query<
        'w,
        's,
        &'static mut EntityNameTagCounters,
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    entities: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            &'static WorldMember,
            &'static DefinitionId,
            Option<&'static Name>,
        ),
        Without<EntityNameRegistered>,
    >,
}

pub(super) fn register_and_name_spawned_world_entities(
    mut commands: Commands,
    naming: WorldEntityNaming,
) {
    if !naming.catalogues_ready() {
        return;
    }
    let mut pending = naming.entities.iter().collect::<Vec<_>>();
    pending.sort_unstable_by_key(|(_, id, member, _, _)| (member.root, id.0));
    for (entity, _, member, _, _) in pending {
        if naming.roots.contains(member.root) {
            commands
                .entity(entity)
                .try_insert_if_new(PendingWorldEntityName);
        }
    }
}

pub(super) fn name_requested_world_entity(
    request: On<GenerateWorldEntityName>,
    mut commands: Commands,
    naming: WorldEntityNaming,
) {
    if naming.entities.contains(request.entity) && naming.catalogues_ready() {
        commands
            .entity(request.entity)
            .try_insert_if_new(PendingWorldEntityName);
    }
}

pub(super) fn assign_pending_world_entity_name(
    added: On<Add<PendingWorldEntityName>>,
    mut commands: Commands,
    mut naming: WorldEntityNaming,
) {
    let mut target = commands.entity(added.entity);
    naming.assign(added.entity, &mut target);
    target.try_remove::<PendingWorldEntityName>();
}

impl WorldEntityNaming<'_, '_> {
    fn catalogues_ready(&self) -> bool {
        self.active
            .get(&self.definitions)
            .is_some_and(|definitions| {
                !definitions.has_pending_assets()
                    && !self.server.as_ref().is_some_and(|server| {
                        self.localization
                            .has_pending_catalogue_loads(server, &self.localization_assets)
                    })
            })
    }

    fn assign(&mut self, entity: Entity, target: &mut EntityCommands<'_>) {
        let Ok((_, _, member, definition, name)) = self.entities.get(entity) else {
            return;
        };
        let has_name = name.is_some();
        let (member, definition) = (*member, *definition);
        let _ = self.name_new_entity(target, member, definition, has_name);
    }

    /// Registers a newly created world member and returns its generated name immediately.
    ///
    /// Call once when assembling the entity. The registration component prevents
    /// the ordinary discovery pass from counting the same entity again.
    pub fn name_new_entity(
        &mut self,
        target: &mut EntityCommands<'_>,
        member: WorldMember,
        definition: DefinitionId,
        has_name: bool,
    ) -> Option<Name> {
        if !self.catalogues_ready() {
            return None;
        }
        let definitions = self.active.get(&self.definitions)?;
        let naming = definitions.find_entity_naming(definition.0)?;
        let localization = self
            .localization
            .borrow_loaded_localization_view(&self.localization_assets)?;
        let mut counters = self.roots.get_mut(member.root).ok()?;
        counters.record_entity_type_tags(&naming.counted_tags);
        target.try_insert_if_new(EntityNameRegistered);
        if has_name {
            return None;
        }
        let generated_name = naming.generated_name?;
        let label = localization
            .find_plain_localized_text(generated_name.localization_key)
            .filter(|label| !label.is_empty())?;
        let number = counters.value(generated_name.counter_tag).unwrap_or(0);
        let text = match i32::try_from(number) {
            Ok(number) if number > 0 => format!("{label} {number}"),
            _ => label.to_owned(),
        };
        let name = Name::new(text);
        target.try_insert_if_new(name.clone());
        Some(name)
    }
}
