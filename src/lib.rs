use async_std::task;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

mod indexer;
mod supervisor_multiple;
mod supervisor_single;

pub use indexer::Indexer;
pub use supervisor_multiple::{
    MultipleDeviceDriver, SupervisorEventForMultiple, SupervisorForMultiple,
};
pub use supervisor_single::{SupervisorEventForSingle, SupervisorForSingle};

/// 实现驱动特性，需要指定其对应的起搏器类型、状态类型和指令类型。
///
/// 通过 `T` 类型的键可以创建出驱动的实例。
///
/// 可以从驱动中读取状态，或向驱动发送指令。
///
/// 监听驱动事件是独占且阻塞的，但在传入的回调中可以向其中发送指令。
pub trait Driver: 'static + Send + Sized {
    type Pacemaker: DriverPacemaker + Send;
    type Key;
    type Event;

    fn keys() -> Vec<Self::Key>;
    fn open_timeout() -> Duration;

    fn new(t: &Self::Key) -> Option<(Self::Pacemaker, Self)>;

    /// 阻塞等待驱动退出
    ///
    /// 驱动可能因为两种原因退出：
    ///
    /// 1. 主动退出，方法返回 true
    /// 2. 内部错误（断开、超时或其他异常），方法返回 false
    ///
    /// 调用时传入回调函数 `f`。`f` 用于传出驱动对象的可变引用和驱动事件，返回 false 时驱动将主动退出。
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
        // 如果超时为 0，直接退出
        let open_timeout = Self::open_timeout();
        let deadline = if open_timeout != Duration::ZERO {
            Instant::now() + open_timeout
        } else {
            return drivers;
        };
        // 打开临时的监控以筛除不产生正确输出的设备
        let counter = Arc::new(()); // ---------------------- // 用一个 Arc 来计数
        let drivers = drivers
            .into_iter()
            .map(|(t, mut d)| {
                let counter = counter.clone();
                (
                    t,
                    task::spawn_blocking(move || {
                        if d.join(|_, _| {
                            Arc::strong_count(&counter) > len && Instant::now() < deadline
                        }) {
                            Some(d)
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
            .filter_map(|(t, o)| task::block_on(o).map(|b| (t, b)))
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
