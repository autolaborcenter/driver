use super::Driver;
use std::time::Instant;

pub struct SupervisorForSingle<D: Driver>(Option<Box<D>>);

pub enum SupersivorEventForSingle<'a, D: Driver> {
    Connected(<D as Driver>::Key, &'a mut D),
    ConnectFailed,
    Event(&'a mut D, Option<(Instant, D::Event)>),
    Disconnected,
}

impl<D: Driver> SupervisorForSingle<D> {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn join<F>(&mut self, mut f: F)
    where
        F: FnMut(SupersivorEventForSingle<D>) -> bool,
    {
        loop {
            // 取出上下文中保存的驱动
            if let Some(mut driver) = self.0.take() {
                // 驱动主动退出，立即释放
                // 或
                // 驱动断联后不希望再次尝试
                if driver.join(|d, e| f(Event(d, e))) || !f(Disconnected) {
                    return;
                }
            }
            // 上下文中驱动已取出
            use SupersivorEventForSingle::*;
            match D::open_some(1).into_iter().next() {
                Some((t, mut driver)) => {
                    // 成功打开驱动
                    if !f(Connected(t, &mut *driver)) {
                        return;
                    } else {
                        self.0 = Some(Box::new(*driver));
                        continue;
                    }
                }
                None => {
                    // 未能打开驱动
                    if !f(ConnectFailed) {
                        return;
                    }
                }
            }
        }
    }
}
