use std::{cell::UnsafeCell, error::Error, sync::atomic::{fence, AtomicUsize, Ordering}};


pub struct RingBuffer<T> {
    buffer: UnsafeCell<Vec<T>>,
    size: usize
}

impl<T> RingBuffer<T> {
    pub fn new(size: usize) -> Result<Self, Box<dyn Error>> {
        if size > 0 && (size & (size - 1)) == 0 {
            Ok(Self {
                buffer: UnsafeCell::new(Vec::with_capacity(size)),
                size
            })
        } else {
            Err("Size isn't a power of 2".into())
        }
    }
}

pub struct Disruptor<T> {
    cursor: AtomicUsize,
    sequence: usize,
    buffer: RingBuffer<T>
}

// == SPSC Disruptor == 
// Producer barrier
// single producer

pub fn claim_next_sequence<T>(disruptor: &mut Disruptor<T>) -> usize {
    let v = disruptor.sequence;
    disruptor.sequence += 1;
    v
}

pub fn write<T>(disruptor: &Disruptor<T>, value: T, claimed_sequence: usize, consumer: &Consumer) {
    // avoid wrapping
    // wait on consumer to catch up
    loop {
        let consumer_sequence = consumer.sequence.load(Ordering::Relaxed);
        
        if consumer_sequence >= claimed_sequence - disruptor.buffer.size  {
            break;
        }
    };
    
    // commiting
    unsafe {
        let raw_pointer = &mut *disruptor.buffer.buffer.get();
        raw_pointer[claimed_sequence % disruptor.buffer.size] = value;
    }
    // publishing
    disruptor.cursor.store(claimed_sequence, Ordering::Release);
}

// Consumer barrier
pub struct Consumer {
    sequence: AtomicUsize
}

pub enum ReadValue<T: 'static> {
    Straight(&'static [T]),
    Wrapped((&'static [T],&'static [T]))
}


pub fn read<T>(disruptor: &'static Disruptor<T>, consumer: &Consumer) -> ReadValue<T> {
    // busy spin
    let (cursor, sequence) = loop {
        let cursor = disruptor.cursor.load(Ordering::Acquire) as usize;
        let sequence = consumer.sequence.load(Ordering::Acquire) as usize;
        if cursor > sequence {
            break (cursor, sequence);
        }
    };
    // read values in a batch
    let start = (sequence + 1) % disruptor.buffer.size;
    let end = cursor % disruptor.buffer.size;
    if start > end {
        unsafe {
            let raw_pointer = &mut *disruptor.buffer.buffer.get();
            // special case where the we return two slices
            let slice_1 = &raw_pointer[start..disruptor.buffer.size];
            let slice_2 = &raw_pointer[..end];
            consumer.sequence.store(cursor, Ordering::Relaxed);
            ReadValue::Wrapped((slice_1, slice_2))
        }
    } else {
        unsafe {
            let raw_pointer = &mut *disruptor.buffer.buffer.get();
            ReadValue::Straight(&raw_pointer[start..end])
        }
    }
}

