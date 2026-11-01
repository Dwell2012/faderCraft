use djcraft_controllers::{Binding, Bridge, Event, HostCommand, Profile};
use djcraft_protocol::{Action, DeckAction, DeckState, HotCue, SessionState};
const PORT: &str = "MixTrack Pro II";

#[test]
fn pad_mode_follows_each_pressed_bank_even_on_an_empty_deck() {
    use djcraft_controllers::PadMode;
    let mut b = bridge();
    let s = state();
    for (deck, notes) in [(0, [0x6d, 0x53, 0x65]), (1, [0x71, 0x56, 0x69])] {
        for (number, mode) in
            notes
                .into_iter()
                .zip([PadMode::HotCues, PadMode::Loop, PadMode::Sampler])
        {
            note(&mut b, number, 127, &s);
            let modes = b.take_pad_modes();
            assert_eq!(modes[deck], Some(mode));
            assert_eq!(modes[deck ^ 1], None);
            assert!(b.take_pad_modes().iter().all(Option::is_none));
            note(&mut b, number, 0, &s);
            assert!(b.take_pad_modes().iter().all(Option::is_none));
        }
    }
    note(&mut b, 0x61, 127, &s);
    note(&mut b, 0x61, 0, &s);
    assert!(b.take_pad_modes().iter().all(Option::is_none));
}
fn bridge() -> Bridge {
    Bridge::new(
        Profile::from_json(include_str!(
            "../../../profiles/numark-mixtrack-pro-ii.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn state() -> SessionState {
    SessionState {
        decks: vec![DeckState::default(); 2],
        ..Default::default()
    }
}
fn note(b: &mut Bridge, n: u8, value: u8, s: &SessionState) -> Vec<Action> {
    b.process(PORT, Event::parse(&[0x90, n, value]).unwrap(), s)
}
fn lamp(b: &Bridge, s: &SessionState, n: u8, time: u64) -> u8 {
    b.state_feedback(PORT, s, time, &[])
        .into_iter()
        .find(|m| m[1] == n)
        .unwrap()[2]
}
#[test]
fn leds_follow_host_cues_samples_loops_and_fx_with_port_and_deck_isolation() {
    let b = bridge();
    let mut s = state();
    assert!(b.state_feedback("other", &s, 0, &[]).is_empty());
    s.decks[0].effects[0].kind = djcraft_protocol::EffectKind::Filter;
    assert!(
        b.state_feedback(PORT, &s, 0, &[Some(1.), None])
            .contains(&[0x90, 0x53, 100])
    );
    assert_eq!(lamp(&b, &s, 0x59, 0), 0);
    s.decks[0].effects[0].enabled = true;
    assert_eq!(lamp(&b, &s, 0x59, 0), 0);
    s.decks[0].effects[0].parameter = 0.2;
    s.decks[1].hot_cues[2] = Some(HotCue {
        seconds: 2.,
        loop_end: None,
    });
    s.decks[0].samples[0] = 1;
    s.decks[0].samples[1] = 2;
    s.decks[1].loop_region = Some([1., 3.]);
    assert_eq!(lamp(&b, &s, 0x59, 0), 100);
    s.decks[0].effects[0].parameter = 0.5;
    assert_eq!(lamp(&b, &s, 0x59, 0), 0);
    assert_eq!(lamp(&b, &s, 0x5d, 0), 0);
    assert_eq!(lamp(&b, &s, 0x73, 0), 100);
    assert_eq!(lamp(&b, &s, 0x6f, 0), 0);
    assert_eq!(lamp(&b, &s, 0x65, 0), 100);
    assert_eq!(lamp(&b, &s, 0x65, 250), 100);
    assert_eq!(lamp(&b, &s, 0x66, 0), 100);
    assert_eq!(lamp(&b, &s, 0x66, 250), 0);
    assert_eq!(lamp(&b, &s, 0x56, 0), 100);
    assert_eq!(lamp(&b, &s, 0x53, 0), 0);
    // Feedback is output-only: a note matching an LED must not dispatch an action.
    let mut b = b;
    assert!(note(&mut b, 0x28, 127, &s).is_empty());
    assert!(b.take_commands().is_empty());
}

#[test]
fn pitch_center_lamps_follow_each_decks_zero_tempo_without_blinking() {
    let b = bridge();
    let mut s = state();
    for time in [0, 250, 500] {
        s.decks[0].tempo = 1.;
        s.decks[1].tempo = 1.02;
        assert_eq!(lamp(&b, &s, 0x28, time), 100);
        assert_eq!(lamp(&b, &s, 0x29, time), 0);
        s.decks[0].tempo = 0.98;
        s.decks[1].tempo = 1.;
        assert_eq!(lamp(&b, &s, 0x28, time), 0);
        assert_eq!(lamp(&b, &s, 0x29, time), 100);
    }
}
#[test]
fn shift_fx_and_tap_choose_auto_loops_without_toggling_effects() {
    let s = state();
    for deck in 0..2u8 {
        let mut b = bridge();
        note(&mut b, 0x61 + deck, 127, &s);
        for (slot, beats) in [1, 2, 4, 16].into_iter().enumerate() {
            let n = 0x59 + deck * 4 + slot as u8;
            assert!(note(&mut b, n, 127, &s).is_empty());
            assert_eq!(
                b.take_commands(),
                vec![HostCommand::AutoLoop { deck, beats }]
            );
            assert!(note(&mut b, n, 127, &s).is_empty());
            assert!(b.take_commands().is_empty());
            note(&mut b, n, 0, &s);
        }
        note(&mut b, 0x61 + deck, 0, &s);
        assert!(matches!(
            note(&mut b, 0x59 + deck * 4, 127, &s).as_slice(),
            [Action::Deck {
                action: DeckAction::SetEffect { .. },
                ..
            }]
        ));
    }
}
#[test]
fn shift_four_latches_delete_mode_blinks_cues_and_disconnect_resets_it() {
    let mut b = bridge();
    let mut s = state();
    s.decks[0].hot_cues[0] = Some(HotCue {
        seconds: 1.,
        loop_end: None,
    });
    s.decks[0].hot_cues[1] = Some(HotCue {
        seconds: 2.,
        loop_end: None,
    });
    note(&mut b, 0x61, 127, &s);
    assert!(note(&mut b, 0x70, 127, &s).is_empty());
    note(&mut b, 0x70, 0, &s);
    note(&mut b, 0x61, 0, &s);
    assert_eq!(lamp(&b, &s, 0x6d, 0), 100);
    assert_eq!(lamp(&b, &s, 0x6d, 250), 0);
    let result = note(&mut b, 0x6d, 127, &s);
    let mut cues = s.decks[0].hot_cues;
    cues[0] = None;
    assert_eq!(
        result,
        vec![Action::Deck {
            deck: 0,
            action: DeckAction::SetHotCues { cues }
        }]
    );
    note(&mut b, 0x6d, 0, &s);
    b.disconnect(PORT);
    assert_eq!(lamp(&b, &s, 0x6d, 250), 100);
    assert!(matches!(
        note(&mut b, 0x6d, 127, &s).as_slice(),
        [Action::Deck {
            action: DeckAction::TriggerHotCue { slot: 0, .. },
            ..
        }]
    ));
    let mut bad = b.profile().clone();
    bad.mappings
        .iter_mut()
        .find(|m| matches!(m.binding, Binding::Led { .. }))
        .unwrap()
        .source = djcraft_controllers::Source::Cc {
        channel: 0,
        number: 1,
    };
    assert!(bad.validate().is_err());
}
