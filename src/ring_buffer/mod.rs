pub mod disruptor;
pub mod simple;

pub trait RingBuffer<T> {
    fn new(capacity: usize) -> Self;
    fn push(&mut self, item :T) -> Result<(), T>;
    fn pop(&mut self) -> Option<T>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;    
}
