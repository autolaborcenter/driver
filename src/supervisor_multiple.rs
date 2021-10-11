use super::{Driver, DriverStatus, SupervisorEventForMultiple, SupervisorForMultiple};
use std::{
    collections::HashMap,
    hash::Hash,
    sync::mpsc::*,
    thread::{self, JoinHandle},
    time::Instant,
};

pub(super) struct JoinContextForMultiple<'a, K, D, F>
where
    K: 'static + Send + Clone + Eq + Hash,
    D: Driver<K>,
    F: FnMut(SupervisorEventForMultiple<K, D>) -> bool,
{
    parent: &'a mut SupervisorForMultiple<K, D>,
    handles: HashMap<K, JoinHandle<Option<(K, Box<D>)>>>,
    sender: SyncSender<OutEvent<K, D>>,
    receiver: Receiver<OutEvent<K, D>>,
    len: usize,
    f: F,
}

impl<'a, K, D, F> JoinContextForMultiple<'a, K, D, F>
where
    K: 'static + Send + Clone + Eq + Hash,
    D: Driver<K>,
    F: FnMut(SupervisorEventForMultiple<K, D>) -> bool,
{
    pub fn new(parent: &'a mut SupervisorForMultiple<K, D>, len: usize, f: F) -> Self {
        let (sender, receiver) = sync_channel(2 * len);

        // 取出上下文中保存的驱动对象
        let handles = std::mem::replace(&mut parent.0, Vec::new())
            .into_iter()
            .map(|(k, d)| (k.clone(), spawn(sender.clone(), k, d)))
            .collect::<HashMap<_, _>>();

        Self {
            parent,
            handles,
            sender,
            receiver,
            len,
            f,
        }
    }

    pub fn run(mut self)
    where
        F: FnMut(SupervisorEventForMultiple<K, D>) -> bool,
    {
        use SupervisorEventForMultiple::*;

        // 尽量接收驱动的消息
        while (&mut self).receive() {
            // 设备数量不足时，尝试打开一些新的设备
            let begining = Instant::now();
            let new = D::open_all(D::keys(), self.len - self.handles.len(), D::open_timeout());
            if new.is_empty() {
                // 没能打开任何设备，报告
                if !(self.f)(ConnectFailed {
                    current: self.handles.len(),
                    target: self.len,
                    begining,
                }) {
                    break;
                }
            } else {
                // 打开了一些设备，报告
                // 所有已打开的设备都要保存到上下文
                if !new.into_iter().fold(true, |sum, (k, mut d)| {
                    if sum && (self.f)(Connected(&k, &mut d)) {
                        self.handles
                            .insert(k.clone(), spawn(self.sender.clone(), k, d));
                        true
                    } else {
                        self.parent.0.push((k, d));
                        false
                    }
                }) {
                    break;
                }
            }
        }

        // 结束所有线程，回收驱动对象并保存到上下文
        std::mem::drop(self.receiver);
        self.parent.0.extend(
            self.handles
                .into_iter()
                .filter_map(|(_, handle)| handle.join().ok().flatten()),
        );
    }

    /// 从线程中接收消息
    fn receive(&mut self) -> bool {
        use SupervisorEventForMultiple::*;

        let mut wait = self.handles.len() >= self.len;
        loop {
            let event = if wait {
                // 当前足够多设备在线，等待所有消息
                match self.receiver.recv() {
                    Ok(e) => e,
                    Err(_) => panic!("Impossible!"),
                }
            } else {
                // 接收已有消息，没有消息立即退出
                match self.receiver.try_recv() {
                    Ok(e) => e,
                    Err(TryRecvError::Empty) => return true,
                    Err(TryRecvError::Disconnected) => panic!("Impossible!"),
                }
            };
            match event {
                // 一般事件
                OutEvent::Event(which, what) => {
                    if !(self.f)(Event(which, what)) {
                        return false;
                    }
                }
                // 有设备断连，检查设备数是否少于目标
                OutEvent::Disconnected(which) => {
                    self.handles.remove(&which);
                    if !(self.f)(Disconnected(which)) {
                        return false;
                    }
                    wait = self.handles.len() >= self.len;
                }
            }
        }
    }
}

enum OutEvent<T, D: Driver<T>> {
    Event(T, Option<(Instant, <D::Status as DriverStatus>::Event)>),
    Disconnected(T),
}

fn spawn<K, D>(
    sender: SyncSender<OutEvent<K, D>>,
    k: K,
    mut d: Box<D>,
) -> JoinHandle<Option<(K, Box<D>)>>
where
    K: 'static + Send + Clone,
    D: Driver<K>,
{
    thread::spawn(move || {
        if d.join(|_, event| sender.send(OutEvent::Event(k.clone(), event)).is_ok()) {
            Some((k, d))
        } else {
            let _ = sender.send(OutEvent::Disconnected(k));
            None
        }
    })
}
