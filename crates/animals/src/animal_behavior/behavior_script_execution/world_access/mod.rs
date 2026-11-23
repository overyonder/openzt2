//! World state that authored behavior scripts read and change through
//! `queryObject` managers and entity tables, such as `CompostPoo.lua`.

use bevy::{ecs::{entity::EntityHashMap, relationship::RelationshipTarget, system::SystemParam}, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationRequest;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorKilledTarget;
use openzt2_zoo_state::economy::account_transaction_types::{
    Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
};
use openzt2_zoo_state::economy::authored_transaction_amount::authored_currency_to_money;
use openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorSelfToken;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_zoo_state::animal_lifecycle::types::{AnimalLifeStage, AnimalSex, SpeciesHandle};
use openzt2_zoo_state::animal_behavior::animal_lineage::{AnimalFather, AnimalLineage, AnimalMate, AnimalMother};
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::maintenance::maintenance_types::CompostPooLevel;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

/// Owns a zoo income requested by a behavior script until it settles.
#[derive(Component)]
pub(in crate::animal_behavior) struct BehaviorScriptTransaction;

pub(super) enum BehaviorScriptEffect {
    Presentation(PhysicalPresentationRequest),
    Crate(Entity),
    Float(Entity, ScriptFloatAttribute, f32),
    Income(f64),
    Mate(Entity, Option<Entity>),
    Kinship(Entity, AssetId, Option<Entity>),
    Particle(super::script_particles::ScriptParticle),
    Disease(Entity, DiseaseAttribute),
    BirthMotherName(Option<String>),
    NotifyBirth(Option<openzt2_game_data::species::Sex>),
}

#[derive(Component)]
#[relationship(relationship_target = ScriptKinshipSources)]
struct ScriptKinshipSource(Entity);

#[derive(Component)]
#[relationship_target(relationship = ScriptKinshipSource, linked_spawn)]
struct ScriptKinshipSources(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ScriptKinshipRecipients)]
struct ScriptKinshipRecipient(Entity);

#[derive(Component)]
#[relationship_target(relationship = ScriptKinshipRecipient, linked_spawn)]
struct ScriptKinshipRecipients(Vec<Entity>);

#[derive(Component)]
struct ScriptKinshipRole(AssetId);

#[derive(Component)]
struct BreedingCenterEndangeredBirths(f32);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScriptFloatAttribute {
    CompostPoo,
    EndangeredBirths,
}

impl ScriptFloatAttribute {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        match name {
            "f_PooLevel" => Some(Self::CompostPoo),
            "f_TotalEndangeredBirths" => Some(Self::EndangeredBirths),
            _ => None,
        }
    }
}

/// Structural changes only, visible to later scripts before Commands apply.
#[derive(Default)]
pub(super) struct BehaviorScriptStructuralEdits {
    mates: EntityHashMap<Option<Entity>>,
    diseases: EntityHashMap<bool>,
}

impl BehaviorScriptStructuralEdits {
    pub(super) fn mate(
        &self,
        world: &BehaviorScriptWorldAccess,
        effects: &[BehaviorScriptEffect],
        entity: Entity,
    ) -> Option<Entity> {
        effects
            .iter()
            .rev()
            .find_map(|effect| match *effect {
                BehaviorScriptEffect::Mate(owner, mate) if owner == entity => Some(mate),
                _ => None,
            })
            .or_else(|| self.mates.get(&entity).copied())
            .unwrap_or_else(|| world.mates.get(entity).ok().map(|mate| mate.0))
    }
}

/// An authored disease attribute a script wrote.
pub(super) enum DiseaseAttribute {
    /// `b_Disease`.
    Diseased(bool),
    /// `s_DiseaseName`; `cureAnimal` writes "None".
    Name(String),
}

#[derive(SystemParam)]
pub(in crate::animal_behavior) struct BehaviorScriptWorldAccess<'w, 's> {
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    definitions: Res<'w, WorldDefinitions>,
    objects: Query<
        'w,
        's,
        (
            Entity,
            &'static DefinitionId,
            &'static GlobalTransform,
            &'static WorldMember,
        ),
    >,
    poo_levels: Query<'w, 's, &'static mut CompostPooLevel>,
    endangered_births: Query<'w, 's, &'static mut BreedingCenterEndangeredBirths>,
    killed: Query<'w, 's, &'static BehaviorKilledTarget>,
    mates: Query<'w, 's, &'static AnimalMate>,
    diseases: Query<'w, 's, &'static mut Disease>,
    clock: Single<'w, 's, &'static ZooClock>,
    tokens: Query<'w, 's, &'static BehaviorSelfToken>,
    token_owners: Query<'w, 's, &'static Children>,
    lineage: AnimalLineage<'w, 's>,
    kinship: Query<'w, 's, &'static ScriptKinshipSources>,
    kinship_links: Query<'w, 's, (&'static ScriptKinshipRole, &'static ScriptKinshipRecipient)>,
    life_stages: Query<'w, 's, &'static AnimalLifeStage>,
    animals: Query<
        'w,
        's,
        (
            &'static AnimalSex,
            &'static SpeciesHandle,
            Option<&'static Name>,
        ),
    >,
    species_assets: Res<'w, Assets<SpeciesAsset>>,
    species_index: Res<'w, SpeciesAssets>,
}

impl BehaviorScriptWorldAccess<'_, '_> {
    pub(super) fn float(&self, entity: Entity, attribute: ScriptFloatAttribute) -> f32 {
        match attribute {
            ScriptFloatAttribute::CompostPoo => {
                self.poo_levels.get(entity).map_or(0.0, |value| value.0)
            }
            ScriptFloatAttribute::EndangeredBirths => self
                .endangered_births
                .get(entity)
                .map_or(0.0, |value| value.0),
        }
    }

    pub(super) fn set_existing_float(
        &mut self,
        entity: Entity,
        attribute: ScriptFloatAttribute,
        value: f32,
    ) -> bool {
        match attribute {
            ScriptFloatAttribute::CompostPoo => self
                .poo_levels
                .get_mut(entity)
                .ok()
                .map(|mut current| {
                    current
                        .reborrow()
                        .map_unchanged(|current| &mut current.0)
                        .set_if_neq(value)
                })
                .is_some(),
            ScriptFloatAttribute::EndangeredBirths => self
                .endangered_births
                .get_mut(entity)
                .ok()
                .map(|mut current| {
                    current
                        .reborrow()
                        .map_unchanged(|current| &mut current.0)
                        .set_if_neq(value)
                })
                .is_some(),
        }
    }

    /// `BFG_GET_ENTITIES`: live world objects of one authored binder type.
    pub(super) fn entities_of_type(&self, binder_type: &str) -> Vec<Entity> {
        let definition = AssetId::from_key(&canonicalize_source_document_record_key(binder_type));
        self.objects
            .iter()
            .filter(|(_, candidate, _, _)| candidate.0 == definition)
            .map(|(entity, ..)| entity)
            .collect()
    }

    /// The target this execution killed, which scripts may still read.
    fn killed_target(
        &self,
        actor: Entity,
        execution_id: u64,
        entity: Entity,
    ) -> Option<&BehaviorKilledTarget> {
        self.killed
            .get(actor)
            .ok()
            .filter(|killed| killed.execution_id == execution_id && killed.entity == entity)
    }

    pub(super) fn can_resolve(&self, actor: Entity, execution_id: u64, entity: Entity) -> bool {
        self.objects.contains(entity) || self.killed_target(actor, execution_id, entity).is_some()
    }

    pub(super) fn position(
        &self,
        actor: Entity,
        execution_id: u64,
        entity: Entity,
    ) -> Option<Vec3> {
        self.objects
            .get(entity)
            .map(|(_, _, transform, _)| transform.translation())
            .ok()
            .or_else(|| {
                self.killed_target(actor, execution_id, entity)
                    .map(|killed| killed.translation)
            })
    }

    pub(super) fn binder_type(
        &self,
        actor: Entity,
        execution_id: u64,
        entity: Entity,
    ) -> Option<String> {
        let definition = self
            .objects
            .get(entity)
            .map(|(_, definition, _, _)| definition.0)
            .ok()
            .or_else(|| {
                self.killed_target(actor, execution_id, entity)
                    .map(|killed| killed.definition)
            })?;
        self.definitions
            .get(&self.definition_assets)?
            .object_binder_type(definition)
            .map(str::to_owned)
    }

    /// `BFAI_HAS_TOKEN`: whether the entity holds a live token of this name.
    pub(super) fn has_token(&self, entity: Entity, name: &str) -> bool {
        let name = AssetId::from_key(&name.to_ascii_lowercase());
        self.tokens
            .iter_many(
                self.token_owners
                    .get(entity)
                    .ok()
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .flatten()
            .any(|token| token.name == name && self.clock.tick < token.expires_at)
    }

    /// `BFAI_GET_RELATED_ENTITIES` `child`: the animals whose mother or father
    /// is this entity.
    pub(super) fn related(
        &self,
        entity: Entity,
        role: AssetId,
        pending: &[BehaviorScriptEffect],
    ) -> Vec<Entity> {
        let mut relatives = if role == AssetId::from_key("child") {
            self.lineage.children(entity)
        } else if role == AssetId::from_key("mother") {
            self.lineage
                .parents(entity)
                .mother
                .into_iter()
                .collect::<Vec<_>>()
        } else if role == AssetId::from_key("father") {
            self.lineage
                .parents(entity)
                .father
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for (_, target) in self.kinship_edges(entity, role) {
            if !relatives.contains(&target) {
                relatives.push(target);
            }
        }
        for effect in pending {
            if let BehaviorScriptEffect::Kinship(owner, relation, recipient) = *effect {
                if owner != entity || relation != role {
                    continue;
                }
                if recipient.is_none() || is_parent_role(role) {
                    relatives.clear();
                }
                if let Some(target) = recipient.filter(|target| !relatives.contains(target)) {
                    relatives.push(target);
                }
            }
        }
        relatives
    }

    fn kinship_edges(
        &self,
        entity: Entity,
        role: AssetId,
    ) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        self.kinship
            .get(entity)
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(move |link| {
                let (kind, target) = self.kinship_links.get(link).ok()?;
                (kind.0 == role).then_some((link, target.0))
            })
    }

    /// `BFG_GET_ATTR_BOOLEAN` `b_Male`.
    pub(super) fn is_male(&self, entity: Entity) -> Option<bool> {
        self.animals
            .get(entity)
            .ok()
            .map(|animal| animal.0.0 == openzt2_game_data::species::Sex::Male)
    }

    /// `BFG_GET_ATTR_STRING` `s_Endangerment` and `s_name` of an animal.
    pub(super) fn animal_string(&self, entity: Entity, name: &str) -> Option<String> {
        use openzt2_game_data::species::ConservationStatus;
        let (_, species, animal_name) = self.animals.get(entity).ok()?;
        match name {
            "s_name" => Some(animal_name.map_or_else(String::new, |name| name.as_str().to_owned())),
            "s_Endangerment" => {
                let status = self
                    .species_index
                    .get(&self.species_assets)?
                    .find(species.species)?
                    .conservation;
                Some(
                    match status {
                        ConservationStatus::Unspecified => "",
                        ConservationStatus::LowRisk => "LowRisk",
                        ConservationStatus::Vulnerable => "Vulnerable",
                        ConservationStatus::Endangered => "Endangered",
                        ConservationStatus::Critical => "Critical",
                        ConservationStatus::Extinct => "Extinct",
                    }
                    .to_owned(),
                )
            }
            _ => None,
        }
    }

    /// `BFG_GET_ATTR_BOOLEAN` `b_Adult`.
    pub(super) fn is_adult(&self, entity: Entity) -> Option<bool> {
        self.life_stages
            .get(entity)
            .ok()
            .map(|stage| stage.0 == openzt2_game_data::species::LifeStage::Adult)
    }

    pub(super) fn apply(
        &mut self,
        commands: &mut Commands,
        actor: Entity,
        effects: Vec<BehaviorScriptEffect>,
        pending: &mut BehaviorScriptStructuralEdits,
    ) {
        let mut birth_mother_name = None;
        let mut kinship_edits = HashMap::<(Entity, AssetId), Vec<(Entity, Entity)>>::default();
        for effect in effects {
            match effect {
                BehaviorScriptEffect::Presentation(request) => {
                    commands.write_message(request);
                }
                BehaviorScriptEffect::Crate(entity) => {
                    commands.entity(entity).try_insert(WorldEntityIsCrated);
                }
                BehaviorScriptEffect::Particle(particle) => commands.trigger(particle),
                BehaviorScriptEffect::Kinship(entity, role, target) => {
                    if role == AssetId::from_key("mother") {
                        if let Some(target) = target {
                            commands.entity(entity).try_insert(AnimalMother(target));
                        } else {
                            commands.entity(entity).try_remove::<AnimalMother>();
                        }
                    } else if role == AssetId::from_key("father") {
                        if let Some(target) = target {
                            commands.entity(entity).try_insert(AnimalFather(target));
                        } else {
                            commands.entity(entity).try_remove::<AnimalFather>();
                        }
                    } else {
                        let links = kinship_edits.entry((entity, role)).or_insert_with(|| {
                            self.kinship_edges(entity, role).collect::<Vec<_>>()
                        });
                        if let Some(target) = target {
                            if !links.iter().any(|&(_, recipient)| recipient == target) {
                                let link = commands
                                    .spawn((
                                        ScriptKinshipSource(entity),
                                        ScriptKinshipRecipient(target),
                                        ScriptKinshipRole(role),
                                    ))
                                    .id();
                                links.push((link, target));
                            }
                        } else {
                            for (link, _) in links.drain(..) {
                                commands.entity(link).try_despawn();
                            }
                        }
                    }
                }
                BehaviorScriptEffect::Float(entity, attribute, value) => match attribute {
                    ScriptFloatAttribute::CompostPoo => {
                        commands.entity(entity).try_insert(CompostPooLevel(value));
                    }
                    ScriptFloatAttribute::EndangeredBirths => {
                        commands
                            .entity(entity)
                            .try_insert(BreedingCenterEndangeredBirths(value));
                    }
                },
                BehaviorScriptEffect::Mate(entity, mate) => {
                    if let Some(mate) = mate {
                        if !pending.mates.contains_key(&entity)
                            && self
                                .mates
                                .get(entity)
                                .is_ok_and(|current| current.0 == mate)
                        {
                            continue;
                        }
                        commands.entity(entity).try_insert(AnimalMate(mate));
                    } else {
                        commands.entity(entity).try_remove::<AnimalMate>();
                    }
                    pending.mates.insert(entity, mate);
                }
                BehaviorScriptEffect::Disease(entity, attribute) => {
                    self.apply_disease_attribute(
                        commands,
                        entity,
                        attribute,
                        &mut pending.diseases,
                    );
                }
                BehaviorScriptEffect::BirthMotherName(name) => {
                    birth_mother_name = name;
                }
                BehaviorScriptEffect::NotifyBirth(sex) => {
                    commands.write_message(
                        openzt2_zoo_state::animal_behavior::behavior_birth_notice_types::BehaviorBirthNotice {
                            subject: actor,
                            mother_name: birth_mother_name.clone(),
                            sex,
                        },
                    );
                }
                BehaviorScriptEffect::Income(income) => {
                    let Some(amount) = authored_currency_to_money(income) else {
                        warn!(
                            ?actor,
                            income, "behavior script income is not a valid amount"
                        );
                        continue;
                    };
                    let mut operation = commands.spawn(BehaviorScriptTransaction);
                    if let Ok((_, _, _, world)) = self.objects.get(actor) {
                        operation.insert(*world);
                    }
                    let operation = operation.id();
                    // `ZT_GENERIC_TRANSACTION` "destroy" books an object sale.
                    commands.write_message(TransactionRequest {
                        operation,
                        debit: Account::External,
                        credit: Account::Zoo,
                        amount,
                        kind: TransactionKind::Refund,
                        subject: None,
                    });
                }
            }
        }
    }

    fn apply_disease_attribute(
        &mut self,
        commands: &mut Commands,
        entity: Entity,
        attribute: DiseaseAttribute,
        pending: &mut EntityHashMap<bool>,
    ) {
        let present = pending
            .get(&entity)
            .copied()
            .unwrap_or_else(|| self.diseases.contains(entity));
        match attribute {
            DiseaseAttribute::Diseased(true) if !present => {
                commands.entity(entity).try_insert(Disease {
                    definition: AssetId::default(),
                    elapsed_ticks: 0,
                    severity_permille: 0,
                    hint_level: 0,
                });
                pending.insert(entity, true);
            }
            DiseaseAttribute::Diseased(false) if present => {
                commands.entity(entity).try_remove::<Disease>();
                pending.insert(entity, false);
            }
            DiseaseAttribute::Name(name) if present => {
                let definition = if name == "None" {
                    AssetId::default()
                } else {
                    AssetId::from_key(&canonicalize_source_document_record_key(&name))
                };
                if pending.contains_key(&entity) {
                    commands
                        .entity(entity)
                        .entry::<Disease>()
                        .and_modify(move |mut disease| {
                            if disease.definition != definition {
                                disease.definition = definition;
                            }
                        });
                } else if let Ok(mut disease) = self.diseases.get_mut(entity) {
                    if disease.definition != definition {
                        disease.definition = definition;
                    }
                }
            }
            _ => {}
        }
    }
}

fn is_parent_role(role: AssetId) -> bool {
    role == AssetId::from_key("mother") || role == AssetId::from_key("father")
}

pub(in crate::animal_behavior) fn remove_settled_behavior_script_transactions(
    mut commands: Commands,
    operations: Query<(), With<BehaviorScriptTransaction>>,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
) {
    for operation in completed
        .read()
        .map(|transaction| transaction.operation)
        .chain(rejected.read().map(|transaction| transaction.operation))
    {
        if operations.contains(operation) {
            commands.entity(operation).despawn();
        }
    }
}
