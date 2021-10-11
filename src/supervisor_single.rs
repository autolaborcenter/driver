use super::Driver;
use std::time::Instant;

pub struct SupervisorForSingle<D: Driver>(Box<Option<D>>);

pub enum SupersivorEventForSingle<'a, D: Driver> {
    Connected(<D as Driver>::Key, &'a mut D),
    ConnectFailed,
    Event(&'a mut D, Option<(Instant, D::Event)>),
    Disconnected,
}

impl<D: Driver> SupervisorForSingle<D> {
    pub fn new() -> Self {
        Self(Box::new(None))
    }

    pub fn join<F>(&mut self, mut f: F)
    where
        F: FnMut(SupersivorEventForSingle<D>) -> bool,
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
                None => match D::open_some(1).into_iter().next() {
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
