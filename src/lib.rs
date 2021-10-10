use std::{
    collections::HashMap,
    hash::Hash,
    sync::{atomic::AtomicBool, mpsc::*, Arc},
    thread::{self, JoinHandle},
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

pub enum SupersivorEventForSingle<'a, T, D: Driver<T>> {
    Connected(T, &'a mut D),
    ConnectFailed,
    Event(
        &'a mut D,
        Option<(Instant, <D::Status as DriverStatus>::Event)>,
    ),
    Disconnected,
}

pub trait SupervisorForSingle<T, D: Driver<T>> {
    fn context<'a>(&'a mut self) -> &'a mut Box<Option<D>>;
    fn open_timeout() -> Duration;
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
                None => match D::open_all(Self::keys(), 1, Self::open_timeout())
                    .into_iter()
                    .next()
                {
                    // 上下文为空，重试
                    Some((t, mut driver)) => {
                        // 成功打开驱动
                        if !f(Connected(t, &mut *driver)) {
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

pub enum SupersivorEventForMultiple<'a, T, D>
where
    T: Clone + Hash,
    D: Driver<T>,
{
    Connected(T, &'a mut D),
    ConnectFailed {
        current: usize,
        target: usize,
        begining: Instant,
    },
    Event(T, Option<(Instant, <D::Status as DriverStatus>::Event)>),
    Disconnected(T),
}

pub trait SupervisorForMultiple<T, D>
where
    T: 'static + Send + Clone + Eq + Hash,
    D: Driver<T>,
{
    fn context<'a>(&'a mut self) -> &'a mut HashMap<T, Box<D>>;
    fn open_timeout() -> Duration;
    fn keys() -> Vec<T>;

    fn join<F>(&mut self, len: usize, mut f: F)
    where
        F: FnMut(SupersivorEventForMultiple<T, D>) -> bool,
    {
        use SupersivorEventForMultiple::*;

        let (sender, receiver) = sync_channel(2 * len);
        let running = Arc::new(AtomicBool::new(true));
        let context = std::mem::replace(self.context(), HashMap::new());
        let mut handles = context
            .into_iter()
            .map(|(t, d)| (t.clone(), spawn(sender.clone(), running.clone(), t, d)))
            .collect::<HashMap<_, _>>();

        let mut _loop = true;
        while _loop {
            let begining = Instant::now();
            while handles.len() < len {
                let new = D::open_all(Self::keys(), len - handles.len(), Self::open_timeout());
                if new.is_empty() {
                    if !f(ConnectFailed {
                        current: handles.len(),
                        target: len,
                        begining,
                    }) {
                        _loop = false;
                        break;
                    }
                } else {
                    for (t, mut d) in new.into_iter() {
                        if _loop && !f(Connected(t.clone(), &mut d)) {
                            _loop = false;
                        }
                        handles.insert(t.clone(), spawn(sender.clone(), running.clone(), t, d));
                    }
                }
            }
            if _loop {
                for event in &receiver {
                    match event {
                        OutEvent::Event(which, what) => {
                            if !f(Event(which, what)) {
                                _loop = false;
                                break;
                            }
                        }
                        OutEvent::Disconnected(which) => {
                            handles.remove(&which);
                            if !f(Disconnected(which)) {
                                _loop = false;
                                break;
                            }
                        }
                    }
                }
            }
        }
        running.store(false, std::sync::atomic::Ordering::Release);
        self.context().extend(
            handles
                .into_iter()
                .filter_map(|(_, handle)| handle.join().ok().flatten()),
        );
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

enum OutEvent<T, D: Driver<T>> {
    Event(T, Option<(Instant, <D::Status as DriverStatus>::Event)>),
    Disconnected(T),
}

fn spawn<T, D>(
    sender: SyncSender<OutEvent<T, D>>,
    running: Arc<AtomicBool>,
    t: T,
    mut d: Box<D>,
) -> JoinHandle<Option<(T, Box<D>)>>
where
    T: 'static + Send + Clone,
    D: Driver<T>,
{
    thread::spawn(move || {
        if d.join(|_, event| {
            let _ = sender.send(OutEvent::Event(t.clone(), event));
            running.load(std::sync::atomic::Ordering::Acquire)
        }) {
            Some((t, d))
        } else {
            let _ = sender.send(OutEvent::Disconnected(t));
            None
        }
    })
}
