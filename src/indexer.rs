use std::cmp::Ordering;

/// 为键排序并标号
pub struct Indexer<T>(Vec<T>, usize);

impl<T> Indexer<T>
where
    T: Ord,
{
    pub fn new(capacity: usize) -> Self {
        Self(Vec::new(), capacity)
    }

    pub fn is_full(&self) -> bool {
        self.0.len() == self.1
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn add(&mut self, k: T) -> Option<usize> {
        let mut i = 0;
        while i < self.0.len() {
            match k.cmp(&self.0[i]) {
                Ordering::Less => i += 1,
                Ordering::Equal => panic!("Impossible"),
                Ordering::Greater => {
                    if self.0.len() == self.1 {
                        self.0.pop();
                    }
                    self.0.insert(i, k);
                    return Some(i);
                }
            }
        }
        if self.0.len() < self.1 {
            self.0.push(k);
            Some(i)
        } else {
            None
        }
    }

    pub fn remove(&mut self, k: T) -> Option<usize> {
        match self.find(&k) {
            Some(i) => {
                self.0.remove(i);
                Some(i)
            }
            None => None,
        }
    }

    pub fn find(&self, k: &T) -> Option<usize> {
        for i in 0..self.0.len() {
            match k.cmp(&self.0[i]) {
                Ordering::Less => {}
                Ordering::Equal => return Some(i),
                Ordering::Greater => return None,
            }
        }
        None
    }
}
