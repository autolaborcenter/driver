use std::{
    hash::Hash,
    marker::PhantomData,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

mod supervisor_multiple;

use supervisor_multiple::JoinContextForMultiple;

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

    fn keys() -> Vec<T>;
    fn open_timeout() -> Duration;

    fn new(t: &T) -> Option<(Self::Pacemaker, Self)>;
    fn status<'a>(&'a self) -> &'a Self::Status;
    fn send(&mut self, command: (Instant, Self::Command));
    fn join<F>(&mut self, f: F) -> bool
    where
        F: FnMut(&mut Self, Option<(Instant, <Self::Status as DriverStatus>::Event)>) -> bool;

    fn open_all<I>(keys: I, len: usize, timeout: Duration) -> Vec<(T, Box<Self>)>
    where
        I: IntoIterator<Item = T>,
    {
        let mut pacemakers = Vec::new();
        let mut drivers = Vec::new();
        keys.into_iter()
            .filter_map(|t| Self::new(&t).map(|pair| (t, pair)))
            .for_each(|(t, (p, d))| {
                pacemakers.push(Box::new(p));
                drivers.push((t, Box::new(d)));
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
            let deadline = Instant::now() + timeout;
            let counter = Arc::new(());
            drivers
                .into_iter()
                .map(|(t, mut o)| {
                    let counter = counter.clone();
                    (
                        t,
                        thread::spawn(move || {
                            if o.join(|_, _| {
                                Arc::strong_count(&counter) > len && Instant::now() < deadline
                            }) {
                                Some(o)
                            } else {
                                None
                            }
                        }),
                    )
                })
                .collect::<Vec<_>>()
        }
        .into_iter()
        .filter_map(|(t, o)| o.join().ok().flatten().map(|b| (t, b)))
        .collect()
    }
}

/// 状态的增量是事件。
///
/// 也可以通过累积事件来跟踪状态。
pub trait DriverStatus: 'static {
    type Event: Send;

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

pub struct SupervisorForSingle<T, D: Driver<T>>(Box<Option<D>>, PhantomData<T>);

pub enum SupersivorEventForSingle<'a, T, D: Driver<T>> {
    Connected(T, &'a mut D),
    ConnectFailed,
    Event(
        &'a mut D,
        Option<(Instant, <D::Status as DriverStatus>::Event)>,
    ),
    Disconnected,
}

impl<T, D: Driver<T>> SupervisorForSingle<T, D> {
    pub fn new() -> Self {
        Self(Box::new(None), PhantomData)
    }

    pub fn join<F>(&mut self, mut f: F)
    where
        F: FnMut(SupersivorEventForSingle<T, D>) -> bool,
    {
        loop {
            use SupersivorEventForSingle::*;

            match self.0.as_mut() {
                Some(ref mut driver) => loop {
                    // 上下文中保存了驱动
                    if driver.join(|d, e| f(Event(d, e))) || !f(Disconnected) {
                        // 驱动退出阻塞或断联后不希望重试
                        return;
                    } else {
                        // 清除上下文，重试
                        self.0 = Box::new(None);
                        break;
                    }
                },
                None => match D::open_all(D::keys(), 1, D::open_timeout())
                    .into_iter()
                    .next()
                {
                    // 上下文为空，重试
                    Some((t, mut driver)) => {
                        // 成功打开驱动
                        if !f(Connected(t, &mut *driver)) {
                            return;
                        } else {
                            self.0 = Box::new(Some(*driver));
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

pub struct SupervisorForMultiple<K, D>(Vec<(K, Box<D>)>, PhantomData<K>)
where
    K: 'static + Send + Clone + Eq + Hash,
    D: Driver<K>;

pub enum SupervisorEventForMultiple<'a, T, D>
where
    T: Clone,
    D: Driver<T>,
{
    Connected(&'a T, &'a mut D),
    ConnectFailed {
        current: usize,
        target: usize,
        begining: Instant,
    },
    Event(T, Option<(Instant, <D::Status as DriverStatus>::Event)>),
    Disconnected(T),
}

impl<K, D> SupervisorForMultiple<K, D>
where
    K: 'static + Send + Clone + Eq + Hash,
    D: Driver<K>,
{
    pub fn new() -> Self {
        Self(Vec::new(), PhantomData)
    }

    pub fn join<F>(&mut self, len: usize, f: F)
    where
        F: FnMut(SupervisorEventForMultiple<K, D>) -> bool,
    {
        JoinContextForMultiple::new(self, len, f).run();
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
