use std::{
    cmp::Ordering::*,
    collections::{BinaryHeap, HashSet},
    ops::Range,
};

/// 为键排序并标号
pub struct Indexer<T> {
    pinned: Vec<Option<T>>,
    waiting: BinaryHeap<T>,
    modified: HashSet<usize>,
    len: usize,
}

impl<T> Indexer<T>
where
    T: Ord,
{
    pub fn new(capacity: usize) -> Self {
        if capacity < 2 {
            panic!("排序器至少有 2 个容量");
        }

        let mut pinned = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            pinned.push(None);
        }
        Self {
            pinned,
            waiting: Default::default(),
            modified: Default::default(),
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
                            // 找到下一个空位
                            let hole = self.find_hole(i);
                            // t 已放在 i 处
                            self.put_forward(hole..i, t);
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

    pub fn remove(&mut self, t: T) {
        let tail = self.pinned.len() - 1;
        for i in (0..=tail).rev() {
            if let Some(ref item) = self.get(i) {
                match t.cmp(item) {
                    Equal => {
                        match self.waiting.pop() {
                            Some(t) => self.put_forward(i..tail, t),
                            None => self.remove_at(i),
                        };
                        break;
                    }
                    Less => {
                        std::mem::replace(&mut self.waiting, Default::default())
                            .into_iter()
                            .filter(|it| t != *it)
                            .for_each(|it| self.waiting.push(it));
                        break;
                    }
                    Greater => {}
                }
            }
        }
    }

    pub fn find(&self, t: &T) -> Option<usize> {
        for i in (0..self.pinned.len()).rev() {
            if let Some(ref item) = self.get(i) {
                match t.cmp(item) {
                    Less => {}
                    Equal => return Some(i),
                    Greater => return None,
                }
            }
        }
        None
    }

    pub fn update(&mut self, i: usize) -> bool {
        self.modified.remove(&i)
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
        self.modified.remove(&i);
        self.len -= 1;
    }

    /// 将空位以 t 填充并移动到范围另一端
    /// range 的开头（包括）是空位
    #[inline]
    fn put_forward(&mut self, range: Range<usize>, t: T) {
        self.len += 1;
        *self.get_mut(range.start) = Some(t);
        self.modified.remove(&range.end);
        for i in range {
            self.pinned.swap(i, i + 1);
            self.modified.insert(i);
        }
    }

    /// 将空位以 t 填充并移动到范围另一端
    /// range 的末尾（不包括）是空位
    #[inline]
    fn put_backward(&mut self, range: Range<usize>, t: T) {
        self.len += 1;
        *self.get_mut(range.end) = Some(t);
        self.modified.remove(&range.start);
        for i in range.rev() {
            self.pinned.swap(i, i + 1);
            self.modified.insert(i + 1);
        }
    }

    /// 找下一个空位
    #[inline]
    fn find_hole(&mut self, mut i: usize) -> usize {
        while i > 0 {
            i -= 1;
            if self.get(i).is_none() {
                return i;
            }
        }
        return self.pinned.len();
    }
}

#[cfg(test)]
mod t {
    use super::*;

    #[inline]
    fn vec_modified<T: Ord>(indexer: &Indexer<T>) -> Vec<usize> {
        let mut x = indexer.modified.iter().map(|r| *r).collect::<Vec<_>>();
        x.sort();
        x
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
        assert_eq!(indexer.len(), 1);
        assert!(!indexer.is_full());
        assert!(indexer.modified.is_empty());
        // 排序插入
        assert_eq!(indexer.add(3), Some(1));
        assert_eq!(indexer.add(2), Some(2));
        assert_eq!(indexer.add(7), Some(0));
        assert_eq!(indexer.add(4), Some(2));
        assert_eq!(
            indexer.pinned,
            vec![Some(7), Some(6), Some(4), Some(3), Some(2)]
        );
        assert_eq!(vec_modified(&indexer), vec![1, 3, 4]);
        assert_eq!(indexer.len(), 5);
        assert!(indexer.is_full());
        // 移除
        indexer.remove(7);
        indexer.remove(3);
        assert_eq!(indexer.pinned, vec![None, Some(6), Some(4), None, Some(2)]);
        assert_eq!(vec_modified(&indexer), vec![1, 4]);
        // 推
        indexer.add(5);
        assert_eq!(
            indexer.pinned,
            vec![None, Some(6), Some(5), Some(4), Some(2)]
        );
        assert_eq!(vec_modified(&indexer), vec![1, 3, 4]);
        indexer.add(1);
        assert_eq!(
            indexer.pinned,
            vec![Some(6), Some(5), Some(4), Some(2), Some(1)]
        );
        assert_eq!(vec_modified(&indexer), vec![0, 1, 2, 3]);
        // 更新
        assert!(indexer.update(0));
        assert!(indexer.update(1));
        assert!(indexer.update(3));
        assert!(!indexer.update(3));
        assert_eq!(vec_modified(&indexer), vec![2]);
        // 直接排队
        assert_eq!(indexer.add(0), None);
        assert_eq!(
            indexer.pinned,
            vec![Some(6), Some(5), Some(4), Some(2), Some(1)]
        );
        assert_eq!(vec_waiting(&indexer), vec![0]);
        assert_eq!(vec_modified(&indexer), vec![2]);
        // 替换排队
        assert_eq!(indexer.add(3), Some(3));
        assert_eq!(
            indexer.pinned,
            vec![Some(6), Some(5), Some(4), Some(3), Some(2)]
        );
        assert_eq!(vec_waiting(&indexer), vec![0, 1]);
        assert_eq!(vec_modified(&indexer), vec![2, 4]);
        // 补充
        indexer.remove(5);
        assert_eq!(
            indexer.pinned,
            vec![Some(6), Some(4), Some(3), Some(2), Some(1)]
        );
        assert_eq!(vec_waiting(&indexer), vec![0]);
        assert_eq!(vec_modified(&indexer), vec![1, 2, 3]);
    }
}
