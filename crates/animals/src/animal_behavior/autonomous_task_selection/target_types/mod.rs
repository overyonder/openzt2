use bevy::{
    ecs::{entity::EntityLocation, relationship::RelationshipTarget, system::SystemParam},
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{AssetId, behavior::eligibility::{BehaviorCandidateEligibilityRequirements, BehaviorEligibilityFactJunction}};
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, AnimalVariant};

use super::candidate_facts::is_live_animal_state_type;

#[derive(Component)]
#[relationship(relationship_target = TargetTypeLinks)]
struct TypedBehaviorTarget(Entity);

#[derive(Component)]
#[component(storage = "SparseSet")]
#[relationship_target(relationship = TypedBehaviorTarget, linked_spawn)]
struct TargetTypeLinks(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = BehaviorTargetsOfType)]
struct BehaviorTargetType(Entity);

#[derive(Component)]
#[relationship_target(relationship = BehaviorTargetType)]
struct BehaviorTargetsOfType(Vec<Entity>);

#[derive(Resource, Default)]
pub(super) struct AuthoredTargetTypes {
    roots: HashMap<AssetId, Entity>,
    objects: HashMap<AssetId, Box<[AssetId]>>,
    variants: HashMap<AssetId, Box<[AssetId]>>,
    fences: HashSet<AssetId>,
}

#[derive(Component)]
pub(super) struct RefreshTargetTypeLinks;

pub(super) fn refresh_target_type_links_after_marker_removal(
    removed: On<Remove<(Animal, DefinitionId, AnimalVariant)>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_insert(RefreshTargetTypeLinks);
}

pub(super) type LiveBehaviorTarget<'w> = (
    Entity,
    Option<&'w DefinitionId>,
    Option<&'w AnimalVariant>,
    Has<Animal>,
    Has<RefreshTargetTypeLinks>,
);

pub(super) fn project_live_behavior_target_types(
    definitions: Res<WorldDefinitions>,
    object_assets: Res<Assets<WorldDefinitionAsset>>,
    species: Res<SpeciesAssets>,
    species_assets: Res<Assets<SpeciesAsset>>,
    mut object_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut species_events: MessageReader<AssetEvent<SpeciesAsset>>,
    mut types: ResMut<AuthoredTargetTypes>,
    objects: Query<
        LiveBehaviorTarget<'static>,
        Or<(
            With<DefinitionId>,
            With<AnimalVariant>,
            With<RefreshTargetTypeLinks>,
        )>,
    >,
    changed: Query<
        LiveBehaviorTarget<'static>,
        Or<(
            Changed<DefinitionId>,
            Changed<AnimalVariant>,
            Added<Animal>,
            With<RefreshTargetTypeLinks>,
        )>,
    >,
    mut commands: Commands,
) {
    let object_assets_changed = object_events.read().any(did_change_asset);
    let species_assets_changed = species_events.read().any(did_change_asset);
    object_events.clear();
    species_events.clear();
    let new_definitions = definitions.is_changed()
        || object_assets_changed
        || species.is_changed()
        || species_assets_changed;
    if new_definitions {
        types.objects.clear();
        types.variants.clear();
        types.fences.clear();
        if let Some(definitions) = definitions.get(&object_assets) {
            for object in definitions.objects() {
                let mut tags = object.behavior_type_identifiers.clone();
                tags.push(object.id);
                tags.sort_unstable();
                tags.dedup();
                types.objects.insert(object.id, tags.into_boxed_slice());
            }
            for fence in definitions.fences() {
                let types = &mut *types;
                if let Some(tags) = types.objects.get(&fence.id) {
                    types.fences.extend(tags.iter().copied());
                }
            }
        }
        if let Some(species) = species.get(&species_assets) {
            for owner in species.species() {
                for variant in species.variants(owner.id) {
                    let mut tags = variant.behavior_subject_type_identifiers.clone();
                    tags.extend([
                        owner.id,
                        variant.id,
                        AssetId::from_key("animal"),
                        AssetId::from_key("b_animal"),
                    ]);
                    tags.sort_unstable();
                    tags.dedup();
                    types.variants.insert(variant.id, tags.into_boxed_slice());
                }
            }
        }
    }
    let affected = new_definitions
        .then(|| objects.iter())
        .into_iter()
        .flatten()
        .chain(
            (!new_definitions)
                .then(|| changed.iter())
                .into_iter()
                .flatten(),
        );
    for (entity, definition, variant, animal, refresh) in affected {
        commands.entity(entity).despawn_related::<TargetTypeLinks>();
        if refresh {
            commands.entity(entity).remove::<RefreshTargetTypeLinks>();
        }
        let types = &mut *types;
        let tags = if animal {
            variant.and_then(|variant| types.variants.get(&variant.0))
        } else {
            definition.and_then(|definition| types.objects.get(&definition.0))
        };
        let Some(tags) = tags else {
            continue;
        };
        for &tag in tags {
            let root = types
                .roots
                .entry(tag)
                .or_insert_with(|| commands.spawn_empty().id());
            commands.spawn((TypedBehaviorTarget(entity), BehaviorTargetType(*root)));
        }
    }
}

fn did_change_asset<A: Asset>(event: &AssetEvent<A>) -> bool {
    matches!(
        event,
        AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
    )
}

pub(in crate::animal_behavior) fn register_target_type_projection(app: &mut App) {
    use openzt2_game_assets::application_lifecycle::GamePhase;
    use openzt2_world_state::application_schedule::FixedGameSet;

    app.init_resource::<AuthoredTargetTypes>()
        .add_observer(refresh_target_type_links_after_marker_removal)
        .add_systems(
            FixedUpdate,
            project_live_behavior_target_types
                .in_set(FixedGameSet::Clock)
                .run_if(in_state(GamePhase::InGame)),
        );
}

#[derive(SystemParam)]
pub(in crate::animal_behavior) struct BehaviorTargetTypeMemberships<'w, 's> {
    types: Res<'w, AuthoredTargetTypes>,
    groups: Query<'w, 's, &'static BehaviorTargetsOfType>,
    links: Query<'w, 's, &'static TypedBehaviorTarget>,
    locations: Query<'w, 's, EntityLocation>,
}

impl BehaviorTargetTypeMemberships<'_, '_> {
    pub(super) fn includes_fence(
        &self,
        requirement: &BehaviorCandidateEligibilityRequirements,
    ) -> bool {
        requirement
            .candidate_type_identifiers
            .iter()
            .any(|id| self.types.fences.contains(id))
    }

    pub(super) fn candidates(
        &self,
        requirement: Option<&BehaviorCandidateEligibilityRequirements>,
        animals: bool,
        candidates: &mut Vec<Entity>,
    ) {
        candidates.clear();
        let all_animals = AssetId::from_key("animal");
        let junction = requirement.map(|requirement| requirement.candidate_type_junction);
        let type_ids = requirement.map_or(&[][..], |requirement| {
            requirement.candidate_type_identifiers.as_slice()
        });
        let dynamic_type = |required| animals && is_live_animal_state_type(required);
        match junction {
            Some(BehaviorEligibilityFactJunction::All) => {
                if let Some(required) = type_ids
                    .iter()
                    .copied()
                    .find(|required| !dynamic_type(*required))
                {
                    self.extend_type(required, candidates);
                } else if animals {
                    self.extend_type(all_animals, candidates);
                }
            }
            Some(BehaviorEligibilityFactJunction::Any)
                if !type_ids.iter().copied().any(dynamic_type) =>
            {
                for &required in type_ids {
                    self.extend_type(required, candidates);
                }
            }
            _ if animals => self.extend_type(all_animals, candidates),
            _ => {}
        }
        if !animals && junction == Some(BehaviorEligibilityFactJunction::Any) {
            candidates.sort_unstable();
        } else {
            self.sort_by_table_order(candidates);
        }
        candidates.dedup();
    }

    fn extend_type(&self, required: AssetId, candidates: &mut Vec<Entity>) {
        let Some(related) = self
            .types
            .roots
            .get(&required)
            .and_then(|root| self.groups.get(*root).ok())
        else {
            return;
        };
        candidates.extend(
            self.links
                .iter_many(related.iter())
                .flatten()
                .map(|target| target.0),
        );
    }

    pub(super) fn sort_by_table_order(&self, candidates: &mut [Entity]) {
        candidates.sort_unstable_by_key(|entity| {
            self.locations
                .get(*entity)
                .ok()
                .map(|location| (location.table_id.as_u32(), location.table_row.index()))
        });
    }
}
