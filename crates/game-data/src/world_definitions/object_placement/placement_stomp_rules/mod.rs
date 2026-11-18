use std::collections::BTreeMap;

use crate::AssetId;

#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub struct PlacementStompPolicy {
    pub grid_snap: bool,
    /// Most-specific declaring family first, with each family's rules in source order.
    pub families: Vec<Vec<PlacementStompRule>>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub struct PlacementStompRule {
    pub action: PlacementStompAction,
    pub target: PlacementStompTarget,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub enum PlacementStompAction {
    Allow,
    Prevent,
    Delete,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub enum PlacementStompTarget {
    Type(AssetId),
    Property { name: AssetId, values: Vec<AssetId> },
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq, serde::Serialize)]
pub struct PlacementTargetFacts {
    pub types: Vec<AssetId>,
    pub properties: BTreeMap<AssetId, AssetId>,
}

impl PlacementStompPolicy {
    pub fn action_for_target(
        &self,
        types: &[AssetId],
        properties: Option<&BTreeMap<AssetId, AssetId>>,
    ) -> Option<PlacementStompAction> {
        self.families.iter().find_map(|family| {
            let most_specific_type = types.iter().rev().find(|id| {
                family.iter().any(|rule| matches!(&rule.target, PlacementStompTarget::Type(target) if target == *id))
            });
            family.iter().rev().find_map(|rule| {
                let matches = match &rule.target {
                    PlacementStompTarget::Type(id) => most_specific_type == Some(id),
                    PlacementStompTarget::Property { name, values } => properties
                        .and_then(|properties| properties.get(name))
                        .is_some_and(|value| values.contains(value)),
                };
                matches.then_some(rule.action)
            })
        })
    }
}
