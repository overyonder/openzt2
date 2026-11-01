#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct GuestFavouriteAnimalTrendPolicy {
    pub trend_group_percent: i32,
    pub trend_setter_percent: i32,
    pub spawn_thresholds: [u32; 2],
}
