use std::{cmp::Ordering::*, collections::BinaryHeap, ops::Range};

/// 为键排序并标号
pub struct Indexer<T> {
    pinned: Vec<Option<T>>,
    modified: FlagVec,
    waiting: BinaryHeap<T>,
    len: usize,
}

struct FlagVec(Vec<u8>);

impl<T> Indexer<T>
where
    T: Ord,
{
    pub fn new(capacity: usize) -> Self {
        let mut pinned = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            pinned.push(None);
        }
        Self {
            pinned,
            modified: FlagVec::with_capacity(capacity),
            waiting: Default::default(),
            len: 0,
        }
    }

    pub fn is_full(&self) -> bool {
        self.len == self.pinned.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn add(&mut self, t: T) -> Option<usize> {
        let tail = self.pinned.len() - 1;
        // 没有空位，检查 t 是否应该等待
        let mut hole = if self.is_full() {
            match t.cmp(self.get_value(tail)) {
                Less => {
                    // t 进入等待队列，无事发生
                    self.waiting.push(t);
                    return None;
                }
                Greater => {
                    // 最后一项进入等待队列，在 tail 产生一个空位
                    let item = std::mem::replace(self.get_mut(tail), None).unwrap();
                    self.waiting.push(item);
                    tail
                }
                Equal => panic!("不应该有两个 key 相同的驱动设备"),
            }
        }
        // 有空位，检查 t 是否在空位之后
        else {
            let mut i = tail;
            loop {
                match self.get(i) {
                    Some(it) => match t.cmp(it) {
                        Less => {
                            // t 已放在 i 处
                            self.put_somewhere_forward(i, t);
                            return Some(i);
                        }
                        Greater => i -= 1,
                        Equal => panic!("不应该有两个 key 相同的驱动设备"),
                    },
                    None => break i,
                }
            }
        };
        // 有空位，且 t 在最后一个空位之前
        let mut i = hole; // t 应在的位置
        while i > 0 {
            i -= 1;
            match self.get(i) {
                Some(ref item) => match t.cmp(item) {
                    Less => {
                        i += 1;
                        break;
                    }
                    Greater => {}
                    Equal => panic!("不应该有两个 key 相同的驱动设备"),
                },
                None => hole = i,
            }
        }
        // t 已放在 i 处
        self.put_backward(i..hole, t);
        Some(i)
    }

    pub fn remove(&mut self, t: &T) -> Option<usize> {
        let tail = self.pinned.len() - 1;
        for i in (0..=tail).rev() {
            if let Some(ref item) = self.get(i) {
                match t.cmp(item) {
                    Equal => {
                        return match self.waiting.pop() {
                            Some(t) => {
                                self.put_forward(i..tail, t);
                                None
                            }
                            None => {
                                self.remove_at(i);
                                Some(i)
                            }
                        };
                    }
                    Less => {
                        std::mem::replace(&mut self.waiting, Default::default())
                            .into_iter()
                            .filter(|it| t != it)
                            .for_each(|it| self.waiting.push(it));
                        return None;
                    }
                    Greater => {}
                }
            }
        }
        return None;
    }

    pub fn find(&self, t: &T) -> Option<usize> {
        for i in (0..self.pinned.len()).rev() {
            if let Some(ref item) = self.get(i) {
                match t.cmp(item) {
                    Less => return None,
                    Equal => return Some(i),
                    Greater => {}
                }
            }
        }
        None
    }

    pub fn update(&mut self, i: usize) -> bool {
        unsafe { self.modified.clear(i) }
    }

    #[inline]
    fn get_mut<'a>(&'a mut self, i: usize) -> &'a mut Option<T> {
        unsafe { self.pinned.get_unchecked_mut(i) }
    }

    #[inline]
    fn get<'a>(&'a self, i: usize) -> &'a Option<T> {
        unsafe { self.pinned.get_unchecked(i) }
    }

    #[inline]
    fn get_value<'a>(&'a self, i: usize) -> &'a T {
        self.get(i).as_ref().unwrap()
    }

    /// 将 i 以 t 填充
    #[inline]
    fn remove_at(&mut self, i: usize) {
        *self.get_mut(i) = None;
        unsafe { self.modified.clear(i) };
        self.len -= 1;
    }

    /// 将 t 填充到 i 并移动到找到一个空位
    /// 不知道空位在何处
    #[inline]
    fn put_somewhere_forward(&mut self, i: usize, mut t: T) {
        t = std::mem::replace(self.get_mut(i).as_mut().unwrap(), t);
        unsafe { self.modified.clear(i) };
        self.len += 1;
        for i in (0..i).rev() {
            unsafe { self.modified.set(i) };
            match self.get_mut(i) {
                Some(t_) => t = std::mem::replace(t_, t),
                None => {
                    *self.get_mut(i) = Some(t);
                    return;
                }
            }
        }
    }

    /// 将空位以 t 填充并移动到范围另一端
    /// range 的开头（包括）是空位
    #[inline]
    fn put_forward(&mut self, range: Range<usize>, t: T) {
        self.len += 1;
        *self.get_mut(range.start) = Some(t);
        unsafe { self.modified.clear(range.end) };
        for i in range {
            self.pinned.swap(i, i + 1);
            unsafe { self.modified.set(i) };
        }
    }

    /// 将空位以 t 填充并移动到范围另一端
    /// range 的末尾（不包括）是空位
    #[inline]
    fn put_backward(&mut self, range: Range<usize>, t: T) {
        self.len += 1;
        *self.get_mut(range.end) = Some(t);
        unsafe { self.modified.clear(range.start) };
        for i in range.rev() {
            self.pinned.swap(i, i + 1);
            unsafe { self.modified.set(i + 1) };
        }
    }
}

impl FlagVec {
    fn with_capacity(capacity: usize) -> Self {
        Self(vec![0; (capacity + 7) / 8])
    }

    unsafe fn set(&mut self, i: usize) {
        *self.0.get_unchecked_mut(i / 8) |= 1 << (i % 8);
    }

    unsafe fn clear(&mut self, i: usize) -> bool {
        let block = self.0.get_unchecked_mut(i / 8);
        let mask = 1 << (i % 8);
        let result = (*block & mask) != 0;
        *block &= !mask;
        result
    }
}

#[cfg(test)]
mod t {
    use super::*;

    #[inline]
    fn vec_modified<T: Ord>(indexer: &Indexer<T>) -> Vec<bool> {
        let mut result = vec![false; indexer.pinned.len()];
        for i in 0..indexer.pinned.len() {
            if (indexer.modified.0[i / 8] & (1 << (i % 8))) != 0 {
                result[i] = true;
            }
        }
        result
    }

    #[inline]
    fn vec_waiting<T: Ord + Copy>(indexer: &Indexer<T>) -> Vec<T> {
        let mut x = indexer.waiting.iter().map(|r| *r).collect::<Vec<_>>();
        x.sort();
        x
    }

    #[test]
    fn test() {
        // 初始化
        let mut indexer = Indexer::<i32>::new(5);
        assert_eq!(indexer.pinned, vec![None, None, None, None, None]);
        assert_eq!(indexer.len, 0);
        // 从空插入
        assert_eq!(indexer.add(6), Some(0));
        assert_eq!(indexer.pinned, vec![Some(6), None, None, None, None]);
        assert_eq!(
            vec_modified(&indexer),
            vec![false, false, false, false, false]
        );
        assert_eq!(indexer.len(), 1);
        assert!(!indexer.is_full());
        // 排序插入
        assert_eq!(indexer.add(3), Some(1));
        assert_eq!(indexer.add(2), Some(2));
        assert_eq!(indexer.add(7), Some(0));
        assert_eq!(indexer.add(4), Some(2));
        assert_eq!(
            indexer.pinned,
            vec![Some(7), Some(6), Some(4), Some(3), Some(2)]
        );
        assert_eq!(vec_modified(&indexer), vec![false, true, false, true, true]);
        assert_eq!(indexer.len(), 5);
        assert!(indexer.is_full());
        // 移除
        indexer.remove(&7);
        indexer.remove(&3);
        assert_eq!(indexer.pinned, vec![None, Some(6), Some(4), None, Some(2)]);
        assert_eq!(
            vec_modified(&indexer),
            vec![false, true, false, false, true]
        );
        // 推
        indexer.add(5);
        assert_eq!(
            indexer.pinned,
            vec![None, Some(6), Some(5), Some(4), Some(2)]
        );
        assert_eq!(vec_modified(&indexer), vec![false, true, false, true, true]);
        indexer.add(1);
        assert_eq!(
            indexer.pinned,
            vec![Some(6), Some(5), Some(4), Some(2), Some(1)]
        );
        assert_eq!(vec_modified(&indexer), vec![true, true, true, true, false]);
        // 更新
        assert!(indexer.update(0));
        assert!(indexer.update(1));
        assert!(indexer.update(3));
        assert!(!indexer.update(3));
        assert_eq!(
            vec_modified(&indexer),
            vec![false, false, true, false, false]
        );
        // 直接排队
        assert_eq!(indexer.add(0), None);
        assert_eq!(
            indexer.pinned,
            vec![Some(6), Some(5), Some(4), Some(2), Some(1)]
        );
        assert_eq!(vec_waiting(&indexer), vec![0]);
        assert_eq!(
            vec_modified(&indexer),
            vec![false, false, true, false, false]
        );
        // 替换排队
        assert_eq!(indexer.add(3), Some(3));
        assert_eq!(
            indexer.pinned,
            vec![Some(6), Some(5), Some(4), Some(3), Some(2)]
        );
        assert_eq!(vec_waiting(&indexer), vec![0, 1]);
        assert_eq!(
            vec_modified(&indexer),
            vec![false, false, true, false, true]
        );
        // 补充
        indexer.remove(&5);
        assert_eq!(
            indexer.pinned,
            vec![Some(6), Some(4), Some(3), Some(2), Some(1)]
        );
        assert_eq!(vec_waiting(&indexer), vec![0]);
        assert_eq!(vec_modified(&indexer), vec![false, true, true, true, false]);
        // 查找
        assert_eq!(indexer.find(&7), None);
        assert_eq!(indexer.find(&6), Some(0));
        assert_eq!(indexer.find(&4), Some(1));
        assert_eq!(indexer.find(&3), Some(2));
        assert_eq!(indexer.find(&2), Some(3));
        assert_eq!(indexer.find(&1), Some(4));
        assert_eq!(indexer.find(&0), None);
    }
}
