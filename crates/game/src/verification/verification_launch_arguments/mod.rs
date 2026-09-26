use super::{
    verification_journey_script_parsing::read_verification_journey_file,
    verification_journey_types::VerificationJourney,
};

/// Headless verification requested on the command line. Normal play has none.
#[derive(Debug, Clone)]
pub(crate) enum VerificationLaunchRequest {
    Journey {
        journey: VerificationJourney,
        output_directory: std::path::PathBuf,
        windowed: bool,
    },
    /// Loads one catalogue map through the production loaders and exits.
    MapLoad { map_index: usize },
}

impl VerificationLaunchRequest {
    pub(crate) const fn renders_to_window(&self) -> bool {
        matches!(self, Self::Journey { windowed: true, .. })
    }
}

pub(crate) fn read_verification_launch_request_from_process_arguments(
) -> std::io::Result<Option<VerificationLaunchRequest>> {
    use std::io::{Error, ErrorKind};

    let mut journey_path = None;
    let mut output_directory = None;
    let mut windowed = false;
    let mut map_index = None;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "missing command-line value"))
        };
        match argument.to_string_lossy().as_ref() {
            "--verify-journey" => journey_path = Some(std::path::PathBuf::from(value()?)),
            "--verification-output" => {
                output_directory = Some(std::path::PathBuf::from(value()?));
            }
            "--verification-windowed" => windowed = true,
            "--verify-map-load" => {
                map_index = Some(value()?.to_string_lossy().parse().map_err(|_| {
                    Error::new(
                        ErrorKind::InvalidInput,
                        "map index must be an unsigned integer",
                    )
                })?);
            }
            _ => {}
        }
    }
    match (journey_path, map_index) {
        (Some(_), Some(_)) => Err(Error::new(
            ErrorKind::InvalidInput,
            "--verify-journey and --verify-map-load are mutually exclusive",
        )),
        (Some(journey_path), None) => {
            let output_directory = output_directory.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    "--verify-journey needs --verification-output",
                )
            })?;
            std::fs::create_dir_all(&output_directory)?;
            Ok(Some(VerificationLaunchRequest::Journey {
                journey: read_verification_journey_file(&journey_path)?,
                output_directory,
                windowed,
            }))
        }
        (None, Some(map_index)) => Ok(Some(VerificationLaunchRequest::MapLoad { map_index })),
        (None, None) => Ok(None),
    }
}
