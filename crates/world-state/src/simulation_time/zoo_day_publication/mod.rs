use bevy::prelude::*;

use super::simulation_clock_types::{ZooClock, ZooDay};

pub fn publish_zoo_day(clock: Option<Res<ZooClock>>, mut day: ResMut<ZooDay>) {
    day.set_if_neq(ZooDay(clock.as_ref().map_or(0, |clock| clock.absolute_day)));
}
