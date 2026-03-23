// src/infra/vulkan/memory/suballoc.rs
use std::sync::Arc;
use crate::core::Backend;
use crate::infra::vulkan::memory::free_list::FreeList;
use crate::infra::vulkan::memory::return_q::ReturnQueue;
use crate::domain::ResourceKind;
use tracing::{debug, trace, warn};

// ── Lifetime ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime { Unset, Submitted(u64) }

// ── FreeRequest ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct FreeRequest {
	pub block_idx:  u32,
	pub node_idx:   u32,
	pub generation: u32,
	pub lifetime:   Lifetime,
}

// ── SubAllocation ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SubAllocation<B: Backend> {
	buffer: B::Buffer,
	memory:       B::DeviceMemory,
	offset:       u64,
	size:         u64,
	block_idx:    u32,
	node_idx:     u32,
	generation:   u32,
	arena_return: Arc<ReturnQueue<FreeRequest>>,
	lifetime:     Lifetime,
}

// SAFETY: B::DeviceMemory is an opaque handle with no interior mutability
// (vk::DeviceMemory is a u64). SubAllocation only writes to the lock-free
// ReturnQueue on drop — no shared mutable state is accessed.
unsafe impl<B: Backend> Send for SubAllocation<B> where B::DeviceMemory: Send {}

impl<B: Backend> SubAllocation<B> {
	pub(crate) fn new(
		buffer:       B::Buffer,
		memory:       B::DeviceMemory,
		offset:       u64,
		size:         u64,
		block_idx:    u32,
		node_idx:     u32,
		generation:   u32,
		arena_return: Arc<ReturnQueue<FreeRequest>>,
	) -> Self {
		trace!(
              offset,
              size,
              block_idx,
              node_idx,
              generation,
              "SubAllocation::new"
          );
		Self {
			buffer, memory, offset, size,
			block_idx, node_idx, generation,
			arena_return,
			lifetime: Lifetime::Unset,
		}
	}
	
	pub fn finalize_lifetime(&mut self, t: u64) {
		debug_assert!(matches!(self.lifetime, Lifetime::Unset));
		debug_assert!(t > 0, "GPU timeline t=0 is reserved for 'never submitted'");
		trace!(
              block_idx = self.block_idx,
              node_idx = self.node_idx,
              generation = self.generation,
              timeline_val = t,
              offset = self.offset,
              size = self.size,
              "SubAllocation::finalize_lifetime"
          );
		self.lifetime = Lifetime::Submitted(t);
	}
	
	pub fn memory(&self) -> B::DeviceMemory where B::DeviceMemory: Copy { self.memory }
	pub fn offset(&self) -> u64 { self.offset }
	pub fn size(&self)   -> u64 { self.size   }
	pub fn block_idx(&self) -> u32 {self.block_idx}
	pub fn buffer(&self) -> B::Buffer {self.buffer}
}

impl<B: Backend> Drop for SubAllocation<B> {
	fn drop(&mut self) {
		if matches!(self.lifetime, Lifetime::Unset) {
			warn!(
                  block_idx = self.block_idx,
                  node_idx = self.node_idx,
                  offset = self.offset,
                  size = self.size,
                  "SubAllocation dropped WITHOUT finalize_lifetime — aborting"
              );
			#[cfg(test)]
			panic!("SubAllocation dropped without calling finalize_lifetime");
			#[cfg(not(test))]
			std::process::abort();
		}
		trace!(
              block_idx = self.block_idx,
              node_idx = self.node_idx,
              generation = self.generation,
              lifetime = ?self.lifetime,
              offset = self.offset,
              size = self.size,
              thread = ?std::thread::current().id(),
              "SubAllocation::drop — pushing FreeRequest"
          );
		self.arena_return.push(FreeRequest {
			block_idx:  self.block_idx,
			node_idx:   self.node_idx,
			generation: self.generation,
			lifetime:   self.lifetime,
		});
	}
}

// ── ArenaBlock ────────────────────────────────────────────────────────────────

pub struct ArenaBlock<B: Backend> {
	pub memory: B::DeviceMemory,
	pub buffer: B::Buffer,
	pub kind: ResourceKind,
	pub free_list: FreeList,
	pub live_allocations: u32,
}

// ── BlockSlot ─────────────────────────────────────────────────────────────────

struct BlockSlot<B: Backend> {
	block:      Option<ArenaBlock<B>>,
	generation: u32,
}

// ── ThreadArena ───────────────────────────────────────────────────────────────

pub struct ThreadArena<B: Backend> {
	blocks:           Vec<BlockSlot<B>>,
	free_block_slots: Vec<u32>,
	return_queue:     Arc<ReturnQueue<FreeRequest>>,
	deferred_frees:   Vec<FreeRequest>,
	pub pending_device_frees: Vec<(B::Buffer, B::DeviceMemory)>
}

impl<B: Backend> ThreadArena<B>
	where
		B::DeviceMemory: Copy,
{
	pub fn new() -> Self {
		trace!("ThreadArena::new");
		Self {
			blocks:               vec![],
			free_block_slots:     vec![],
			return_queue:         Arc::new(ReturnQueue::new()),
			deferred_frees:       vec![],
			pending_device_frees: vec![],
		}
	}
	
	pub fn reap(&mut self, current_gpu_t: u64) {
		let incoming: Vec<_> = self.return_queue.drain().collect();
		let incoming_count = incoming.len();
		let mut work_list = std::mem::take(&mut self.deferred_frees);
		let prev_deferred = work_list.len();
		work_list.extend(incoming);
		
		trace!(
              current_gpu_t,
              incoming = incoming_count,
              prev_deferred,
              total_work = work_list.len(),
              "ThreadArena::reap — begin"
          );
		
		let mut applied = 0u32;
		let mut deferred = 0u32;
		let mut stale = 0u32;
		
		// ── PASS 1: apply ready deallocations ──────────────────────────────
		for req in work_list {
			let ready = match req.lifetime {
				Lifetime::Unset        => false,
				Lifetime::Submitted(t) => t <= current_gpu_t,
			};
			
			if !ready {
				self.deferred_frees.push(req);
				deferred += 1;
				continue;
			}
			
			if let Some(slot) = self.blocks.get_mut(req.block_idx as usize) {
				if slot.generation == req.generation {
					if let Some(block) = slot.block.as_mut() {
						block.free_list.free(req.node_idx);
						block.live_allocations =
							block.live_allocations.saturating_sub(1);
						applied += 1;
						
						trace!(
                              block_idx = req.block_idx,
                              node_idx = req.node_idx,
                              generation = req.generation,
                              remaining_live = block.live_allocations,
                              free_bytes = block.free_list.free_bytes(),
                              "FreeRequest applied"
                          );
					}
				} else {
					stale += 1;
					trace!(
                          block_idx = req.block_idx,
                          req_gen = req.generation,
                          slot_gen = slot.generation,
                          "Stale FreeRequest discarded (generation mismatch)"
                      );
				}
			}
		}
		
		if applied > 0 || stale > 0 || deferred > 0 {
			debug!(
                  applied,
                  deferred,
                  stale,
                  current_gpu_t,
                  "ThreadArena::reap pass 1 summary"
              );
		}
		
		let deferred_pending: std::collections::HashSet<(u32, u32)> =
			self.deferred_frees
				.iter()
				.map(|r| (r.block_idx, r.generation))
				.collect();
		
		// ── PASS 2: destroy fully-empty blocks ─────────────────────────────
		let mut destroyed = 0u32;
		for idx in 0..self.blocks.len() {
			let slot = &mut self.blocks[idx];
			let eligible = slot.block.as_ref().map_or(false, |b| {
				b.live_allocations == 0
					&& !deferred_pending.contains(&(idx as u32, slot.generation))
			});
			
			if eligible {
				let dead = slot.block.take().unwrap();
				debug!(
                      block_idx = idx,
                      generation = slot.generation,
                      block_size = dead.free_list.block_size(),
                      ?dead.kind,
                      "Block fully empty — queued for vkFreeMemory"
                  );
				self.pending_device_frees.push((dead.buffer, dead.memory));
				self.free_block_slots.push(idx as u32);
				destroyed += 1;
			}
		}
		
		if destroyed > 0 {
			debug!(
                  destroyed,
                  pending_frees = self.pending_device_frees.len(),
                  free_slots = self.free_block_slots.len(),
                  "ThreadArena::reap pass 2 — blocks destroyed"
              );
		}
		
		trace!(
              current_gpu_t,
              applied,
              deferred,
              stale,
              destroyed,
              active_blocks = self.blocks.iter().filter(|s| s.block.is_some()).count(),
              "ThreadArena::reap — done"
          );
	}
	
	#[must_use]
	pub fn allocate(
		&mut self,
		size:  u64,
		align: u64,
		kind:  ResourceKind,
		_t:    u64,
	) -> Option<SubAllocation<B>> {
		for (i, slot) in self.blocks.iter_mut().enumerate() {
			let generation = slot.generation;
			if let Some(block) = slot.block.as_mut() {
				if block.kind != kind { continue; }
				if let Some((off, node_idx)) = block.free_list.allocate(size, align) {
					block.live_allocations = block
						.live_allocations
						.checked_add(1)
						.expect("live_allocations overflow");
					
					trace!(
                          block_idx = i,
                          generation,
                          offset = off,
                          size,
                          align,
                          node_idx,
                          live = block.live_allocations,
                          free_bytes = block.free_list.free_bytes(),
                          ?kind,
                          "ThreadArena::allocate — success"
                      );
					
					return Some(SubAllocation::new(
						block.buffer, block.memory, off, size,
						i as u32, node_idx, generation,
						Arc::clone(&self.return_queue),
					));
				}
			}
		}
		
		trace!(
              size,
              align,
              ?kind,
              block_count = self.blocks.len(),
              "ThreadArena::allocate — no block has room"
          );
		None
	}
	
	pub fn inject_new_block(
		&mut self,
		buffer: B::Buffer,
		mem:    B::DeviceMemory,
		size:   u64,
		kind:   ResourceKind,
	) {
		let new_block = ArenaBlock {
			buffer,
			memory: mem,
			kind,
			free_list: FreeList::new(size),
			live_allocations: 0,
		};
		
		if let Some(r_idx) = self.free_block_slots.pop() {
			let slot = &mut self.blocks[r_idx as usize];
			let old_gen = slot.generation;
			slot.generation = slot.generation.wrapping_add(1);
			slot.block = Some(new_block);
			debug!(
                  slot_idx = r_idx,
                  old_gen,
                  new_gen = slot.generation,
                  size,
                  ?kind,
                  "inject_new_block — reused slot"
              );
		} else {
			let idx = self.blocks.len();
			self.blocks.push(BlockSlot {
				block:      Some(new_block),
				generation: 0,
			});
			debug!(
                  slot_idx = idx,
                  generation = 0,
                  size,
                  ?kind,
                  "inject_new_block — new slot"
              );
		}
	}
	
	pub fn get_return_queue(&self) -> Arc<ReturnQueue<FreeRequest>> {
		Arc::clone(&self.return_queue)
	}
	
	pub fn return_queue_is_empty(&self) -> bool {
		self.return_queue.is_empty()
	}
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use rand::RngExt;
	use ash::vk::Handle;
	use super::*;
	use std::thread;
	use crate::infra::vulkan::backend::VulkanBackend;
	
	type Arena  = ThreadArena<VulkanBackend>;
	type SubAlloc = SubAllocation<VulkanBackend>;
	
	fn dummy_mem(id: u64) -> <VulkanBackend as crate::core::Backend>::DeviceMemory {
		ash::vk::DeviceMemory::from_raw(id)
	}
	fn dummy_buf(id: u64) -> <VulkanBackend as crate::core::Backend>::Buffer {
		ash::vk::Buffer::from_raw(id)
	}
	// ── Core correctness ─────────────────────────────────────────────────
	
	#[test]
	fn test_temporal_safety_gating() {
		let mut arena = Arena::new();
		arena.inject_new_block(
			dummy_buf(1),
			dummy_mem(1),
			1024 * 1024,
			ResourceKind::Buffer
		);
		
		let mut alloc = arena.allocate(1024, 16, ResourceKind::Buffer, 0).unwrap();
		alloc.finalize_lifetime(10);
		drop(alloc);
		
		arena.reap(5);
		assert_eq!(arena.deferred_frees.len(), 1, "Should stay in deferred list");
		assert!(
			!arena.blocks[0].block.as_ref().unwrap().free_list.is_empty(),
			"FreeList must not merge before GPU fence"
		);
		
		arena.reap(10);
		assert_eq!(arena.deferred_frees.len(), 0);
		assert!(arena.blocks[0].block.is_none(), "Block must be destroyed when empty");
	}
	
	#[test]
	fn test_generation_aba_protection() {
		let mut arena = Arena::new();
		let block_size = 1024;
		
		arena.inject_new_block(
			dummy_buf(200),
			dummy_mem(200),
			block_size,
			ResourceKind::Buffer
		);
		let mut alloc_gen0 = arena.allocate(block_size, 1, ResourceKind::Buffer, 0).unwrap();
		alloc_gen0.finalize_lifetime(1);
		drop(alloc_gen0);
		arena.reap(1);
		assert!(arena.blocks[0].block.is_none());
		
		arena.inject_new_block(
			dummy_buf(200),
			dummy_mem(200),
			block_size,
			ResourceKind::Buffer
		);
		assert_eq!(arena.blocks[0].generation, 1);
		
		let mut hold = arena.allocate(1, 1, ResourceKind::Buffer, 0).unwrap();
		
		arena.return_queue.push(FreeRequest {
			block_idx:  0,
			node_idx:   0,
			generation: 0, // stale
			lifetime:   Lifetime::Submitted(1),
		});
		
		arena.reap(100);
		
		assert_eq!(arena.blocks[0].generation, 1);
		let block = arena.blocks[0].block.as_ref().unwrap();
		assert!(!block.free_list.is_empty());
		assert_eq!(block.live_allocations, 1);
		
		hold.finalize_lifetime(200);
		drop(hold);
		arena.reap(200);
	}
	
	#[test]
	#[should_panic(expected = "SubAllocation dropped without calling finalize_lifetime")]
	fn test_contract_enforcement_panic() {
		let mut arena = Arena::new();
		arena.inject_new_block(
			dummy_buf(1),
			dummy_mem(1),
			1024,
			ResourceKind::Buffer
		);
		let alloc = arena.allocate(16, 16, ResourceKind::Buffer, 0).unwrap();
		drop(alloc);
	}
	
	#[test]
	fn test_random_lifetimes() {
		let mut arena = Arena::new();
		arena.inject_new_block(
			dummy_buf(1),
			dummy_mem(1),
			1024 * 1024,
			ResourceKind::Buffer
		);
		
		let mut allocs = Vec::new();
		for i in 0u64..1000 {
			let mut a = arena.allocate(256, 16, ResourceKind::Buffer, 0).unwrap();
			a.finalize_lifetime(i % 5 + 1);
			allocs.push(a);
		}
		
		drop(allocs);
		for t in 1..=5 { arena.reap(t); }
		
		assert!(arena.return_queue.is_empty());
		assert!(arena.deferred_frees.is_empty());
		assert!(arena.blocks.iter().all(|s| s.block.is_none()));
	}
	
	#[test]
	fn test_concurrent_stress_return() {
		let mut arena = Arena::new();
		for i in 0..4 {
			let id = i as u64 + 1;
			arena.inject_new_block(
				dummy_buf(id),
				dummy_mem(id),
				2 * 1024 * 1024,
				ResourceKind::Buffer
			);
		}
		
		let mut allocs: Vec<SubAlloc> = (0..1000)
			.filter_map(|_| {
				let mut a = arena.allocate(64, 16, ResourceKind::Buffer, 0)?;
				a.finalize_lifetime(1);
				Some(a)
			})
			.collect();
		
		let mut iter = allocs.into_iter();
		let handles: Vec<_> = (0..10)
			.map(|_| {
				let batch: Vec<_> = (0..100).filter_map(|_| iter.next()).collect();
				thread::spawn(move || drop(batch))
			})
			.collect();
		
		for t in 1..=10 { arena.reap(t); }
		for h in handles { h.join().unwrap(); }
		arena.reap(u64::MAX);
		
		assert!(arena.deferred_frees.is_empty());
		assert!(arena.blocks.iter().all(|s| s.block.is_none()));
	}
	
	#[test]
	fn test_concurrent_stress_return_pure() {
		let mut arena = Arena::new();
		let block_count     = 8;
		let allocs_per_thread = 200;
		let thread_count    = 8;
		
		for i in 0..block_count {
			for i in 0..block_count {
				let id = i as u64 + 1;
				arena.inject_new_block(
					dummy_buf(id),
					dummy_mem(id),
					2 * 1024 * 1024,
					ResourceKind::Buffer
				);
			}
		}
		
		let mut buckets: Vec<Vec<SubAlloc>> = (0..thread_count)
			.map(|_| Vec::with_capacity(allocs_per_thread))
			.collect();
		
		for t in 0..thread_count {
			for _ in 0..allocs_per_thread {
				let mut a = arena.allocate(1024, 16, ResourceKind::Buffer, 0)
								 .expect("Arena exhausted during setup");
				a.finalize_lifetime(1);
				buckets[t].push(a);
			}
		}
		
		let handles: Vec<_> = buckets.into_iter()
									 .map(|b| thread::spawn(move || drop(b)))
									 .collect();
		
		let mut pumps = 0;
		while pumps < 500 {
			arena.reap(1);
			pumps += 1;
			thread::yield_now();
		}
		
		for h in handles { h.join().unwrap(); }
		
		let mut timeout = 0;
		loop {
			arena.reap(u64::MAX);
			if arena.blocks.iter().all(|s| s.block.is_none()) { break; }
			timeout += 1;
			if timeout > 10_000 {
				let (idx, slot) = arena.blocks.iter().enumerate()
									   .find(|(_, s)| s.block.is_some()).unwrap();
				let b = slot.block.as_ref().unwrap();
				panic!(
					"STALL: block {} free={}/{} live={}",
					idx, b.free_list.free_bytes(), b.free_list.block_size(), b.live_allocations
				);
			}
			thread::yield_now();
		}
		
		assert!(arena.deferred_frees.is_empty());
		assert!(arena.return_queue.is_empty());
	}
	
	#[test]
	fn test_alignment_pathology() {
		let mut arena = Arena::new();
		arena.inject_new_block(
			dummy_buf(1),
			dummy_mem(1),
			1024 * 1024,
			ResourceKind::Buffer
		);
		
		let aligns = [8u64, 16, 32, 64, 128, 256];
		let mut allocs = Vec::with_capacity(1000);
		for i in 0..1000 {
			let align = aligns[i % aligns.len()];
			let a = arena.allocate(128, align, ResourceKind::Buffer, 0)
						 .expect("alignment failure");
			allocs.push(a);
		}
		
		for a in allocs.iter_mut() { a.finalize_lifetime(1); }
		drop(allocs);
		arena.reap(1);
	}
	
	#[test]
	fn test_fragmentation_pressure() {
		let mut arena = Arena::new();
		arena.inject_new_block(
			dummy_buf(1),
			dummy_mem(1),
			1024 * 1024,
			ResourceKind::Buffer
		);
		
		let mut allocs = Vec::new();
		let sizes = [64u64, 128, 256, 512, 1024, 2048];
		for i in 0..2000 {
			let size = sizes[i % sizes.len()];
			if let Some(a) = arena.allocate(size, 16, ResourceKind::Buffer, 0) {
				allocs.push(a);
			}
		}
		
		for i in (0..allocs.len()).step_by(2).rev() {
			let mut a = allocs.swap_remove(i);
			a.finalize_lifetime(1);
		}
		for a in allocs.iter_mut() { a.finalize_lifetime(1); }
		
		while !arena.return_queue.is_empty() || !arena.deferred_frees.is_empty() {
			arena.reap(1);
		}
		drop(allocs);
		while !arena.return_queue.is_empty() || !arena.deferred_frees.is_empty() {
			arena.reap(1);
		}
		
		if arena.blocks.iter().all(|s| s.block.is_none()) {
			arena.inject_new_block(
				dummy_buf(99),
				dummy_mem(99),
				1024 * 1024,
				ResourceKind::Buffer
			);
		}
		
		let mut extra: Vec<SubAlloc> = (0..100)
			.map(|_| arena.allocate(512, 16, ResourceKind::Buffer, 1)
						  .expect("Fragmentation failure: TLSF islands did not coalesce"))
			.collect();
		
		for a in extra.iter_mut() { a.finalize_lifetime(1); }
		drop(extra);
		arena.reap(1);
	}
	
	#[test]
	fn test_invariants_under_stress() {
		let mut arena = Arena::new();
		arena.inject_new_block(
			dummy_buf(1),
			dummy_mem(1),
			1024 * 1024,
			ResourceKind::Buffer
		);
		
		for _ in 0..500 {
			let mut a = arena.allocate(128, 16, ResourceKind::Buffer, 0).unwrap();
			a.finalize_lifetime(1);
		}
		
		let mut keepers: Vec<SubAlloc> = (0..500)
			.map(|_| arena.allocate(128, 16, ResourceKind::Buffer, 0).unwrap())
			.collect();
		
		arena.reap(1);
		
		for slot in arena.blocks.iter() {
			if let Some(b) = slot.block.as_ref() {
				b.free_list.check_invariants();
			}
		}
		
		for a in keepers.iter_mut() { a.finalize_lifetime(2); }
		drop(keepers);
		arena.reap(2);
	}
	
	#[test]
	fn test_split_coalesce_worst_case() {
		let mut arena = Arena::new();
		arena.inject_new_block(
			dummy_buf(1),
			dummy_mem(1),
			1024 * 1024,
			ResourceKind::Buffer
		);
		
		let mut allocs: Vec<SubAlloc> = (0..8000)
			.map(|_| arena.allocate(64, 16, ResourceKind::Buffer, 0).unwrap())
			.collect();
		
		for i in (0..allocs.len()).step_by(2).rev() {
			let mut a = allocs.swap_remove(i);
			a.finalize_lifetime(1);
			drop(a);
		}
		
		arena.reap(1);
		
		let mut big = arena.allocate(64 * 100, 16, ResourceKind::Buffer, 1).expect("coalesce failed");
		big.finalize_lifetime(1);
		drop(big);
		
		for a in allocs.iter_mut() { a.finalize_lifetime(1); }
		drop(allocs);
		arena.reap(1);
	}
	
	#[test]
	fn fuzz_allocator() {
		use rand::{Rng, SeedableRng};
		let mut rng = rand::rngs::StdRng::seed_from_u64(42);
		
		let mut arena = Arena::new();
		arena.inject_new_block(
			dummy_buf(1),
			dummy_mem(1),
			1024 * 1024,
			ResourceKind::Buffer
		);
		
		let mut live: Vec<SubAlloc> = Vec::new();
		
		for t in 1u64..=5000 {
			if rng.random_bool(0.6) {
				let size = 1 << rng.random_range(4..10u32);
				if let Some(a) = arena.allocate(size, 16, ResourceKind::Buffer, t) {
					live.push(a);
				}
			} else if !live.is_empty() {
				let idx = rng.random_range(0..live.len());
				let mut a = live.swap_remove(idx);
				a.finalize_lifetime(t);
			}
			
			arena.reap(t);
			
			for slot in arena.blocks.iter() {
				if let Some(b) = slot.block.as_ref() {
					b.free_list.check_invariants();
				}
			}
		}
		
		for a in live.iter_mut() { a.finalize_lifetime(5001); }
		drop(live);
		arena.reap(5001);
	}
}

impl<B: Backend> Default for SubAllocation<B>
	where B::DeviceMemory: Default
{
	fn default() -> Self {
		Self {
			buffer:          B::Buffer::default(),
			memory:       B::DeviceMemory::default(),
			offset:       0,
			size:         0,
			block_idx:    u32::MAX,
			node_idx:     0,
			generation:   0,
			arena_return: Arc::new(ReturnQueue::new()),
			lifetime:     Lifetime::Submitted(0),
		}
	}
}