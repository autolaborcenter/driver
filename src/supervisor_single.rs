use super::Driver;
use std::time::Instant;

/// 控制一个驱动程序的监控器
pub struct SupervisorForSingle<D>(Option<Box<D>>);

/// 监控一个驱动程序时产生的事件
pub enum SupervisorEventForSingle<'a, D: Driver> {
    /// 成功连接到驱动程序
    Connected(<D as Driver>::Key, &'a mut D),
    /// 监听到驱动程序事件
    Event(&'a mut D, Option<(Instant, D::Event)>),
    /// 断开连接
    Disconnected,
    /// 尝试连接但失败
    ConnectFailed,
}

impl<D> Default for SupervisorForSingle<D> {
    /// 产生一个空的监控器
    #[inline]
    fn default() -> Self {
        Self(None)
    }
}

impl<D> From<Box<D>> for SupervisorForSingle<D> {
    /// 监控传入的驱动程序 `d`
    #[inline]
    fn from(d: Box<D>) -> Self {
        Self(Some(d))
    }
}

impl<D: Driver> SupervisorForSingle<D> {
    /// 取出监控器中保存的驱动对象，取出后监控器为空
    #[inline]
    pub fn take(&mut self) -> Option<Box<D>> {
        self.0.take()
    }

    /// 使用监控器监控驱动程序
    pub fn join<F>(&mut self, mut f: F)
    where
        F: FnMut(SupervisorEventForSingle<D>) -> bool,
    {
        loop {
            use SupervisorEventForSingle::*;
            // 取出上下文中保存的驱动
            if let Some(mut driver) = self.0.take() {
                // 驱动主动退出，保存并连锁退出
                if driver.join(|d, e| f(Event(d, e))) {
                    self.0 = Some(driver);
                    return;
                }
                // 驱动断联后不希望再次尝试
                if !f(Disconnected) {
                    return;
                }
            }
            // 上下文中驱动已取出
            match D::open_some(1).into_iter().next() {
                // 成功打开驱动，保存
                Some((t, driver)) => {
                    self.0 = Some(driver);
                    if !f(Connected(t, self.0.as_mut().unwrap())) {
                        return;
                    }
                }
                // 未能打开驱动
                None => {
                    if !f(ConnectFailed) {
                        return;
                    }
                }
            }
        }
    }
}
