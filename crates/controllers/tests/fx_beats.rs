use djcraft_controllers::{Bridge, Event, Profile};
use djcraft_engine::Engine;
use djcraft_protocol::{Action, DeckAction, EFFECT_BEATS, Role};

#[test]
fn beats_encoders_step_shared_timing_independently_and_clamp() {
    let profile = Profile::from_json(include_str!(
        "../../../profiles/numark-mixtrack-pro-ii.json"
    ))
    .unwrap();
    let mut bridge = Bridge::new(profile).unwrap();
    let (mut engine, _mixer) = Engine::headless(48000);
    for deck in 0..2u8 {
        for raw in [0, 64] {
            assert!(
                bridge
                    .process(
                        "MixTrack Pro II",
                        Event::parse(&[0xb0, 0x21 + deck, raw]).unwrap(),
                        &engine.state()
                    )
                    .is_empty()
            );
        }
        for (raw, expected) in [
            (127, 0.25),
            (127, 0.125),
            (127, 0.125),
            (3, 1.),
            (3, 8.),
            (1, 8.),
            (127, 4.),
        ] {
            let actions = bridge.process(
                "MixTrack Pro II",
                Event::parse(&[0xb0, 0x21 + deck, raw]).unwrap(),
                &engine.state(),
            );
            assert_eq!(
                actions,
                vec![Action::Deck {
                    deck,
                    action: DeckAction::SetEffectBeats { beats: expected }
                }]
            );
            for action in actions {
                engine.apply(Role::Host, action).unwrap();
            }
            let state = engine.state();
            assert!(
                state.decks[usize::from(deck)]
                    .effects
                    .iter()
                    .all(|fx| fx.beats == expected)
            );
            let other = &state.decks[usize::from(deck ^ 1)];
            assert_eq!(other.effect_beats, if deck == 0 { 0.5 } else { 4. });
        }
    }
    assert_eq!(EFFECT_BEATS, [0.125, 0.25, 0.5, 1., 2., 4., 8.]);
    assert!(
        bridge
            .process(
                "Other port",
                Event::parse(&[0xb0, 0x21, 1]).unwrap(),
                &engine.state()
            )
            .is_empty()
    );
    assert!(
        bridge
            .process(
                "MixTrack Pro II",
                Event::parse(&[0xb1, 0x21, 1]).unwrap(),
                &engine.state()
            )
            .is_empty()
    );
}
