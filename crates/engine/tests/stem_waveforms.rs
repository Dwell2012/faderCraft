use djcraft_engine::Track;

#[test]
fn stem_envelopes_preserve_stereo_energy_residual_and_partial_last_block() {
    // Opposite-polarity stereo must remain visible, including a one-frame tail.
    let frames = 257;
    let track = Track::from_samples("stereo", 48000, [0.4, -0.4].repeat(frames));
    assert!(track.stem_levels(0).is_none());
    let mut vocals = [0.1, -0.1].repeat(frames);
    vocals[(frames - 1) * 2..].copy_from_slice(&[0.3, -0.3]);
    track
        .attach_stems([
            vocals.into_boxed_slice(),
            vec![0.; frames * 2].into_boxed_slice(),
            [0.05, -0.05].repeat(frames).into_boxed_slice(),
            [0.2, -0.2].repeat(frames).into_boxed_slice(),
        ])
        .unwrap();
    for (block, expected) in [(0, [0.1, 0.05, 0.05, 0.2]), (1, [0.3, 0.15, 0.05, 0.2])] {
        let levels = track.stem_levels(block).unwrap();
        for (actual, expected) in levels.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1e-5,
                "block {block}: {levels:?}"
            );
        }
    }
    assert_eq!(track.stem_levels(2), Some([0.; 4]));
    assert!(
        track
            .attach_stems(std::array::from_fn(
                |_| vec![0.; frames * 2].into_boxed_slice()
            ))
            .is_err()
    );
    assert!((track.stem_levels(0).unwrap()[0] - 0.1).abs() < 1e-5);
}
