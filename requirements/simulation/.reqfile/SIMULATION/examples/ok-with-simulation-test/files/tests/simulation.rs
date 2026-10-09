use machine::core::machine::step;

#[test]
fn simulation_random_events() {
    // A fixed seed makes every failure reproducible.
    let mut seed: u32 = 42;
    let mut state: u8 = 0;
    for _ in 0..1000 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state = step(state % 200, (seed % 2) as u8);
        assert!(state <= 200);
    }
}
