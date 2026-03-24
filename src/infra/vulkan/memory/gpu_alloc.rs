// src/infra/vulkan/memory/gpu_allocator.rs
//!
//! # GpuAllocator
//!
//! Global factory that manages per-thread sub-allocators backed by Vulkan
//! device memory.
//!
//! ## Architecture
//!
//! ```text
//!  ┌──────────────────────────────────────────────────────────────┐
//!  │  GpuAllocator  (Arc, shared across threads)                  │
//!  │  ├── BlockFactory  (trait — impl'd by VulkanDevice)          │
//!  │  └── free_channel: Arc<ReturnQueue<DeviceMemory>>  (MPSC)    │
//!  └──────────────────────────────────────────────────────────────┘
//!                              ▲
//!   Thread A ──── LocalArena ──┤  push on reap()  (lock-free CAS)
//!   Thread B ──── LocalArena ──┘
//!
//!   Main thread ── flush_device_frees() ── drain() ── BlockFactory::free_block()
//! ```
//!
//! ## Lock-free guarantee
//!
//! Every hot-path operation is free of locks and system calls:
//!
//! | Operation | Mechanism |
//! |-----------|-----------|
//! | Sub-allocation (fast path) | `RefCell` borrow on thread-local — zero contention |
//! | Cross-thread `SubAllocation` drop | Lock-free Treiber-stack `ReturnQueue<FreeRequest>` |
//! | Emptied block → render thread | Lock-free Treiber-stack `ReturnQueue<DeviceMemory>` |
//! | New Vulkan block (slow path) | `vkAllocateMemory` — once per ~64 MiB, not per frame |
//! | `vkFreeMemory` | Called only from `flush_device_frees`, single-threaded |
//!
//! The `Mutex` that was here previously has been removed. `FreeChannel` is now
//! an alias for `ReturnQueue<vk::DeviceMemory>` — the same data structure
//! already tested and used for `FreeRequest` routing in `suballoc.rs`.
//!
//! ## BlockFactory and VulkanDevice
//!
//! `BlockFactory` is a narrow two-method trait (allocate / free one Vulkan
//! block). It exists purely as a testability seam: `MockFactory` implements
//! it in tests without needing a real device. In production, `VulkanDevice`
//! implements `BlockFactory` directly — `allocate_memory` and `free_memory`
//! are already present on `VulkanDevice` via `DeviceOps<VulkanBackend>`, so
//! the impl is a one-line delegation per method with no duplicated logic.
//!
//! There is **no** separate `VulkanBlockFactory` wrapper struct. The
//! `GpuAllocator::new` constructor takes `Arc<VulkanDevice>` directly.


use std::cell::RefCell;
use std::sync::Arc;
use crate::core::Backend;
use crate::domain::ResourceKind;
use super::suballoc::{ThreadArena, SubAllocation};
use super::return_q::ReturnQueue;
use crate::infra::vulkan::backend::VulkanDevice;
use crate::infra::VulkanBackend;
use crate::core::types::{MemoryPropertyFlags, MemoryRequirements};
use tracing::{debug, info, trace, warn};

// ── Block sizes ──────────────────────────────────────────────────────────────
pub const DEFAULT_BUFFER_BLOCK_SIZE: u64 =  128 * 1024 * 1024;

pub const DEFAULT_IMAGE_BLOCK_SIZE: u64 =  1024 * 1024 ;

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationError {
	/// No Vulkan memory type satisfied the requested property flags.
	NoSuitableMemoryType,
	/// The Vulkan device returned an error from `vkAllocateMemory`.
	DeviceOom(<VulkanBackend as Backend>::Error),
	/// The allocation is larger than the configured block size. Either use a
	/// larger block size or implement the dedicated allocation path
	/// (spec principle 13).
	TooLargeForArena { size: u64, block_size: u64 },
}
impl std::fmt::Display for AllocationError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			AllocationError::NoSuitableMemoryType => {
				write!(f, "no suitable memory type found for the requested property flags")
			}
			AllocationError::DeviceOom(e) => {
				write!(f, "device out of memory: {e}")
			}
			AllocationError::TooLargeForArena { size, block_size } => {
				write!(f, "allocation size {size} exceeds block size {block_size}")
			}
		}
	}
}

impl std::error::Error for AllocationError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			AllocationError::DeviceOom(e) => Some(e),
			_ => None,
		}
	}
}
// ── BlockFactory ─────────────────────────────────────────────────────────────

pub trait BlockFactory: Send + Sync {
	fn allocate_block(
		&self,
		size: u64,
		memory_type_index: u32,
	) -> Result<(<VulkanBackend as Backend>::Buffer, <VulkanBackend as Backend>::DeviceMemory, u64), <VulkanBackend as
	Backend>::Error>;
	
	fn free_block(
		&self,
		buffer: <VulkanBackend as Backend>::Buffer,
		memory: <VulkanBackend as Backend>::DeviceMemory,
	);
}


// ── LocalArena ───────────────────────────────────────────────────────────────

struct LocalArena {
	arena:   ThreadArena<VulkanBackend>,
	channel: Arc<
		ReturnQueue<(
			<VulkanBackend as Backend>::Buffer,
			<VulkanBackend as Backend>::DeviceMemory
		)>
	>
}

impl LocalArena {
	fn new(
		channel: Arc<
			ReturnQueue<(
				<VulkanBackend as Backend>::Buffer,
				<VulkanBackend as Backend>::DeviceMemory
			)>
		>
	) -> Self {
		trace!("LocalArena created for thread {:?}", std::thread::current().id());
		Self { arena: ThreadArena::new(), channel }
	}
	
	fn reap(&mut self, gpu_t: u64) {
		self.arena.reap(gpu_t);
		let freed_count = self.arena.pending_device_frees.len();
		if freed_count > 0 {
			debug!(
                  gpu_t,
                  freed_blocks = freed_count,
                  thread = ?std::thread::current().id(),
                  "LocalArena pushing emptied blocks to free channel"
              );
		}
		for (buffer, memory) in self.arena.pending_device_frees.drain(..) {
			self.channel.push((buffer, memory));
		}
	}
}

impl Drop for LocalArena {
	fn drop(&mut self) {
		debug!(
              thread = ?std::thread::current().id(),
              "LocalArena dropping — final reap"
          );
		self.arena.reap(u64::MAX);
		let freed_count = self.arena.pending_device_frees.len();
		if freed_count > 0 {
			debug!(freed_blocks = freed_count, "Final reap freed blocks");
		}
		for mem in self.arena.pending_device_frees.drain(..) {
			self.channel.push(mem);
		}
		
		debug_assert!(
			self.arena.return_queue_is_empty(),
			"Thread exiting with live SubAllocations — Vulkan sub-allocations will be leaked"
		);
	}
}

// ── Thread-local storage ─────────────────────────────────────────────────────

thread_local! {
      static THREAD_ARENA: RefCell<Option<LocalArena>> = const { RefCell::new(None) };
  }

// ── GpuAllocator ─────────────────────────────────────────────────────────────

pub struct GpuAllocator {
	factory:           Arc<dyn BlockFactory>,
	memory_properties: ash::vk::PhysicalDeviceMemoryProperties,
	buffer_block_size: u64,
	image_block_size:  u64,
	free_channel: Arc<ReturnQueue<(<VulkanBackend as Backend>::Buffer, <VulkanBackend as Backend>::DeviceMemory)>>,
}

impl GpuAllocator {
	// ── Constructors ──────────────────────────────────────────────────────
	
	pub fn new(
		device:            Arc<VulkanDevice>,
		memory_properties: ash::vk::PhysicalDeviceMemoryProperties,
	) -> Arc<Self> {
		info!(
              buffer_block_size = DEFAULT_BUFFER_BLOCK_SIZE,
              image_block_size = DEFAULT_IMAGE_BLOCK_SIZE,
              memory_type_count = memory_properties.memory_type_count,
              "GpuAllocator::new — creating with default block sizes"
          );
		Self::with_factory(
			device,
			memory_properties,
			DEFAULT_BUFFER_BLOCK_SIZE,
			DEFAULT_IMAGE_BLOCK_SIZE,
		)
	}
	
	pub fn new_with_block_sizes(
		device:            Arc<VulkanDevice>,
		memory_properties: ash::vk::PhysicalDeviceMemoryProperties,
		buffer_block_size: u64,
		image_block_size:  u64,
	) -> Arc<Self> {
		info!(
              buffer_block_size,
              image_block_size,
              memory_type_count = memory_properties.memory_type_count,
              "GpuAllocator::new_with_block_sizes"
          );
		Self::with_factory(device, memory_properties, buffer_block_size, image_block_size)
	}
	
	pub fn with_factory(
		factory:           Arc<dyn BlockFactory>,
		memory_properties: ash::vk::PhysicalDeviceMemoryProperties,
		buffer_block_size: u64,
		image_block_size:  u64,
	) -> Arc<Self> {
		debug!(
              buffer_block_size,
              image_block_size,
              "GpuAllocator::with_factory"
          );
		Arc::new(Self {
			factory,
			memory_properties,
			buffer_block_size,
			image_block_size,
			free_channel: Arc::new(ReturnQueue::new()),
		})
	}
	
	// ── Public API ────────────────────────────────────────────────────────
	
	pub fn allocate(
		&self,
		requirements:   MemoryRequirements,
		property_flags: MemoryPropertyFlags,
		kind:           ResourceKind,
		gpu_t:          u64,
	) -> Result<SubAllocation<VulkanBackend>, AllocationError> {
		trace!(
              size = requirements.size,
              alignment = requirements.alignment,
              type_bits = requirements.memory_type_bits,
              ?kind,
              gpu_t,
              "GpuAllocator::allocate — looking up memory type"
          );
		
		let memory_type_index = self
			.find_memory_type(requirements.memory_type_bits, property_flags)
			.ok_or_else(|| {
				warn!(
                      type_bits = requirements.memory_type_bits,
                      ?property_flags,
                      "No suitable memory type found"
                  );
				AllocationError::NoSuitableMemoryType
			})?;
		
		trace!(memory_type_index, "Memory type resolved");
		
		self.allocate_raw(
			requirements.size,
			requirements.alignment,
			memory_type_index,
			kind,
			gpu_t,
		)
	}
	
	pub fn allocate_raw(
		&self,
		size:              u64,
		align:             u64,
		memory_type_index: u32,
		kind:              ResourceKind,
		gpu_t:             u64,
	) -> Result<SubAllocation<VulkanBackend>, AllocationError> {
		let block_size = self.block_size_for(kind);
		
		trace!(
              size,
              align,
              memory_type_index,
              ?kind,
              gpu_t,
              block_size,
              "GpuAllocator::allocate_raw"
          );
		
		// Spec principle 13: dedicated allocation for oversized requests.
		if size > block_size {
			warn!(
                  size,
                  block_size,
                  ?kind,
                  "Allocation too large for arena"
              );
			return Err(AllocationError::TooLargeForArena { size, block_size });
		}
		
		self.ensure_thread_arena();
		
		// Spec principle 14: reap before allocating so that completed frees
		// are visible to the arena before we consider growing a new block.
		THREAD_ARENA.with(|cell| {
			cell.borrow_mut().as_mut().unwrap().reap(gpu_t);
		});
		
		// ── Fast path: existing block has room ────────────────────────────
		let fast = THREAD_ARENA.with(|cell| {
			cell.borrow_mut()
				.as_mut()
				.unwrap()
				.arena
				.allocate(size, align, kind, gpu_t)
		});
		if let Some(sub) = fast {
			trace!(
                  size,
                  align,
                  ?kind,
                  "Fast path — allocated from existing block"
              );
			return Ok(sub);
		}
		
		// ── Slow path: request a new Vulkan block ─────────────────────────
		debug!(
              block_size,
              memory_type_index,
              ?kind,
              thread = ?std::thread::current().id(),
              "Slow path — requesting new Vulkan block (vkAllocateMemory)"
          );
		
		let t0 = std::time::Instant::now();
		let (buffer, mem, actual_size) = self
			.factory
			.allocate_block(block_size, memory_type_index)
			.map_err(|e| {
				warn!(
                      block_size,
                      memory_type_index,
                      error = ?e,
                      "vkAllocateMemory failed"
                  );
				AllocationError::DeviceOom(e)
			})?;
		let alloc_ms = t0.elapsed().as_millis();
		
		debug!(
              actual_size,
              alloc_ms,
              ?kind,
              "New Vulkan block allocated"
          );
		if alloc_ms > 5 {
			warn!(alloc_ms, block_size, "SLOW vkAllocateMemory");
		}
		
		THREAD_ARENA.with(|cell| {
			cell.borrow_mut()
				.as_mut()
				.unwrap()
				.arena
				.inject_new_block(buffer, mem, actual_size, kind);
		});
		
		// Retry. No reap between inject and allocate — calling reap here would
		// destroy the freshly-injected empty block before we can allocate from it.
		let result = THREAD_ARENA.with(|cell| {
			cell.borrow_mut()
				.as_mut()
				.unwrap()
				.arena
				.allocate(size, align, kind, gpu_t)
		});
		
		match result {
			Some(sub) => {
				trace!(size, align, ?kind, "Slow path retry — success");
				Ok(sub)
			}
			None => {
				warn!(
                      size,
                      align,
                      block_size,
                      ?kind,
                      "Slow path retry failed — OOM after fresh block inject"
                  );
				Err(AllocationError::DeviceOom(
					<VulkanBackend as Backend>::Error::ERROR_OUT_OF_DEVICE_MEMORY,
				))
			}
		}
	}
	
	pub fn reap(&self, gpu_t: u64) {
		trace!(gpu_t, "GpuAllocator::reap");
		self.ensure_thread_arena();
		THREAD_ARENA.with(|cell| {
			cell.borrow_mut().as_mut().unwrap().reap(gpu_t);
		});
	}
	
	pub fn flush_device_frees(&self) {
		let drained: Vec<_> = self.free_channel.drain().collect();
		let count = drained.len();
		if count > 0 {
			debug!(count, "flush_device_frees — calling vkFreeMemory");
		}
		let t0 = std::time::Instant::now();
		for (buffer, memory) in drained {
			self.factory.free_block(buffer, memory);
		}
		if count > 0 {
			let free_ms = t0.elapsed().as_millis();
			trace!(count, free_ms, "flush_device_frees — done");
			if free_ms > 2 {
				warn!(count, free_ms, "SLOW flush_device_frees");
			}
		}
	}
	
	pub fn find_memory_type(
		&self,
		type_filter:    u32,
		required_flags: MemoryPropertyFlags,
	) -> Option<u32> {
		let result = (0..self.memory_properties.memory_type_count).find(|&i| {
			let mt = self.memory_properties.memory_types[i as usize];
			(type_filter & (1 << i)) != 0
				&& mt.property_flags.contains(required_flags.into())
		});
		trace!(
              type_filter,
              ?required_flags,
              result = ?result,
              "find_memory_type"
          );
		result
	}
	
	// ── Accessors ─────────────────────────────────────────────────────────
	
	pub fn buffer_block_size(&self) -> u64 { self.buffer_block_size }
	pub fn image_block_size(&self)  -> u64 { self.image_block_size  }
	
	pub fn pending_device_frees_is_empty(&self) -> bool {
		self.free_channel.is_empty()
	}
	
	// ── Private ───────────────────────────────────────────────────────────
	
	fn block_size_for(&self, kind: ResourceKind) -> u64 {
		match kind {
			ResourceKind::Buffer => self.buffer_block_size,
			ResourceKind::Image  => self.image_block_size,
		}
	}
	
	fn ensure_thread_arena(&self) {
		let channel = Arc::clone(&self.free_channel);
		THREAD_ARENA.with(move |cell| {
			let mut guard = cell.borrow_mut();
			if guard.is_none() {
				debug!(
                      thread = ?std::thread::current().id(),
                      "Initializing thread-local arena"
                  );
				*guard = Some(LocalArena::new(channel));
			}
		});
	}
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
	use std::thread;
	use ash::vk::Handle;
	
	// ── MockFactory ───────────────────────────────────────────────────────
	
	struct MockFactory {
		next_id:     AtomicU64,
		freed_count: AtomicU64,
		inject_oom:  AtomicBool,
	}
	
	impl MockFactory {
		fn new() -> Arc<Self> {
			Arc::new(Self {
				next_id:     AtomicU64::new(1),
				freed_count: AtomicU64::new(0),
				inject_oom:  AtomicBool::new(false),
			})
		}
	}
	
	impl BlockFactory for MockFactory {
		fn allocate_block(
			&self,
			size: u64,
			_memory_type_index: u32,
		) -> Result<
			(
				<VulkanBackend as Backend>::Buffer,
				<VulkanBackend as Backend>::DeviceMemory,
				u64
			),
			<VulkanBackend as Backend>::Error
		> {
			if self.inject_oom.load(Ordering::Relaxed) {
				return Err(<VulkanBackend as Backend>::Error::ERROR_OUT_OF_DEVICE_MEMORY);
			}
			
			let id = self.next_id.fetch_add(1, Ordering::Relaxed);
			
			let buffer = <VulkanBackend as Backend>::Buffer::from_raw(id);
			let memory = <VulkanBackend as Backend>::DeviceMemory::from_raw(id + 1);
			
			Ok((buffer, memory, size))
		}
		
		fn free_block(
			&self,
			_buffer: <VulkanBackend as Backend>::Buffer,
			_memory: <VulkanBackend as Backend>::DeviceMemory,
		) {
			self.freed_count.fetch_add(1, Ordering::Relaxed);
		}
	}
	
	fn test_allocator(block_size: u64) -> (Arc<GpuAllocator>, Arc<MockFactory>) {
		let factory = MockFactory::new();
		let alloc = GpuAllocator::with_factory(
			Arc::clone(&factory) as Arc<dyn BlockFactory>,
			ash::vk::PhysicalDeviceMemoryProperties::default(),
			block_size,
			block_size,
		);
		(alloc, factory)
	}
	
	// ── Basic lifecycle ───────────────────────────────────────────────────
	
	#[test]
	fn test_basic_allocate_reap_flush() {
		let (alloc, factory) = test_allocator(1024 * 1024);
		
		let mut sub = alloc
			.allocate_raw(256, 16, 0, ResourceKind::Buffer, 0)
			.expect("allocation must succeed");
		
		assert_eq!(factory.next_id.load(Ordering::Relaxed), 2);
		
		sub.finalize_lifetime(1);
		drop(sub);
		
		alloc.reap(1);
		alloc.flush_device_frees();
		
		assert_eq!(factory.freed_count.load(Ordering::Relaxed), 1);
	}
	
	#[test]
	fn test_fast_path_reuses_block() {
		let (alloc, factory) = test_allocator(1024 * 1024);
		
		let mut subs: Vec<SubAllocation<VulkanBackend>> = (0..100)
			.map(|_| alloc.allocate_raw(1024, 16, 0, ResourceKind::Buffer, 0).unwrap())
			.collect();
		
		assert_eq!(factory.next_id.load(Ordering::Relaxed), 2,
				   "Only one vkAllocateMemory call expected");
		
		for s in subs.iter_mut() { s.finalize_lifetime(1); }
		drop(subs);
		alloc.reap(1);
		alloc.flush_device_frees();
	}
	
	#[test]
	fn test_slow_path_new_block() {
		let (alloc, factory) = test_allocator(1024);
		
		let mut subs: Vec<SubAllocation<VulkanBackend>> = (0..200)
			.filter_map(|_| alloc.allocate_raw(16, 16, 0, ResourceKind::Buffer, 0).ok())
			.collect();
		
		assert!(factory.next_id.load(Ordering::Relaxed) >= 3,
				"Expected at least 2 vkAllocateMemory calls");
		
		for s in subs.iter_mut() { s.finalize_lifetime(1); }
		drop(subs);
		alloc.reap(1);
		alloc.flush_device_frees();
		assert!(alloc.pending_device_frees_is_empty());
	}
	
	#[test]
	fn test_oom_propagates() {
		let (alloc, factory) = test_allocator(64);
		
		let mut s1 = alloc.allocate_raw(64, 1, 0, ResourceKind::Buffer, 0).unwrap();
		factory.inject_oom.store(true, Ordering::Relaxed);
		
		let err = alloc.allocate_raw(64, 1, 0, ResourceKind::Buffer, 0).unwrap_err();
		assert!(matches!(err, AllocationError::DeviceOom(_)));
		
		s1.finalize_lifetime(1);
		drop(s1);
		factory.inject_oom.store(false, Ordering::Relaxed);
		alloc.reap(1);
		alloc.flush_device_frees();
	}
	
	#[test]
	fn test_too_large_returns_error() {
		let (alloc, _) = test_allocator(1024);
		let err = alloc.allocate_raw(2048, 1, 0, ResourceKind::Buffer, 0).unwrap_err();
		assert!(matches!(
              err,
              AllocationError::TooLargeForArena { size: 2048, block_size: 1024 }
          ));
	}
	
	// ── Thread-local isolation ────────────────────────────────────────────
	
	#[test]
	fn test_thread_local_isolation() {
		let (alloc, factory) = test_allocator(512 * 1024);
		
		let handles: Vec<_> = (0..2).map(|_| {
			let alloc = Arc::clone(&alloc);
			thread::spawn(move || {
				let mut subs: Vec<SubAllocation<VulkanBackend>> = (0..100)
					.map(|_| alloc.allocate_raw(1024, 16, 0, ResourceKind::Buffer, 1).unwrap())
					.collect();
				for s in subs.iter_mut() { s.finalize_lifetime(1); }
				drop(subs);
				alloc.reap(1);
			})
		}).collect();
		
		for h in handles { h.join().unwrap(); }
		
		assert!(factory.next_id.load(Ordering::Relaxed) >= 3,
				"Two threads must have allocated independent blocks");
		
		alloc.flush_device_frees();
		assert!(alloc.pending_device_frees_is_empty());
		assert!(factory.freed_count.load(Ordering::Relaxed) >= 2);
	}
	
	#[test]
	fn test_cross_thread_drop() {
		let (alloc, factory) = test_allocator(512 * 1024);
		
		let mut subs: Vec<SubAllocation<VulkanBackend>> = (0..500)
			.map(|_| alloc.allocate_raw(256, 16, 0, ResourceKind::Buffer, 0).unwrap())
			.collect();
		for s in subs.iter_mut() { s.finalize_lifetime(1); }
		
		let chunk_size = subs.len() / 5;
		let mut chunks: Vec<Vec<SubAllocation<VulkanBackend>>> = Vec::new();
		for _ in 0..4 { chunks.push(subs.drain(..chunk_size).collect()); }
		chunks.push(subs);
		
		let handles: Vec<_> = chunks.into_iter()
									.map(|batch| thread::spawn(move || drop(batch)))
									.collect();
		for h in handles { h.join().unwrap(); }
		
		alloc.reap(1);
		alloc.flush_device_frees();
		
		assert!(alloc.pending_device_frees_is_empty());
		assert!(factory.freed_count.load(Ordering::Relaxed) >= 1);
	}
	
	#[test]
	fn test_concurrent_alloc_n_threads() {
		const THREADS:     usize = 8;
		const ALLOCS_EACH: usize = 200;
		
		let (alloc, factory) = test_allocator(512 * 1024);
		
		let handles: Vec<_> = (0..THREADS).map(|_| {
			let alloc = Arc::clone(&alloc);
			thread::spawn(move || {
				let mut subs: Vec<SubAllocation<VulkanBackend>> = (0..ALLOCS_EACH)
					.map(|_| alloc.allocate_raw(1024, 16, 0, ResourceKind::Buffer, 1).unwrap())
					.collect();
				for s in subs.iter_mut() { s.finalize_lifetime(1); }
				drop(subs);
				alloc.reap(1);
			})
		}).collect();
		
		for h in handles { h.join().unwrap(); }
		
		alloc.flush_device_frees();
		assert!(alloc.pending_device_frees_is_empty());
		
		let allocated = factory.next_id.load(Ordering::Relaxed) - 1;
		let freed     = factory.freed_count.load(Ordering::Relaxed);
		assert_eq!(allocated, freed,
				   "Every allocated block must be returned: allocated={allocated} freed={freed}");
	}
	
	// ── Free channel correctness ──────────────────────────────────────────
	
	#[test]
	fn test_flush_is_idempotent() {
		let (alloc, factory) = test_allocator(1024 * 1024);
		
		let mut s = alloc.allocate_raw(256, 16, 0, ResourceKind::Buffer, 0).unwrap();
		s.finalize_lifetime(1);
		drop(s);
		alloc.reap(1);
		
		alloc.flush_device_frees();
		alloc.flush_device_frees(); // must not double-free
		
		assert_eq!(factory.freed_count.load(Ordering::Relaxed), 1);
	}
	
	#[test]
	fn test_partial_reap_deferred() {
		let (alloc, factory) = test_allocator(1024 * 1024);
		
		let mut s = alloc.allocate_raw(256, 16, 0, ResourceKind::Buffer, 0).unwrap();
		s.finalize_lifetime(10);
		drop(s);
		
		alloc.reap(5);
		alloc.flush_device_frees();
		assert_eq!(factory.freed_count.load(Ordering::Relaxed), 0,
				   "Block must not be freed before GPU timeline reaches lifetime");
		
		alloc.reap(10);
		alloc.flush_device_frees();
		assert_eq!(factory.freed_count.load(Ordering::Relaxed), 1,
				   "Block must be freed once GPU timeline has passed");
	}
	
	// ── Memory type lookup ────────────────────────────────────────────────
	
	#[test]
	fn test_find_memory_type() {
		let mut props = ash::vk::PhysicalDeviceMemoryProperties::default();
		props.memory_type_count = 3;
		props.memory_types[0].property_flags = MemoryPropertyFlags::HOST_VISIBLE.into();
		props.memory_types[1].property_flags =
			(MemoryPropertyFlags::DEVICE_LOCAL | MemoryPropertyFlags::HOST_VISIBLE).into();
		props.memory_types[2].property_flags = MemoryPropertyFlags::DEVICE_LOCAL.into();
		
		let alloc = GpuAllocator::with_factory(
			MockFactory::new() as Arc<dyn BlockFactory>,
			props,
			DEFAULT_BUFFER_BLOCK_SIZE,
			DEFAULT_IMAGE_BLOCK_SIZE,
		);
		
		assert_eq!(
			alloc.find_memory_type(0b110, MemoryPropertyFlags::DEVICE_LOCAL),
			Some(1)
		);
		
		assert_eq!(
			alloc.find_memory_type(0b001, MemoryPropertyFlags::DEVICE_LOCAL),
			None
		);
	}
	
	// ── Buffer vs Image isolation ─────────────────────────────────────────
	
	#[test]
	fn test_buffer_image_block_isolation() {
		let (alloc, factory) = test_allocator(512 * 1024);
		
		let mut buf = alloc.allocate_raw(1024, 16, 0, ResourceKind::Buffer, 0).unwrap();
		let mut img = alloc.allocate_raw(1024, 16, 0, ResourceKind::Image,  0).unwrap();
		
		assert_eq!(factory.next_id.load(Ordering::Relaxed), 3,
				   "Buffer and Image must use separate blocks");
		
		buf.finalize_lifetime(1);
		img.finalize_lifetime(1);
		drop(buf);
		drop(img);
		alloc.reap(1);
		alloc.flush_device_frees();
		assert_eq!(factory.freed_count.load(Ordering::Relaxed), 2);
	}
}