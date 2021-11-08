use async_std::task;
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

mod indexer;
mod supervisor_multiple;
mod supervisor_single;

pub use indexer::Indexer;
pub use supervisor_multiple::{SupervisorEventForMultiple, SupervisorForMultiple};
pub use supervisor_single::{SupersivorEventForSingle, SupervisorForSingle};

/// 实现驱动特性，需要指定其对应的起搏器类型、状态类型和指令类型。
///
/// 通过 `T` 类型的键可以创建出驱动的实例。
///
/// 可以从驱动中读取状态，或向驱动发送指令。
///
/// 监听驱动事件是独占且阻塞的，但在传入的回调中可以修改驱动状态。
pub trait Driver: 'static + Send + Sized {
    type Key;
    type Pacemaker: DriverPacemaker + Send;
    type Event;
    type Command;

    fn keys() -> Vec<Self::Key>;
    fn open_timeout() -> Duration;

    fn new(t: &Self::Key) -> Option<(Self::Pacemaker, Self)>;
    fn send(&mut self, command: Self::Command);
    fn join<F>(&mut self, f: F) -> bool
    where
        F: FnMut(&mut Self, Option<(Instant, Self::Event)>) -> bool;

    fn open_some(len: usize) -> Vec<(Self::Key, Box<Self>)> {
        // 打开所有可能的驱动并启动起搏器
        // 这段的耗时不计入超时
        let drivers: Vec<_> = Self::keys()
            .into_iter()
            .filter_map(|t| {
                Self::new(&t).map(|(mut p, d)| {
                    task::spawn(async move {
                        let period = Self::Pacemaker::period();
                        while p.send() {
                            task::sleep(period).await;
                        }
                    });
                    (t, Box::new(d))
                })
            })
            .collect();

        // 打开临时的监控以筛除不产生正确输出的设备
        // 用一个 Arc 来计数
        let counter = Arc::new(());
        let deadline = Instant::now() + Self::open_timeout();
        let drivers = drivers
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
            .collect::<Vec<_>>();
        std::mem::drop(counter); // 丢弃外面的引用，此后引用计数 === 存活的驱动数

        // 收集正确打开的驱动
        drivers
            .into_iter()
            .filter_map(|(t, o)| o.join().ok().flatten().map(|b| (t, b)))
            .collect()
    }
}

/// 起搏器有一个静态不变的周期。
///
/// 应该根据这个周期定时发送触发脉冲。
pub trait DriverPacemaker {
    /// 发送周期
    fn period() -> Duration;

    /// 发送一个触发脉冲，返回是否需要继续发送
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
