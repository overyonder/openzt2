fn main() {
    openzt2_game::run_game_application_from_process_arguments()
        .unwrap_or_else(|error| panic!("could not start OpenZT2: {error}"));
}
