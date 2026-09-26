pub(super) fn ambient_animal_population_is_below_authored_maximum(
    current_population: usize,
    authored_population_bounds: [u16; 2],
) -> bool {
    current_population < usize::from(authored_population_bounds[1])
}
