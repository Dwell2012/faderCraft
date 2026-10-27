use djcraft_controllers::{Binding, Bridge, Encoding, Event, HostCommand, Profile, Source};
use djcraft_protocol::{DeckState, SessionState};
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
#[test]
fn browse_rotary_dispatches_every_relative_tick_but_ignores_neutral_other_ports_and_channels() {
    let mut b = bridge();
    let s = state();
    for (raw, steps) in [(1, 1), (1, 1), (127, -1), (3, 3), (125, -3)] {
        assert!(
            b.process(
                "MixTrack Pro II",
                Event::parse(&[0xb0, 0x1a, raw]).unwrap(),
                &s
            )
            .is_empty()
        );
        assert_eq!(b.take_commands(), vec![HostCommand::Browse { steps }]);
    }
    for (port, status, raw) in [
        ("other", 0xb0, 1),
        ("MixTrack Pro II", 0xb1, 1),
        ("MixTrack Pro II", 0xb0, 0),
        ("MixTrack Pro II", 0xb0, 64),
    ] {
        assert!(
            b.process(port, Event::parse(&[status, 0x1a, raw]).unwrap(), &s)
                .is_empty()
        );
        assert!(b.take_commands().is_empty());
    }
}
#[test]
fn back_and_load_are_press_only_and_browser_activity_does_not_release_held_transport_cue() {
    let mut b = bridge();
    let s = state();
    b.process(
        "MixTrack Pro II",
        Event::parse(&[0x90, 0x33, 127]).unwrap(),
        &s,
    );
    b.take_commands();
    for (n, command) in [
        (0x77, HostCommand::BrowseBack),
        (0x4b, HostCommand::LoadSelected { deck: 0 }),
        (0x34, HostCommand::LoadSelected { deck: 1 }),
    ] {
        assert!(
            b.process(
                "MixTrack Pro II",
                Event::parse(&[0x90, n, 127]).unwrap(),
                &s
            )
            .is_empty()
        );
        assert_eq!(b.take_commands(), vec![command]);
        b.process(
            "MixTrack Pro II",
            Event::parse(&[0x90, n, 127]).unwrap(),
            &s,
        );
        assert!(b.take_commands().is_empty());
        b.process("MixTrack Pro II", Event::parse(&[0x90, n, 0]).unwrap(), &s);
        assert!(b.take_commands().is_empty());
    }
    b.disconnect("MixTrack Pro II");
    assert_eq!(
        b.take_commands(),
        vec![HostCommand::Cue {
            deck: 0,
            pressed: false
        }]
    );
}
#[test]
fn browse_mapping_rejects_absolute_or_non_cc_inputs() {
    let b = bridge();
    let mut p = b.profile().clone();
    let m = p
        .mappings
        .iter_mut()
        .find(|m| matches!(m.binding, Binding::Browse { .. }))
        .unwrap();
    m.binding = Binding::Browse {
        encoding: Encoding::Absolute,
    };
    assert!(p.validate().is_err());
    let mut p = b.profile().clone();
    let m = p
        .mappings
        .iter_mut()
        .find(|m| matches!(m.binding, Binding::Browse { .. }))
        .unwrap();
    m.source = Source::Note {
        channel: 0,
        number: 1,
    };
    assert!(p.validate().is_err());
}
