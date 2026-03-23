
//! Lock-free multi-producer single-consumer return queue (ABA-safe).
//!
//! Treiber stack with tagged pointer to prevent ABA.
//! Producers push via CAS; single consumer drains via swap(null).
//!
//! Tag is incremented on each push, so pointer reuse cannot fool CAS.

use std::marker::PhantomData;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::trace;

// ── Tagged pointer layout ─────────────────────────────────────────────────────
// [ upper 16 bits: tag ][ lower 48 bits: pointer ]
// Assumes x86_64 canonical addresses (48-bit VA).

const PTR_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
const TAG_SHIFT: u64 = 48;

#[inline(always)]
fn pack<T>(ptr: *mut Node<T>, tag: u64) -> u64 {
	((tag << TAG_SHIFT) & !PTR_MASK) | ((ptr as u64) & PTR_MASK)
}

#[inline(always)]
fn unpack<T>(data: u64) -> (*mut Node<T>, u64) {
	let ptr = (data & PTR_MASK) as *mut Node<T>;
	let tag = data >> TAG_SHIFT;
	(ptr, tag)
}

// ── Cache-line alignment ─────────────────────────────────────────────────────

#[repr(align(64))]
#[derive(Default)]
struct CacheAligned<T>(T);

// ── Node ─────────────────────────────────────────────────────────────────────

struct Node<T> {
	value: T,
	next: *mut Node<T>,
}

// ── ReturnQueue ──────────────────────────────────────────────────────────────
#[derive(Default)]
pub struct ReturnQueue<T: Send> {
	head: CacheAligned<AtomicU64>,
	_marker: PhantomData<T>,
}

unsafe impl<T: Send> Send for ReturnQueue<T> {}
unsafe impl<T: Send> Sync for ReturnQueue<T> {}

impl<T: Send> ReturnQueue<T> {
	pub fn new() -> Self {
		Self {
			head: CacheAligned(AtomicU64::new(pack::<T>(ptr::null_mut(), 0))),
			_marker: PhantomData,
		}
	}
	
	/// Push a value. Lock-free via CAS. ABA-safe via tag.
	///
	/// No trace here — this is the hot path called from every SubAllocation
	/// drop on any thread. Tracing would add atomic loads (subscriber check)
	/// inside the CAS retry loop.
	pub fn push(&self, value: T) {
		let node = Box::into_raw(Box::new(Node {
			value,
			next: ptr::null_mut(),
		}));
		
		loop {
			let old = self.head.0.load(Ordering::Relaxed);
			let (old_ptr, tag) = unpack::<T>(old);
			
			unsafe { (*node).next = old_ptr; }
			
			let new = pack(node, tag.wrapping_add(1));
			
			if self
				.head
				.0
				.compare_exchange_weak(old, new, Ordering::Release, Ordering::Relaxed)
				.is_ok()
			{
				break;
			}
		}
	}
	
	/// Drain all pending items (single consumer).
	pub fn drain(&self) -> Drain<T> {
		let old = self
			.head
			.0
			.swap(pack::<T>(ptr::null_mut(), 0), Ordering::Acquire);
		
		let (ptr, tag) = unpack::<T>(old);
		
		if !ptr.is_null() {
			trace!(
                  tag,
                  empty = false,
                  "ReturnQueue::drain — swapped head"
              );
		}
		
		Drain { current: ptr }
	}
	
	pub fn is_empty(&self) -> bool {
		let (ptr, _) = unpack::<T>(self.head.0.load(Ordering::Relaxed));
		ptr.is_null()
	}
}

impl<T: Send> Drop for ReturnQueue<T> {
	fn drop(&mut self) {
		let data = *self.head.0.get_mut();
		let (mut current, _) = unpack::<T>(data);
		
		let mut count = 0u32;
		while !current.is_null() {
			unsafe {
				let node = Box::from_raw(current);
				current = node.next;
				count += 1;
			}
		}
		
		if count > 0 {
			trace!(count, "ReturnQueue::drop — cleaned up remaining nodes");
		}
	}
}

// ── Drain iterator ───────────────────────────────────────────────────────────

pub struct Drain<T> {
	current: *mut Node<T>,
}

impl<T> Iterator for Drain<T> {
	type Item = T;
	
	fn next(&mut self) -> Option<T> {
		if self.current.is_null() {
			return None;
		}
		
		unsafe {
			let node = Box::from_raw(self.current);
			self.current = node.next;
			Some(node.value)
		}
	}
}

impl<T> Drop for Drain<T> {
	fn drop(&mut self) {
		while self.next().is_some() {}
	}
}

unsafe impl<T: Send> Send for Drain<T> {}

impl<T: Send> std::fmt::Debug for ReturnQueue<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let (ptr, tag) = unpack::<T>(self.head.0.load(std::sync::atomic::Ordering::Relaxed));
		f.debug_struct("ReturnQueue")
		 .field("empty", &ptr.is_null())
		 .field("tag", &tag)
		 .finish()
	}
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Arc;
	use std::thread;
	
	#[test]
	fn push_and_drain() {
		let q = ReturnQueue::new();
		q.push(1u32);
		q.push(2);
		q.push(3);
		
		let items: Vec<_> = q.drain().collect();
		assert_eq!(items, vec![3, 2, 1]);
		
		assert!(q.is_empty());
		assert_eq!(q.drain().count(), 0);
	}
	
	#[test]
	fn concurrent_push() {
		let q = Arc::new(ReturnQueue::new());
		let per_thread = 1000;
		
		let handles: Vec<_> = (0..4)
			.map(|t| {
				let q = Arc::clone(&q);
				thread::spawn(move || {
					for i in 0..per_thread {
						q.push(t * per_thread + i);
					}
				})
			})
			.collect();
		
		for h in handles {
			h.join().unwrap();
		}
		
		let mut items: Vec<_> = q.drain().collect();
		assert_eq!(items.len(), 4000);
		
		items.sort();
		items.dedup();
		assert_eq!(items.len(), 4000);
	}
	
	#[test]
	fn drain_partial_then_drop() {
		let q = ReturnQueue::new();
		q.push(1u32);
		q.push(2);
		q.push(3);
		
		let mut drain = q.drain();
		drain.next();
		drop(drain);
		
		assert!(q.is_empty());
	}
	
	#[test]
	fn drop_cleans_up_heap_values() {
		let q = ReturnQueue::new();
		q.push(Box::new(42));
		q.push(Box::new(43));
		drop(q);
	}
	
	#[test]
	fn interleaved_push_drain() {
		let q = ReturnQueue::new();
		q.push(1u32);
		q.push(2);
		
		let batch1: Vec<_> = q.drain().collect();
		assert_eq!(batch1.len(), 2);
		
		q.push(3);
		let batch2: Vec<_> = q.drain().collect();
		assert_eq!(batch2, vec![3]);
		
		assert!(q.is_empty());
	}
}