use std::io::{Cursor, Read};

use openzt2_game_data::AssetId;
use openzt2_interface_state::persistence::profile_types::ProfileFailure;
use openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables;

pub(super) fn append_profile_scenario_variables(
    bytes: &mut Vec<u8>,
    variables: &ProfileScenarioVariables,
) {
    bytes.extend_from_slice(&(variables.iter().len() as u32).to_le_bytes());
    for (key, value) in variables.iter() {
        bytes.extend_from_slice(&key.0);
        bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
}

pub(super) fn decode_profile_scenario_variables(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<ProfileScenarioVariables, ProfileFailure> {
    let mut reader = Cursor::new(bytes.get(*offset..).ok_or(ProfileFailure::CorruptIndex)?);
    let mut count = [0; 4];
    reader
        .read_exact(&mut count)
        .map_err(|_| ProfileFailure::CorruptIndex)?;
    let count = u32::from_le_bytes(count);
    if count > 4096 {
        return Err(ProfileFailure::CorruptIndex);
    }
    let mut variables = ProfileScenarioVariables::default();
    let mut previous = None;
    for _ in 0..count {
        let mut key = [0; 16];
        let mut length = [0; 2];
        reader
            .read_exact(&mut key)
            .map_err(|_| ProfileFailure::CorruptIndex)?;
        reader
            .read_exact(&mut length)
            .map_err(|_| ProfileFailure::CorruptIndex)?;
        let length = usize::from(u16::from_le_bytes(length));
        if length > 1024 || previous.is_some_and(|previous| previous >= key) {
            return Err(ProfileFailure::CorruptIndex);
        }
        previous = Some(key);
        let mut value = vec![0; length];
        reader
            .read_exact(&mut value)
            .map_err(|_| ProfileFailure::CorruptIndex)?;
        let value = std::str::from_utf8(&value).map_err(|_| ProfileFailure::CorruptIndex)?;
        variables
            .set(AssetId(key), Some(value))
            .map_err(|_| ProfileFailure::CorruptIndex)?;
    }
    *offset += reader.position() as usize;
    Ok(variables)
}
