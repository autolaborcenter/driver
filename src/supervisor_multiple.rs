use super::Driver;
use std::{hash::Hash, time::Instant};

mod context;

pub struct SupervisorForMultiple<D: Driver>(Vec<(D::Key, Box<D>)>);

pub enum SupervisorEventForMultiple<'a, D: Driver> {
    Connected(&'a D::Key, &'a mut D),
    ConnectFailed { current: usize, target: usize },
    Event(D::Key, Option<(Instant, D::Event)>),
    Disconnected(D::Key),
}

impl<D: Driver> SupervisorForMultiple<D>
where
    D::Key: Send + Clone + Eq + Hash,
    D::Event: Send,
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
