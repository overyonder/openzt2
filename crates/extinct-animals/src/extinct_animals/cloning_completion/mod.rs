use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_progression::progression::{AwardHistoryOutcome, RecordAwardOutcome};
use openzt2_world_state::world_spawn::{
    world_entity_crating::AnimalCrate,
    world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    authored_float_attributes::AuthoredFloatAttributes,
    extinct_animals::extinct_animal_observable_fact_types::AuthoredSuperExtinctAnimal,
};

/// A placed cloning result. A failed animal can occupy the center until released.
#[derive(EntityEvent)]
pub struct CompleteCloningPlacement {
    pub entity: Entity,
    pub center: Entity,
    pub failure_inside_center: bool,
}

#[derive(Component)]
struct PlacedClone {
    center: Entity,
    failure_inside_center: bool,
}

#[derive(Component)]
#[relationship(relationship_target = CloningCenterOccupants)]
pub struct CloningCenterOccupantOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = CloningCenterOccupantOf)]
pub struct CloningCenterOccupants(Vec<Entity>);

pub(super) fn register(app: &mut App) {
    app.add_message::<RecordAwardOutcome>()
        .add_observer(accept_cloning_placement)
        .add_observer(settle_cloning_placement)
        .add_observer(assign_cloning_center_fact)
        .add_observer(release_cloning_center_occupant)
        .add_observer(clear_cloning_center_fact);
}

fn accept_cloning_placement(
    placement: On<CompleteCloningPlacement>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    centers: Query<(&DefinitionId, &WorldMember)>,
    products: Query<&WorldMember, Without<PlacedClone>>,
    mut commands: Commands,
) {
    let (Ok((definition, center)), Ok(product)) = (
        centers.get(placement.center),
        products.get(placement.entity),
    ) else {
        return;
    };
    if center.root != product.root
        || definitions
            .get(&assets)
            .and_then(|view| view.find_cloning_center(definition.0))
            .is_none()
    {
        return;
    }
    commands
        .entity(placement.entity)
        .try_insert_if_new(PlacedClone {
            center: placement.center,
            failure_inside_center: placement.failure_inside_center,
        });
}

fn settle_cloning_placement(
    added: On<Add<PlacedClone>>,
    products: Query<(&WorldMember, &PlacedClone, Has<AuthoredSuperExtinctAnimal>)>,
    mut commands: Commands,
) {
    let Ok((member, product, super_clone)) = products.get(added.entity) else {
        return;
    };
    if product.failure_inside_center {
        commands
            .entity(added.entity)
            .try_insert_if_new(CloningCenterOccupantOf(product.center));
    } else if super_clone {
        commands.write_message(RecordAwardOutcome {
            root: member.root,
            outcome: AwardHistoryOutcome::SuperClone,
        });
    }
}

fn assign_cloning_center_fact(
    added: On<Add<CloningCenterOccupantOf>>,
    mut attributes: Query<&mut AuthoredFloatAttributes>,
    mut commands: Commands,
) {
    let key = AssetId::from_key("b_incloningcenter");
    if let Ok(mut attributes) = attributes.get_mut(added.entity) {
        if attributes.get(key) != Some(1.0) {
            attributes.set(key, 1.0);
        }
    } else {
        let mut values = AuthoredFloatAttributes::default();
        values.set(key, 1.0);
        commands.entity(added.entity).try_insert_if_new(values);
    }
}

fn release_cloning_center_occupant(
    removed: On<Remove<AnimalCrate>>,
    occupants: Query<(), With<CloningCenterOccupantOf>>,
    mut commands: Commands,
) {
    if occupants.contains(removed.entity) {
        commands
            .entity(removed.entity)
            .try_remove::<CloningCenterOccupantOf>();
    }
}

fn clear_cloning_center_fact(
    removed: On<Remove<CloningCenterOccupantOf>>,
    mut attributes: Query<&mut AuthoredFloatAttributes>,
) {
    if let Ok(mut attributes) = attributes.get_mut(removed.entity) {
        let key = AssetId::from_key("b_incloningcenter");
        if attributes.get(key) == Some(1.0) {
            attributes.set(key, 0.0);
        }
    }
}
