use std::time::{Duration, Instant};

use crossterm::event::{Event, MouseEventKind};
use diffo_app::workbench::Workbench;

const ACTIVE_INTERVAL: Duration = Duration::from_millis(48);
const BURST_RESET: Duration = Duration::from_millis(120);

#[derive(Default)]
pub(super) struct WheelFriction {
    direction: Option<MouseEventKind>,
    last_event: Option<Instant>,
    cancelled: bool,
}

impl WheelFriction {
    fn cancel(&mut self) {
        self.cancelled = self.direction.is_some();
    }

    fn accepts(&mut self, event: &Event, now: Instant) -> bool {
        let Event::Mouse(mouse) = event else {
            return true;
        };
        let direction = match mouse.kind {
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => mouse.kind,
            _ => return true,
        };
        let gap = self.last_event.map(|last| now.duration_since(last));
        let starts_burst =
            self.direction != Some(direction) || gap.is_none_or(|gap| gap >= BURST_RESET);
        self.last_event = Some(now);
        if starts_burst {
            self.direction = Some(direction);
            self.cancelled = false;
            return true;
        }
        if self.cancelled {
            return false;
        }
        gap.is_some_and(|gap| gap <= ACTIVE_INTERVAL)
    }
}

pub(super) fn filter(
    events: &mut Vec<Event>,
    workbench: &Workbench,
    wheel_friction: &mut WheelFriction,
    now: Instant,
) {
    events.retain(|event| {
        if workbench.is_diff_change_navigation(event) {
            wheel_friction.cancel();
        }
        wheel_friction.accepts(event, now)
    });
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
    use diffo_app::workbench::Workbench;
    use diffo_core::RepositorySnapshot;

    use super::{WheelFriction, filter};

    #[test]
    fn preserves_active_scroll_and_cuts_off_the_tail() {
        let mut friction = WheelFriction::default();
        let started = Instant::now();
        let down = wheel(MouseEventKind::ScrollDown);

        assert!(friction.accepts(&down, started));
        assert!(friction.accepts(&down, started));
        assert!(friction.accepts(&down, started));
        assert!(friction.accepts(&down, started + Duration::from_millis(48)));
        assert!(!friction.accepts(&down, started + Duration::from_millis(97)));
        assert!(friction.accepts(&down, started + Duration::from_millis(217)));
        assert!(friction.accepts(
            &wheel(MouseEventKind::ScrollUp),
            started + Duration::from_millis(218)
        ));
        let right = wheel(MouseEventKind::ScrollRight);
        assert!(friction.accepts(&right, started + Duration::from_millis(219)));
        assert!(!friction.accepts(&right, started + Duration::from_millis(268)));
    }

    #[test]
    fn cancelled_burst_rejects_its_tail_until_reversal_or_reset() {
        let mut friction = WheelFriction::default();
        let started = Instant::now();
        let down = wheel(MouseEventKind::ScrollDown);

        assert!(friction.accepts(&down, started));
        assert!(friction.accepts(&down, started + Duration::from_millis(20)));
        friction.cancel();
        assert!(!friction.accepts(&down, started + Duration::from_millis(40)));
        assert!(!friction.accepts(&down, started + Duration::from_millis(60)));
        assert!(friction.accepts(
            &wheel(MouseEventKind::ScrollUp),
            started + Duration::from_millis(61)
        ));

        friction.cancel();
        assert!(!friction.accepts(
            &wheel(MouseEventKind::ScrollUp),
            started + Duration::from_millis(80)
        ));
        assert!(friction.accepts(
            &wheel(MouseEventKind::ScrollUp),
            started + Duration::from_millis(200)
        ));
    }

    #[test]
    fn only_navigation_cancels_later_wheel_events_in_the_batch() {
        let workbench = Workbench::new(RepositorySnapshot::default());
        let mut friction = WheelFriction::default();
        let now = Instant::now();
        let down = wheel(MouseEventKind::ScrollDown);
        let unrelated = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        let mut normal_scroll = vec![down.clone(), unrelated, down.clone()];

        filter(&mut normal_scroll, &workbench, &mut friction, now);

        assert_eq!(normal_scroll.len(), 3);

        let next = Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        let mut events = vec![down.clone(), next.clone(), down.clone(), down];

        filter(&mut events, &workbench, &mut friction, now);

        assert_eq!(events, vec![wheel(MouseEventKind::ScrollDown), next]);
    }

    fn wheel(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: 50,
            row: 10,
            modifiers: KeyModifiers::NONE,
        })
    }
}
