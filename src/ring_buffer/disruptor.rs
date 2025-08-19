use std::{
    cell::UnsafeCell,
    error::Error,
    sync::{
        atomic::{AtomicIsize, AtomicUsize, Ordering},
        Arc,
    },
};

pub struct DisruptorRingBuffer<T> {
    buffer: UnsafeCell<Vec<T>>,
    size: isize,
}

impl<T> DisruptorRingBuffer<T> {
    pub fn new(size: isize) -> Result<Self, Box<dyn Error>> {
        if size > 0 && (size & (size - 1)) == 0 {
            let mut buffer = Vec::with_capacity(size as usize);
            unsafe {
                buffer.set_len(size as usize);
            }
            Ok(Self {
                buffer: UnsafeCell::new(buffer),
                size,
            })
        } else {
            Err("Size isn't a power of 2".into())
        }
    }
}

pub struct Sequence {
    sequence: isize,
}

impl Sequence {
    pub fn new() -> Self {
        Self { sequence: -1 }
    }

    pub fn get(&self) -> isize {
        self.sequence
    }

    pub fn set(&mut self, value: isize) {
        self.sequence = value
    }

    pub fn inc(&mut self) {
        self.sequence += 1
    }
}

pub struct AtomicSequence {
    sequence: AtomicIsize,
}

impl AtomicSequence {
    pub fn new() -> Self {
        Self {
            sequence: AtomicIsize::new(-1),
        }
    }

    pub fn get(&self, ordering: Ordering) -> isize {
        self.sequence.load(ordering)
    }

    pub fn set(&self, value: isize, ordering: Ordering) {
        self.sequence.store(value, ordering);
    }

    pub fn inc(&self, ordering: Ordering) {
        self.sequence.fetch_add(1, ordering);
    }
}

pub struct Disruptor<T> {
    pub cursor: AtomicUsize,
    pub sequence: Sequence,
    pub buffer: DisruptorRingBuffer<T>,
}

// == SPSC Disruptor ==
// Producer barrier
// single producer

pub struct RingBufferCore<T> {
    cursor: AtomicSequence,
    consumer_sequence: AtomicSequence,
    buffer: DisruptorRingBuffer<T>,
}

unsafe impl<T: Send> Sync for RingBufferCore<T> {}

pub struct Producer<T> {
    sequence: Sequence,
    ring_buffer_core: Arc<RingBufferCore<T>>,
}

impl<T> Producer<T> {
    pub fn claim_next_slot(&mut self) {
        self.sequence.inc();
    }

    pub fn write(&mut self, value: T) {
        self.claim_next_slot();
        // avoid wrapping
        // wait on consumer to catch up
        loop {
            let consumer_sequence = self
                .ring_buffer_core
                .consumer_sequence
                .get(Ordering::Relaxed);

            if consumer_sequence >= self.sequence.get() - self.ring_buffer_core.buffer.size {
                break;
            }
        }

        // commiting
        unsafe {
            let raw_pointer = &mut *self.ring_buffer_core.buffer.buffer.get();
            raw_pointer
                [self.sequence.get() as usize % self.ring_buffer_core.buffer.size as usize] = value;
        }

        println!(
            "pre publishing cursor: {}",
            self.ring_buffer_core.cursor.get(Ordering::Relaxed)
        );
        println!("sequence: {}", self.sequence.get());
        // publishing
        self.ring_buffer_core
            .cursor
            .set(self.sequence.get(), Ordering::Release);
        println!(
            "post publishing cursor: {}",
            self.ring_buffer_core.cursor.get(Ordering::Relaxed)
        );
    }
}

pub struct Consumer<T> {
    sequence: AtomicSequence,
    ring_buffer_core: Arc<RingBufferCore<T>>,
}

impl<T> Consumer<T> {
    pub fn read(&mut self) -> ReadValue<T> {
        // busy spin
        println!("Bussy spinning");
        let (cursor, sequence) = loop {
            let cursor = self.ring_buffer_core.cursor.get(Ordering::Acquire);
            // println!("cursor {}", cursor);
            let sequence = self.sequence.get(Ordering::Acquire);

            if cursor > sequence {
                break (cursor, sequence);
            }
        };

        let start = (sequence + 1) % self.ring_buffer_core.buffer.size;
        let end = cursor % self.ring_buffer_core.buffer.size;
        let read_value = if start > end {
            unsafe {
                let raw_pointer = &*self.ring_buffer_core.buffer.buffer.get();
                let slice_1 =
                    &raw_pointer[start as usize..self.ring_buffer_core.buffer.size as usize];
                let slice_2 = &raw_pointer[..end as usize];
                ReadValue::Wrapped((slice_1, slice_2))
            }
        } else {
            unsafe {
                let raw_pointer = &*self.ring_buffer_core.buffer.buffer.get();
                ReadValue::Straight(&raw_pointer[start as usize..=end as usize])
            }
        };

        self.sequence.set(cursor, Ordering::Relaxed);

        self.ring_buffer_core
            .consumer_sequence
            .set(self.sequence.get(Ordering::Relaxed), Ordering::Relaxed);

        return read_value;
    }
}

#[derive(Debug)]
pub enum ReadValue<T: 'static> {
    Straight(&'static [T]),
    Wrapped((&'static [T], &'static [T])),
}

pub fn spsc_disruptor<T: 'static + Send>(size: isize) -> (Producer<T>, Consumer<T>) {
    let core = Arc::new(RingBufferCore {
        cursor: AtomicSequence::new(),
        consumer_sequence: AtomicSequence::new(),
        buffer: DisruptorRingBuffer::new(size).unwrap(),
    });

    let producer = Producer {
        sequence: Sequence::new(),
        ring_buffer_core: Arc::clone(&core),
    };

    let consumer = Consumer::<T> {
        // Assuming the consumer doesn't need its own sequence based on our final design
        // If your Consumer struct has a sequence, you would initialize it here.
        sequence: AtomicSequence::new(),
        ring_buffer_core: Arc::clone(&core),
    };

    (producer, consumer)
}

// pub fn claim_next_sequence<T>(disruptor: &mut Disruptor<T>) -> usize {
//     let v = disruptor.sequence.get();
//     disruptor.sequence.inc();
//     v
// }

// pub fn write<T>(disruptor: &Disruptor<T>, value: T, claimed_sequence: usize, consumer: &Consumer) {
//     // avoid wrapping
//     // wait on consumer to catch up
//     loop {
//         let consumer_sequence = consumer.sequence.get(Ordering::Relaxed);

//         if consumer_sequence >= claimed_sequence - disruptor.buffer.size {
//             break;
//         }
//     }

//     // commiting
//     unsafe {
//         let raw_pointer = &mut *disruptor.buffer.buffer.get();
//         raw_pointer[claimed_sequence % disruptor.buffer.size] = value;
//     }
//     // publishing
//     disruptor.cursor.store(claimed_sequence, Ordering::Release);
// }

// Consumer barrier
// pub struct Consumer {
//     sequence: AtomicSequence,
// }

// impl Consumer {
//     pub fn new() -> Self {
//         Self {
//             sequence: AtomicSequence::new(),
//         }
//     }
// }

// pub fn read<T>(disruptor: &'static Disruptor<T>, consumer: &Consumer) -> ReadValue<T> {
//     // busy spin
//     let (cursor, sequence) = loop {
//         let cursor = disruptor.cursor.load(Ordering::Acquire) as usize;
//         let sequence = consumer.sequence.get(Ordering::Acquire) as usize;
//         if cursor > sequence {
//             break (cursor, sequence);
//         }
//     };
//     // read values in a batch
//     let start = (sequence + 1) % disruptor.buffer.size;
//     let end = cursor % disruptor.buffer.size;
//     let read_value = if start > end {
//         unsafe {
//             let raw_pointer = &mut *disruptor.buffer.buffer.get();
//             // special case where the we return two slices
//             let slice_1 = &raw_pointer[start..disruptor.buffer.size];
//             let slice_2 = &raw_pointer[..end];
//             ReadValue::Wrapped((slice_1, slice_2))
//         }
//     } else {
//         unsafe {
//             let raw_pointer = &mut *disruptor.buffer.buffer.get();
//             ReadValue::Straight(&raw_pointer[start..end])
//         }
//     };
//     consumer.sequence.set(cursor, Ordering::Relaxed);

//     return read_value;
// }
