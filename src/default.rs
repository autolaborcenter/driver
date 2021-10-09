use std::time::Duration;

pub struct DefaultPacemaker;

impl super::DriverPacemaker for DefaultPacemaker {
    fn period() -> Duration {
        Duration::MAX
    }

    fn send(&mut self) -> bool {
        false
    }
}
