use super::{SupervisorEventForMultiple, SupervisorForMultiple};
use crate::Driver;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::mpsc::*,
    thread::{self, JoinHandle},
    time::Instant,
};

pub(super) struct JoinContextForMultiple<'a, D: Driver, F> {
    parent: &'a mut SupervisorForMultiple<D>,
    handles: HashMap<<D as Driver>::Key, JoinHandle<Option<(<D as Driver>::Key, Box<D>)>>>,
    sender: SyncSender<OutEvent<D>>,
    receiver: Receiver<OutEvent<D>>,
    target_len: usize,
    f: F,
}

impl<'a, D, F> JoinContextForMultiple<'a, D, F>
where
    D: Driver,
    D::Key: Send + Clone + Eq + Hash,
    D::Event: Send,
    F: FnMut(SupervisorEventForMultiple<D>) -> usize,
{
    pub fn new(parent: &'a mut SupervisorForMultiple<D>, len: usize, f: F) -> Self {
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
            target_len: len,
            f,
        }
    }

    pub fn run(mut self) {
        use SupervisorEventForMultiple::*;

        // 尽量接收驱动的消息
        while self.target_len > 0 {
            // 设备数量不足时，尝试打开一些新的设备
            let new = D::open_some(self.target_len - self.handles.len());
            if new.is_empty() {
                // 没能打开任何设备，报告
                self.target_len = (self.f)(ConnectFailed {
                    current: self.handles.len(),
                    target: self.target_len,
                });
            } else {
                // 打开了一些设备，报告
                // 所有已打开的设备都要保存到上下文
                for (k, mut d) in new.into_iter() {
                    if self.target_len > 0 {
                        self.target_len = (self.f)(Connected(&k, &mut d));
                    }
                    if self.target_len > 0 {
                        self.handles
                            .insert(k.clone(), spawn(self.sender.clone(), k, d));
                    } else {
                        self.parent.0.push((k, d));
                    }
                }
            }
            self.receive_from_child();
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
    fn receive_from_child(&mut self) {
        use SupervisorEventForMultiple::*;

        while self.target_len > 0 {
            let event = if self.handles.len() >= self.target_len {
                // 当前足够多设备在线，等待所有消息
                match self.receiver.recv() {
                    Ok(e) => e,
                    Err(_) => panic!("Impossible!"),
                }
            } else {
                // 接收已有消息，没有消息立即退出
                match self.receiver.try_recv() {
                    Ok(e) => e,
                    Err(TryRecvError::Empty) => return,
                    Err(TryRecvError::Disconnected) => panic!("Impossible!"),
                }
            };
            self.target_len = match event {
                // 一般事件
                OutEvent::Event(which, what) => (self.f)(Event(which, what)),
                // 有设备断连
                OutEvent::Disconnected(which) => {
                    self.handles.remove(&which);
                    (self.f)(Disconnected(which))
                }
            }
        }
    }
}

enum OutEvent<D: Driver> {
    Event(D::Key, Option<(Instant, D::Event)>),
    Disconnected(D::Key),
}

fn spawn<D: Driver>(
    sender: SyncSender<OutEvent<D>>,
    k: D::Key,
    mut d: Box<D>,
) -> JoinHandle<Option<(D::Key, Box<D>)>>
where
    D::Key: Send + Clone,
    D::Event: Send,
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
