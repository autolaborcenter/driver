use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

/// 实现驱动特性，需要指定其对应的起搏器类型、状态类型和指令类型。
///
/// 通过 `T` 类型的键可以创建出驱动的实例。
///
/// 可以从驱动中读取状态，或向驱动发送指令。
///
/// 监听驱动事件是独占且阻塞的，但在传入的回调中可以修改驱动状态。
pub trait Driver<T>: 'static + Send + Sized {
    type Pacemaker: DriverPacemaker;
    type Status: DriverStatus;
    type Command;

    fn new(t: T) -> Option<(Self::Pacemaker, Self)>;
    fn status<'a>(&'a self) -> &'a Self::Status;
    fn send(&mut self, command: (Instant, Self::Command));
    fn join<F>(&mut self, f: F) -> bool
    where
        F: FnMut(&mut Self, Option<(Instant, <Self::Status as DriverStatus>::Event)>) -> bool;

    fn open_all<I>(keys: I, len: usize) -> Vec<Box<Self>>
    where
        I: IntoIterator<Item = T>,
    {
        let mut pacemakers = Vec::<Box<Self::Pacemaker>>::new();
        let mut drivers = Vec::<Box<Self>>::new();
        keys.into_iter()
            .filter_map(|t| Self::new(t))
            .for_each(|(p, d)| {
                pacemakers.push(Box::new(p));
                drivers.push(Box::new(d));
            });

        thread::spawn(move || {
            let period = Self::Pacemaker::period();
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
                    thread::spawn(move || {
                        if o.join(|_, _| Arc::strong_count(&counter) > len) {
                            Some(o)
                        } else {
                            None
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

/// 状态的增量是事件。
///
/// 也可以通过累积事件来跟踪状态。
pub trait DriverStatus: 'static {
    type Event;

    fn update(&mut self, event: Self::Event);
}

/// 起搏器有一个静态不变的周期。
///
/// 应该根据这个周期定时发送触发脉冲。
pub trait DriverPacemaker: 'static + Send {
    fn period() -> Duration;
    fn send(&mut self) -> bool;
}

/// 空白起搏器，什么也不做，立即退出循环。
impl DriverPacemaker for () {
    fn period() -> Duration {
        Duration::MAX
    }

    fn send(&mut self) -> bool {
        false
    }
}

pub enum SupersivorEventForSingle<'a, T, D: Driver<T>> {
    Connected(&'a mut D),
    ConnectFailed,
    Event(
        &'a mut D,
        Option<(Instant, <D::Status as DriverStatus>::Event)>,
    ),
    Disconnected,
}

pub trait SupervisorForSingle<T, D: Driver<T>> {
    fn context<'a>(&'a mut self) -> &'a mut Box<Option<D>>;
    fn keys() -> Vec<T>;

    fn join<F>(&mut self, mut f: F)
    where
        F: FnMut(SupersivorEventForSingle<T, D>) -> bool,
    {
        loop {
            use SupersivorEventForSingle::*;

            match self.context().as_mut() {
                Some(ref mut driver) => loop {
                    // 上下文中保存了驱动
                    if driver.join(|d, e| f(Event(d, e))) || !f(Disconnected) {
                        // 驱动退出阻塞或断联后不希望重试
                        return;
                    } else {
                        // 清除上下文，重试
                        *self.context() = Box::new(None);
                        break;
                    }
                },
                None => match D::open_all(Self::keys(), 1).into_iter().next() {
                    // 上下文为空，重试
                    Some(mut driver) => {
                        // 成功打开驱动
                        if !f(Connected(&mut *driver)) {
                            return;
                        } else {
                            *self.context() = Box::new(Some(*driver));
                            continue;
                        }
                    }
                    None => {
                        // 未能打开驱动
                        if !f(ConnectFailed) {
                            return;
                        }
                    }
                },
            }
        }
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
