use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// A generic fixed-capacity circular ring buffer that automatically evicts the oldest items when full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RingBuffer<T> {
    buffer: VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// Creates a new `RingBuffer` with the specified maximum capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
        }
    }

    /// Appends an item to the back of the buffer, evicting the oldest item if capacity is reached.
    pub fn push(&mut self, item: T) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    /// Extends the buffer with multiple items, respecting maximum capacity.
    pub fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push(item);
        }
    }

    /// Number of items currently in the buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns `true` if the buffer contains no elements.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Maximum capacity of the ring buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clears all elements from the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Returns an iterator over the elements in the buffer (oldest to newest).
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, T> {
        self.buffer.iter()
    }

    /// Returns a vector of cloned elements in chronological order.
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.buffer.iter().cloned().collect()
    }

    /// Returns a reference to the newest element added.
    pub fn latest(&self) -> Option<&T> {
        self.buffer.back()
    }

    /// Returns a reference to the oldest element currently retained.
    pub fn oldest(&self) -> Option<&T> {
        self.buffer.front()
    }

    /// Dynamically resizes the maximum capacity, truncating oldest elements if necessary.
    pub fn set_capacity(&mut self, new_capacity: usize) {
        self.capacity = new_capacity.max(1);
        while self.buffer.len() > self.capacity {
            self.buffer.pop_front();
        }
    }
}

impl<T> Default for RingBuffer<T> {
    fn default() -> Self {
        Self::new(64)
    }
}

/// Appends an item to a `Vec` and removes elements from the front if `len > max_len`.
pub fn vec_push_limited<T>(vec: &mut Vec<T>, item: T, max_len: usize) {
    vec.push(item);
    if vec.len() > max_len {
        let drain_count = vec.len() - max_len;
        vec.drain(0..drain_count);
    }
}

impl<T> IntoIterator for RingBuffer<T> {
    type Item = T;
    type IntoIter = std::collections::vec_deque::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.buffer.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a RingBuffer<T> {
    type Item = &'a T;
    type IntoIter = std::collections::vec_deque::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.buffer.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_capacity_eviction() {
        let mut rb = RingBuffer::new(3);
        assert_eq!(rb.capacity(), 3);
        assert!(rb.is_empty());

        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.len(), 3);
        assert_eq!(rb.oldest(), Some(&1));
        assert_eq!(rb.latest(), Some(&3));
        assert_eq!(rb.to_vec(), vec![1, 2, 3]);

        // Push 4: 1 is evicted
        rb.push(4);
        assert_eq!(rb.len(), 3);
        assert_eq!(rb.oldest(), Some(&2));
        assert_eq!(rb.latest(), Some(&4));
        assert_eq!(rb.to_vec(), vec![2, 3, 4]);

        // Clear
        rb.clear();
        assert!(rb.is_empty());
        assert_eq!(rb.len(), 0);
    }

    #[test]
    fn test_ring_buffer_set_capacity() {
        let mut rb = RingBuffer::new(5);
        for i in 1..=5 {
            rb.push(i);
        }
        assert_eq!(rb.len(), 5);

        // Shrink to 2
        rb.set_capacity(2);
        assert_eq!(rb.capacity(), 2);
        assert_eq!(rb.len(), 2);
        assert_eq!(rb.to_vec(), vec![4, 5]);
    }

    #[test]
    fn test_vec_push_limited() {
        let mut v = vec![1, 2, 3];
        vec_push_limited(&mut v, 4, 3);
        assert_eq!(v, vec![2, 3, 4]);

        vec_push_limited(&mut v, 5, 2);
        assert_eq!(v, vec![4, 5]);
    }
}
