use std::time::Duration;

#[derive(Clone)]
pub struct DefaultHandle;
pub struct DefaultPacemaker;

impl super::DriverHandle for DefaultHandle {
    type Command = ();

    fn send(&self, _: Self::Command) -> bool {
        false
    }
}

impl super::DriverPacemaker for DefaultPacemaker {
    fn period() -> Duration {
        Duration::MAX
    }

    fn send(&mut self) -> bool {
        false
    }
}
