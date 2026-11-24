//! World state that authored behavior scripts read and change through
//! `queryObject` managers and entity tables, such as `CompostPoo.lua`.

use bevy::{ecs::{entity::EntityHashMap, query::QueryData, relationship::RelationshipTarget, system::SystemParam}, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{AssetId, species::NeedKind};
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentIdAllocator, prefab_presentation_types::PrefabPresentation,
};
use openzt2_zoo_state::placement::object_placement_definition_queries::object_placement_prefab_identifier;
use openzt2_zoo_state::placement::placement_surface_site::PlacementSiteQueries;
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
use openzt2_zoo_state::animal_welfare::types::{
    BathroomNeed, EnvironmentNeed, ExerciseNeed, HealthNeed, Hunger, HygieneNeed, MAX_NEED_Q16,
    NeedPoints, PrivacyNeed, Q16_ONE, ReproductionNeed, RestNeed, SetAnimalNeed, SocialNeed,
    StimulationNeed, Thirst,
};
use openzt2_zoo_state::maintenance::maintenance_types::CompostPooLevel;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

/// Owns a zoo income requested by a behavior script until it settles.
#[derive(Component)]
pub(in crate::animal_behavior) struct BehaviorScriptTransaction;

pub(super) enum BehaviorScriptEffect {
    Presentation(PhysicalPresentationRequest),
    Crate(Entity),
    Float(Entity, ScriptFloatAttribute, f32),
    Need(SetAnimalNeed),
    Income(f64),
    Mate(Entity, Option<Entity>),
    Kinship(Entity, AssetId, Option<Entity>),
    Token(Entity, AssetId, Option<Entity>),
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
pub struct BreedingCenterEndangeredBirths(pub f32);

pub(super) struct BehaviorScriptPlacement {
    pub(super) kind: String,
    pub(super) position: Vec3,
    pub(super) rotation_degrees: f32,
    pub(super) ignore_land_collision: bool,
    pub(super) ignore_water_adjacent: bool,
}

pub(super) struct ScriptPlacedObject {
    pub(super) entity: Entity,
    pub(super) binder: String,
    pub(super) position: Vec3,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScriptFloatAttribute {
    CompostPoo,
    EndangeredBirths,
    GoodNeedPoints,
    BadNeedPoints,
    AnimalNeed(NeedKind),
    Authored(AssetId),
}

impl ScriptFloatAttribute {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        match name {
            "f_PooLevel" => Some(Self::CompostPoo),
            "f_TotalEndangeredBirths" => Some(Self::EndangeredBirths),
            "f_needPointsGood" => Some(Self::GoodNeedPoints),
            "f_needPointsBad" => Some(Self::BadNeedPoints),
            "hunger" => Some(Self::AnimalNeed(NeedKind::Hunger)),
            "thirst" => Some(Self::AnimalNeed(NeedKind::Thirst)),
            "rest" => Some(Self::AnimalNeed(NeedKind::Rest)),
            "privacy" => Some(Self::AnimalNeed(NeedKind::Privacy)),
            "social" => Some(Self::AnimalNeed(NeedKind::Social)),
            "exercise" => Some(Self::AnimalNeed(NeedKind::Exercise)),
            "stimulation" => Some(Self::AnimalNeed(NeedKind::Stimulation)),
            "environment" => Some(Self::AnimalNeed(NeedKind::Environment)),
            "health" => Some(Self::AnimalNeed(NeedKind::Health)),
            "hygiene" => Some(Self::AnimalNeed(NeedKind::Hygiene)),
            "bathroom" => Some(Self::AnimalNeed(NeedKind::Bathroom)),
            "reproduction" => Some(Self::AnimalNeed(NeedKind::Reproduction)),
            _ => name.starts_with("f_").then(|| Self::Authored(
                AssetId::from_key(&canonicalize_source_document_record_key(name)),
            )),
        }
    }
}

#[derive(Component)]
pub struct ScriptFloatAttributes(pub HashMap<AssetId, f32>);

#[derive(QueryData)]
struct ScriptNeedFacts {
    hunger: Option<&'static Hunger>,
    thirst: Option<&'static Thirst>,
    rest: Option<&'static RestNeed>,
    privacy: Option<&'static PrivacyNeed>,
    social: Option<&'static SocialNeed>,
    exercise: Option<&'static ExerciseNeed>,
    stimulation: Option<&'static StimulationNeed>,
    environment: Option<&'static EnvironmentNeed>,
    health: Option<&'static HealthNeed>,
    hygiene: Option<&'static HygieneNeed>,
    bathroom: Option<&'static BathroomNeed>,
    reproduction: Option<&'static ReproductionNeed>,
}

pub(super) fn need_distress(wellness: i32) -> f32 {
    (MAX_NEED_Q16 - wellness.clamp(0, MAX_NEED_Q16)) as f32 / (10 * Q16_ONE) as f32
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
    localization: Option<Res<'w, LocalizationPrecedenceIndex>>,
    localization_assets: Option<Res<'w, Assets<LocalizationAsset>>>,
    biomes: openzt2_zoo_state::animal_behavior::behavior_candidate_selection::BehaviorTargetBiome<'w, 's>,
    placement: PlacementSiteQueries<'w, 's>,
    ground: openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility<'w, 's>,
    ids: Query<'w, 's, &'static mut PersistentIdAllocator>,
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
    need_points: Query<'w, 's, &'static mut NeedPoints>,
    script_floats: Query<'w, 's, &'static mut ScriptFloatAttributes>,
    needs: Query<'w, 's, ScriptNeedFacts, With<openzt2_zoo_state::animal_lifecycle::types::Animal>>,
    killed: Query<'w, 's, &'static BehaviorKilledTarget>,
    mates: Query<'w, 's, &'static AnimalMate>,
    diseases: Query<'w, 's, &'static mut Disease>,
    clock: Single<'w, 's, &'static ZooClock>,
    tokens: Query<'w, 's, &'static BehaviorSelfToken>,
    token_owners: Query<'w, 's, &'static Children>,
    habitat_members: Query<'w, 's, &'static openzt2_zoo_state::habitat::habitat_types::HabitatMember>,
    habitats: Query<'w, 's, &'static openzt2_zoo_state::habitat::habitat_types::HabitatSummary>,
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
            ScriptFloatAttribute::GoodNeedPoints => self.need_points.get(entity).map_or(0.0, |points| points.good),
            ScriptFloatAttribute::BadNeedPoints => self.need_points.get(entity).map_or(0.0, |points| points.bad),
            ScriptFloatAttribute::AnimalNeed(need) => self.need(entity, need).map_or(0.0, need_distress),
            ScriptFloatAttribute::Authored(attribute) => self.script_floats.get(entity).ok()
                .and_then(|values| values.0.get(&attribute).copied())
                .or_else(|| self.authored_float(entity, attribute)).unwrap_or_default(),
        }
    }

    fn need(&self, entity: Entity, kind: NeedKind) -> Option<i32> {
        let needs = self.needs.get(entity).ok()?;
        match kind {
            NeedKind::Hunger => needs.hunger.map(|value| value.0),
            NeedKind::Thirst => needs.thirst.map(|value| value.0),
            NeedKind::Rest => needs.rest.map(|value| value.0),
            NeedKind::Privacy => needs.privacy.map(|value| value.0),
            NeedKind::Social => needs.social.map(|value| value.0),
            NeedKind::Exercise => needs.exercise.map(|value| value.0),
            NeedKind::Stimulation => needs.stimulation.map(|value| value.0),
            NeedKind::Environment => needs.environment.map(|value| value.0),
            NeedKind::Health => needs.health.map(|value| value.0),
            NeedKind::Hygiene => needs.hygiene.map(|value| value.0),
            NeedKind::Bathroom => needs.bathroom.map(|value| value.0),
            NeedKind::Reproduction => needs.reproduction.map(|value| value.0),
        }
    }

    pub(super) fn request_need(&self, animal: Entity, need: NeedKind, distress: f32) -> Option<SetAnimalNeed> {
        self.need(animal, need)?;
        let distress = distress.clamp(0.0, 100.0);
        let value_q16 = ((1000.0 - distress * 10.0) * Q16_ONE as f32).round() as i32;
        Some(SetAnimalNeed { animal, need, value_q16 })
    }

    pub(super) fn authored_float(&self, entity: Entity, attribute: AssetId) -> Option<f32> {
        let (_, definition, _, _) = self.objects.get(entity).ok()?;
        let object = self.definitions.get(&self.definition_assets)?.find_object(definition.0)?;
        object.authored_runtime.numeric_behavior_properties.iter().find_map(|(key, value)| {
            (*key == attribute).then_some(*value)
        })
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
            ScriptFloatAttribute::GoodNeedPoints | ScriptFloatAttribute::BadNeedPoints => self.need_points.get_mut(entity).ok().map(|points| {
                points.map_unchanged(|points| match attribute {
                    ScriptFloatAttribute::GoodNeedPoints => &mut points.good,
                    _ => &mut points.bad,
                }).set_if_neq(value);
            }).is_some(),
            ScriptFloatAttribute::AnimalNeed(_) => false,
            ScriptFloatAttribute::Authored(attribute) => {
                let Ok(mut values) = self.script_floats.get_mut(entity) else { return false; };
                if values.0.get(&attribute) != Some(&value) {
                    values.0.insert(attribute, value);
                }
                true
            }
        }
    }

    /// `BFG_GET_ENTITIES`: live world objects of one authored binder type.
    pub(super) fn entities_of_type(&self, actor: Entity, binder_type: &str) -> Vec<Entity> {
        let Ok((_, _, _, world)) = self.objects.get(actor) else {
            return Vec::new();
        };
        let definition = AssetId::from_key(&canonicalize_source_document_record_key(binder_type));
        let definitions = self.definitions.get(&self.definition_assets);
        self.objects
            .iter()
            .filter(|(_, candidate, _, member)| member.root == world.root && (candidate.0 == definition
                || definitions.and_then(|definitions| definitions.find_object(candidate.0))
                    .is_some_and(|object| object.behavior_type_identifiers.contains(&definition))))
            .map(|(entity, ..)| entity)
            .collect()
    }

    pub(super) fn primary_biome(&self, entity: Entity) -> Option<String> {
        let (_, definition, pose, world) = self.objects.get(entity).ok()?;
        let definitions = self.definitions.get(&self.definition_assets)?;
        let biome = definitions.find_object(definition.0)
            .and_then(|object| object.biomes.first().copied())
            .or_else(|| self.biomes.at(world.root, pose.translation()))?;
        if let Some((_, suffix)) = definitions.object_binder_type(definition.0).and_then(|binder| binder.rsplit_once('_')) {
            if AssetId::from_key(&suffix.to_ascii_lowercase()) == biome {
                return Some(format!("Biome_{suffix}"));
            }
        }
        let definition = definitions.find_biome(biome)?;
        self.localization.as_ref()?
            .borrow_loaded_localization_view(self.localization_assets.as_ref()?)?
            .find_plain_localized_text(definition.name_key)
            .map(|name| format!("Biome_{name}"))
    }

    pub(super) fn place_object(
        &mut self,
        commands: &mut Commands,
        actor: Entity,
        request: BehaviorScriptPlacement,
    ) -> Option<ScriptPlacedObject> {
        let (_, _, _, world) = self.objects.get(actor).ok()?;
        let definitions = self.definitions.get(&self.definition_assets)?;
        let kind = AssetId::from_key(&canonicalize_source_document_record_key(&request.kind));
        let object = definitions.find_object(kind)?;
        let placeable = definitions.find_placeable(kind)?;
        let mut surface = placeable.surface_policy;
        surface.land |= request.ignore_land_collision;
        if request.ignore_water_adjacent {
            surface.use_actual_depth_in_water_collision = false;
        }
        let mut position = request.position;
        if position.y == 0.0 {
            position.y = self.ground.ground_height(world.root, position.xz())?;
        }
        self.placement.validate(surface, world.root, None, position).ok()?;
        let prefab = definitions.scene(object_placement_prefab_identifier(object)?)?;
        let id = self.ids.get_mut(world.root).ok()?.allocate().ok()?;
        let pose = Transform::from_translation(position)
            .with_scale(Vec3::splat(object.prefab_scale))
            .with_rotation(Quat::from_rotation_y(-request.rotation_degrees.to_radians()));
        let entity = commands.spawn((*world, DefinitionId(kind), id, pose, Visibility::Inherited, PrefabPresentation::new(prefab))).id();
        Some(ScriptPlacedObject {
            entity,
            binder: definitions.object_binder_type(kind).unwrap_or(&request.kind).to_owned(),
            position,
        })
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

    pub(super) fn traversable_area(&self, actor: Entity) -> Option<Entity> {
        let area = self.habitat_members.get(actor).ok()?.habitat_entity;
        self.habitats.contains(area).then_some(area)
    }

    pub(super) fn area_has_feature(&self, area: Entity, feature: &str) -> bool {
        let Ok(summary) = self.habitats.get(area) else { return false; };
        if feature.eq_ignore_ascii_case("water") {
            return summary.water_area_square_metres > 0.0;
        }
        let feature = AssetId::from_key(&feature.to_ascii_lowercase());
        summary.biome_areas_square_metres.iter().any(|(biome, area)| *biome == feature && *area > 0.0)
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
        let child = AssetId::from_key("child");
        let mother = AssetId::from_key("mother");
        let mut parent_edits = EntityHashMap::default();
        for effect in pending {
            if let BehaviorScriptEffect::Kinship(owner, relation, recipient) = *effect {
                if role == child && is_parent_role(relation) {
                    let parents = parent_edits.entry(owner).or_insert_with(|| self.lineage.parents(owner));
                    let was_child = parents.mother == Some(entity) || parents.father == Some(entity);
                    if relation == mother {
                        parents.mother = recipient;
                    } else {
                        parents.father = recipient;
                    }
                    let is_child = parents.mother == Some(entity) || parents.father == Some(entity);
                    if is_child && !relatives.contains(&owner) {
                        relatives.push(owner);
                    } else if was_child && !is_child && !self.kinship_edges(entity, role).any(|(_, target)| target == owner) {
                        relatives.retain(|target| *target != owner);
                    }
                }
                if is_parent_role(role) && relation == child && recipient.is_none() {
                    relatives.retain(|target| *target != owner);
                }
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
        let mut parent_edits = HashMap::<(Entity, AssetId), Option<Entity>>::default();
        let mut authored_floats = EntityHashMap::<ScriptFloatAttributes>::default();
        for effect in effects {
            match effect {
                BehaviorScriptEffect::Presentation(request) => {
                    commands.write_message(request);
                }
                BehaviorScriptEffect::Crate(entity) => {
                    commands.entity(entity).try_insert(WorldEntityIsCrated);
                }
                BehaviorScriptEffect::Particle(particle) => commands.trigger(particle),
                BehaviorScriptEffect::Need(request) => { commands.write_message(request); }
                BehaviorScriptEffect::Token(recipient, name, payload) => {
                    commands.spawn((ChildOf(recipient), BehaviorSelfToken {
                        name,
                        available_at: self.clock.tick,
                        expires_at: u64::MAX,
                        payload,
                        retain_on_failure: false,
                    }));
                }
                BehaviorScriptEffect::Kinship(entity, role, target) => {
                    if is_parent_role(role) {
                        parent_edits.insert((entity, role), target);
                    }
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
                        if role == AssetId::from_key("child") && let Some(child) = target {
                            let parents = self.lineage.parents(child);
                            let mother = parent_edits.get(&(child, AssetId::from_key("mother"))).copied().unwrap_or(parents.mother);
                            let father = parent_edits.get(&(child, AssetId::from_key("father"))).copied().unwrap_or(parents.father);
                            if mother == Some(entity) || father == Some(entity) {
                                continue;
                            }
                        }
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
                            if role == AssetId::from_key("child") {
                                let mother = AssetId::from_key("mother");
                                let father = AssetId::from_key("father");
                                let mut children = self.lineage.children(entity);
                                for (&(child, _), &parent) in &parent_edits {
                                    if parent == Some(entity) && !children.contains(&child) {
                                        children.push(child);
                                    }
                                }
                                for child in children {
                                    let parents = self.lineage.parents(child);
                                    if *parent_edits.get(&(child, mother)).unwrap_or(&parents.mother) == Some(entity) {
                                        commands.entity(child).try_remove::<AnimalMother>();
                                        parent_edits.insert((child, mother), None);
                                    }
                                    if *parent_edits.get(&(child, father)).unwrap_or(&parents.father) == Some(entity) {
                                        commands.entity(child).try_remove::<AnimalFather>();
                                        parent_edits.insert((child, father), None);
                                    }
                                }
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
                    ScriptFloatAttribute::Authored(attribute) => {
                        authored_floats.entry(entity).or_insert_with(|| ScriptFloatAttributes(HashMap::default())).0.insert(attribute, value);
                    }
                    ScriptFloatAttribute::GoodNeedPoints | ScriptFloatAttribute::BadNeedPoints | ScriptFloatAttribute::AnimalNeed(_) => {}
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
        for (entity, floats) in authored_floats {
            commands.entity(entity).try_insert(floats);
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
