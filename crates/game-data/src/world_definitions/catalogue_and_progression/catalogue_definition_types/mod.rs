//! Authored purchase, construction, adoption, and hiring catalogue definitions.

use crate::AssetId;

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct CatalogueFilterFlags(pub(super) u16);

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum CatalogueCategory {
    Animals,
    Biomes,
    Scenery,
    Facilities,
    Fences,
    Paths,
    Staff,
    Tanks,
    Shows,
    Transport,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct CatalogueEntry {
    /// Authored shared-data values used by the purchase menu's field filters.
    #[serde(default)]
    pub filter_values: Vec<CatalogueFilterValue>,
    pub id: AssetId,
    pub definition: AssetId,
    pub kind: AssetId,
    pub kinds: Vec<AssetId>,
    pub category: CatalogueCategory,
    pub authored_purchase_sort_key: String,
    pub authored_purchase_sort_fallback_type_name: String,
    pub authored_type_registry_source_order: [u64; 3],
    pub filters: CatalogueFilterFlags,
    pub name_key: AssetId,
    pub icon: AssetId,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct CatalogueFilterValue {
    /// Case-normalized shared-data field name, used by authored filter selectors.
    pub field: String,
    /// Original textual value, also used to form `FilterText` localization keys.
    pub value: String,
}
