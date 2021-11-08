use super::Driver;
use std::{hash::Hash, sync::mpsc, time::Instant};

mod context;

pub trait MultipleDeviceDriver: Driver {
    type Command;
    fn send(&mut self, command: Self::Command);
}

pub struct SupervisorForMultiple<D: Driver>(Vec<(D::Key, Box<D>)>);

pub enum SupervisorEventForMultiple<'a, D: MultipleDeviceDriver> {
    Connected(&'a D::Key, &'a mut D),
    ConnectFailed {
        current: usize,
        target: usize,
        next_try: &'a mut Instant,
    },
    Event(
        D::Key,
        Option<(Instant, D::Event)>,
        &'a mpsc::Sender<D::Command>,
    ),
    Disconnected(D::Key),
}

impl<D: MultipleDeviceDriver> SupervisorForMultiple<D>
where
    D::Key: Send + Clone + Eq + Hash,
    D::Event: Send,
    D::Command: Send,
{
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn join<F>(&mut self, init_len: usize, f: F)
    where
        F: FnMut(SupervisorEventForMultiple<D>) -> usize,
    {
        context::JoinContextForMultiple::new(self, init_len, f).run();
    }
}
