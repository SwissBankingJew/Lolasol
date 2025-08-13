use super::RingBuffer;
use std::collections::VecDeque;

pub struct SimpleRingBuffer<T> {
    buffer: VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> for SimpleRingBuffer<T> {
    fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn push(&mut self, item :T) -> Result<(), T> {
        if self.buffer.len() >= self.capacity {
            Err(item)
        } else {
            self.buffer.push_back(item);
            Ok(())
        }
    }

    fn pop(&mut self) -> Option<T> {
        self.buffer.pop_front()
    }

    fn len(&self) -> usize {
        self.buffer.len()
    }

    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}
