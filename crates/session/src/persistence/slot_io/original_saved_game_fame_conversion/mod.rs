use std::io;

use openzt2_game_assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_zoo_state::progression::{fame_history_types::FameHistorySample, fame_types::Fame};

pub(super) fn read_original_saved_fame(
    status: &OrderedSourceDocumentNode,
) -> io::Result<(Fame, Vec<FameHistorySample>)> {
    let percent = |name| -> io::Result<f32> {
        let value = status
            .attribute(name)
            .ok_or_else(|| invalid(format!("ZTStatus has no {name}")))?
            .parse::<f32>()
            .map_err(|_| invalid(format!("saved {name} is not a percentage")))?;
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(invalid(format!("saved {name} lies outside 0..100")));
        }
        Ok(value)
    };
    let current_percent = percent("zoo_fame")?;
    let maximum_percent_reached = percent("zoo_fame_max_reached")?;
    let half_stars = status
        .attribute("num_halfstars")
        .ok_or_else(|| invalid("ZTStatus has no num_halfstars"))?
        .parse::<u8>()
        .map_err(|_| invalid("saved num_halfstars is not u8"))?;
    if half_stars > 10 || current_percent > maximum_percent_reached {
        return Err(invalid("saved current fame exceeds its range or maximum"));
    }
    let mut histories = status
        .element_children()
        .filter(|node| node.name.as_str() == "fameHistory");
    let history = histories
        .next()
        .ok_or_else(|| invalid("ZTStatus has no fameHistory"))?;
    if histories.next().is_some() {
        return Err(invalid("ZTStatus has multiple fame histories"));
    }
    let values = history
        .attribute("values")
        .ok_or_else(|| invalid("saved fameHistory has no values"))?;
    let mut samples = Vec::new();
    for (index, value) in values.split_ascii_whitespace().enumerate() {
        let stars = value
            .parse::<f32>()
            .map_err(|_| invalid("saved fame history value is not numeric"))?;
        let halves = stars * 2.0;
        if !halves.is_finite() || !(0.0..=10.0).contains(&halves) || halves.fract() != 0.0 {
            return Err(invalid(
                "saved fame history value is not a half-star rating",
            ));
        }
        samples.push(FameHistorySample {
            month_index: u32::try_from(index)
                .map_err(|_| invalid("saved fame history exceeds u32 months"))?,
            half_stars: halves as u8,
        });
    }
    let maximum_reached = Fame::half_stars_for_percent(maximum_percent_reached).max(half_stars);
    Ok((
        Fame {
            half_stars,
            current_percent: Some(current_percent),
            maximum_reached,
            maximum_percent_reached: Some(maximum_percent_reached),
        },
        samples,
    ))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
