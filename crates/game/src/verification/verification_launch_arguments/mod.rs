use super::{
    verification_journey_script_parsing::{
        read_verification_journey_file, verification_navigation_routes,
    },
    verification_journey_types::{
        VerificationCaptureMode, VerificationJourney, VerificationNavigationRoutes,
    },
};

/// Headless verification requested on the command line. Normal play has none.
#[derive(Debug, Clone)]
pub(crate) enum VerificationLaunchRequest {
    /// Runs the journeys in order in this one game.
    Journeys {
        journeys: Box<[VerificationJourney]>,
        routes: VerificationNavigationRoutes,
        output_root: std::path::PathBuf,
        windowed: bool,
        capture_mode: VerificationCaptureMode,
        /// Wall time for the whole game, after which it stops with the remaining
        /// journeys unrun.
        budget: std::time::Duration,
    },
    /// Loads one catalogue map through the production loaders and exits.
    MapLoad { map_index: usize },
}

impl VerificationLaunchRequest {
    pub(crate) const fn renders_to_window(&self) -> bool {
        matches!(self, Self::Journeys { windowed: true, .. })
    }
}

pub(crate) fn read_verification_launch_request_from_process_arguments(
) -> std::io::Result<Option<VerificationLaunchRequest>> {
    use std::io::{Error, ErrorKind};

    let mut journey_paths = Vec::new();
    let mut output_root = None;
    let mut windowed = false;
    let mut capture_mode = VerificationCaptureMode::Full;
    let mut budget = std::time::Duration::from_secs(300);
    let mut map_index = None;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "missing command-line value"))
        };
        match argument.to_string_lossy().as_ref() {
            "--verify-journey" => journey_paths.push(std::path::PathBuf::from(value()?)),
            "--verification-output" => {
                output_root = Some(std::path::PathBuf::from(value()?));
            }
            "--verification-windowed" => windowed = true,
            "--verification-capture-mode" => {
                capture_mode = match value()?.to_string_lossy().as_ref() {
                    "full" => VerificationCaptureMode::Full,
                    "ready" => VerificationCaptureMode::Ready,
                    _ => {
                        return Err(Error::new(
                            ErrorKind::InvalidInput,
                            "capture mode must be full or ready",
                        ));
                    }
                };
            }
            "--verification-budget-seconds" => {
                budget = std::time::Duration::from_secs(
                    value()?.to_string_lossy().parse().map_err(|_| {
                        Error::new(
                            ErrorKind::InvalidInput,
                            "budget must be a whole number of seconds",
                        )
                    })?,
                );
            }
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
    match (journey_paths.first(), map_index) {
        (Some(_), Some(_)) => Err(Error::new(
            ErrorKind::InvalidInput,
            "--verify-journey and --verify-map-load are mutually exclusive",
        )),
        (Some(_), None) => {
            let output_root = output_root.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    "--verify-journey needs --verification-output",
                )
            })?;
            let routes = verification_navigation_routes()?;
            let journeys = journey_paths
                .iter()
                .map(|path| read_verification_journey_file(path))
                .collect::<std::io::Result<Box<[_]>>>()?;
            for journey in &journeys {
                std::fs::create_dir_all(output_root.join(&journey.name))?;
            }
            Ok(Some(VerificationLaunchRequest::Journeys {
                journeys,
                routes,
                output_root,
                windowed,
                capture_mode,
                budget,
            }))
        }
        (None, Some(map_index)) => Ok(Some(VerificationLaunchRequest::MapLoad { map_index })),
        (None, None) => Ok(None),
    }
}
