use gpui_kit::{ScrollWheelEvent, px};

/// Preserve fractional trackpad motion. A conventional three-line wheel notch
/// is one step; Shift gives ten times finer control.
pub fn steps(event: &ScrollWheelEvent) -> f32 {
    let delta = event.delta.pixel_delta(px(40.));
    let axis = if delta.y.abs() >= delta.x.abs() {
        delta.y.as_f32()
    } else {
        delta.x.as_f32()
    };
    axis / 120. * if event.modifiers.shift { 0.1 } else { 1. }
}
#[cfg(test)]
mod tests {
    use super::steps;
    use gpui_kit::{ScrollDelta, ScrollWheelEvent, point, px};
    #[test]
    fn wheel_direction_fractional_motion_and_fine_mode() {
        let mut event = ScrollWheelEvent::default();
        event.delta = ScrollDelta::Lines(point(0., 3.));
        assert_eq!(steps(&event), 1.);
        event.modifiers.shift = true;
        assert!((steps(&event) - 0.1).abs() < 1e-6);
        event.modifiers.shift = false;
        event.delta = ScrollDelta::Pixels(point(px(0.), px(-30.)));
        assert_eq!(steps(&event), -0.25);
        event.delta = ScrollDelta::Lines(point(3., 0.));
        assert_eq!(steps(&event), 1.);
    }
}
