//! Resolution of authored finance and overview-map vocabulary.

use crate::assets::source_document::ui::model::SourceUiMapLayer;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::finance_table::UiFinanceTableValueSource;
use openzt2_game_data::ui_document::overview_map_presentation::{
    UiOverviewCanvas, UiOverviewLayer,
};
use std::io;

pub(super) fn resolve_authored_finance_table_value_source(
    name: &str,
) -> Option<UiFinanceTableValueSource> {
    match name.to_ascii_lowercase().as_str() {
        "admissions" => Some(UiFinanceTableValueSource::AdmissionsCount),
        "admissions_income" => Some(UiFinanceTableValueSource::AdmissionIncome),
        "cash_grants" => Some(UiFinanceTableValueSource::CashGrants),
        "donations_income" => Some(UiFinanceTableValueSource::DonationIncome),
        "concessions_food_drink" => Some(UiFinanceTableValueSource::FoodDrinkSales),
        "recycling" => Some(UiFinanceTableValueSource::RecyclingIncome),
        "concessions_gifts" => Some(UiFinanceTableValueSource::GiftSales),
        "animal" => Some(UiFinanceTableValueSource::AnimalAdoption),
        "animal_upkeep" => Some(UiFinanceTableValueSource::AnimalUpkeep),
        "construction" => Some(UiFinanceTableValueSource::Construction),
        "research" => Some(UiFinanceTableValueSource::Research),
        "staff_salaries" => Some(UiFinanceTableValueSource::StaffSalaries),
        "upkeep" => Some(UiFinanceTableValueSource::Upkeep),
        "zoo" => Some(UiFinanceTableValueSource::ClosingCash),
        _ => None,
    }
}

pub(super) fn lower_authored_overview_map_layer_to_canonical_canvas(
    layer: &SourceUiMapLayer,
    input: &AuthoredUiDocument,
) -> io::Result<UiOverviewCanvas> {
    let source = layer.source.as_deref().unwrap_or(&layer.kind);
    let normalized = source
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "terrain" | "overviewmapterrain" => Ok(UiOverviewCanvas::Terrain),
        "water" | "overviewmapwater" => Ok(UiOverviewCanvas::Water),
        "path" | "paths" | "overviewmappaths" => Ok(UiOverviewCanvas::Paths),
        "fence" | "fences" | "overviewmapfences" => Ok(UiOverviewCanvas::Fences),
        "groundtrack" => Ok(UiOverviewCanvas::GroundTrack),
        "elevatedpath" | "elevatedpaths" => Ok(UiOverviewCanvas::ElevatedPaths),
        "skytrack" => Ok(UiOverviewCanvas::SkyTrack),
        _ => Err(invalid_at(
            input,
            &format!("unsupported overview-map canvas source {source:?}"),
        )),
    }
}

pub(super) fn lower_authored_overview_map_layer_to_canonical_layer(
    layer: &SourceUiMapLayer,
    input: &AuthoredUiDocument,
) -> io::Result<UiOverviewLayer> {
    let source = layer.source.as_deref().unwrap_or(&layer.kind);
    let normalized = source
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "terrain" | "overviewmapterrain" => Ok(UiOverviewLayer::Terrain),
        "water" | "overviewmapwater" => Ok(UiOverviewLayer::Water),
        "path" | "paths" | "overviewmappaths" => Ok(UiOverviewLayer::Paths),
        "fence" | "fences" | "overviewmapfences" => Ok(UiOverviewLayer::Fences),
        "building" | "buildings" => Ok(UiOverviewLayer::Buildings),
        "animal" | "animals" => Ok(UiOverviewLayer::Animals),
        "groundtrack" => Ok(UiOverviewLayer::GroundTrack),
        "elevatedpath" | "elevatedpaths" => Ok(UiOverviewLayer::ElevatedPaths),
        "skytrack" => Ok(UiOverviewLayer::SkyTrack),
        "fossilmarker" | "fossilmarkers" => Ok(UiOverviewLayer::FossilMarkers),
        "where" => Ok(UiOverviewLayer::CameraPosition),
        _ => Err(invalid_at(
            input,
            &format!("unsupported overview-map layer source {source:?}"),
        )),
    }
}
