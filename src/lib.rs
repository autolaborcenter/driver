use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

pub mod default;

pub trait Driver<T, S: DriverStatus>:
    'static + Send + Iterator<Item = (Instant, S::Event)>
{
    type Pacemaker: DriverPacemaker;
    type Handle: DriverHandle;

    fn new(t: T) -> (Self::Pacemaker, Self);
    fn handle(&self) -> Self::Handle;
    fn status(&self) -> S;
}

pub trait DriverStatus: 'static + Clone {
    type Event;

    fn update(&mut self, event: Self::Event);
}

pub trait DriverHandle: 'static + Clone {
    type Command;

    fn send(&self, command: Self::Command) -> bool;
}

pub trait DriverPacemaker: 'static + Send + Sized {
    fn period() -> Duration;
    fn send(&mut self) -> bool;
}

pub trait Module<T, S, D>
where
    S: DriverStatus,
    D: Driver<T, S>,
{
    fn keys() -> Vec<T>;

    fn open_all(len: usize) -> Vec<Box<D>> {
        let mut pacemakers = Vec::<Box<D::Pacemaker>>::new();
        let mut drivers = Vec::<Box<D>>::new();
        Self::keys()
            .into_iter()
            .map(|t| D::new(t))
            .for_each(|(p, d)| {
                pacemakers.push(Box::new(p));
                drivers.push(Box::new(d));
            });

        thread::spawn(move || {
            let period = D::Pacemaker::period();
            let mut timer = Timer(Instant::now());

            loop {
                pacemakers = pacemakers
                    .into_iter()
                    .filter_map(|mut s| if s.send() { Some(s) } else { None })
                    .collect();
                if pacemakers.is_empty() {
                    return;
                } else {
                    timer.wait_per(period);
                }
            }
        });

        {
            let counter = Arc::new(());
            drivers
                .into_iter()
                .map(|mut o| {
                    let counter = counter.clone();
                    thread::spawn(move || loop {
                        if let Some(_) = o.next() {
                            if Arc::strong_count(&counter) <= len {
                                return Some(o);
                            }
                        } else {
                            return None;
                        }
                    })
                })
                .collect::<Vec<_>>()
        }
        .into_iter()
        .filter_map(|o| o.join().ok().flatten())
        .collect()
    }
}

struct Timer(Instant);

impl Timer {
    fn wait_per(&mut self, period: Duration) {
        let now = Instant::now();
        while self.0 <= now {
            self.0 += period;
        }
        thread::sleep(self.0 - now);
    }
}
