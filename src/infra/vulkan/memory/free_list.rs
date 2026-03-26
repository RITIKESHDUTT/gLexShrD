
//! TLSF (Two-Level Segregated Fit) free-list for GPU memory sub-allocation.
//!
//! O(1) allocate, O(1) free, O(1) coalescing via bitmap-driven bin lookup.
//! 64×64 bin matrix: 64 first-level size classes × 64 second-level subdivisions.

use tracing::{debug, trace, warn};

// ── Constants (authoritative) ────────────────────────────────────────────────
pub const NULL: u32 = u32::MAX;
pub const SL_BITS: u32 = 6;
pub const SL_COUNT: usize = 1 << SL_BITS; // 64
pub const FL_COUNT: usize = 64;
pub const MIN_FL: u32 = 8;
pub const FL_OFFSET: u32 = MIN_FL - 1; // 7
pub const MIN_ALLOC: u64 = 16;
pub const SMALL_STEP_SHIFT: u32 = MIN_FL - SL_BITS; // 2

// ── Node ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct MemNode {
	pub offset: u64,
	pub size: u64,
	pub prev_phys: u32,
	pub next_phys: u32,
	pub prev_free: u32,
	pub next_free: u32,
	pub is_free: bool,
	pub generation: u32,
}

// ── FreeList ─────────────────────────────────────────────────────────────────

pub struct FreeList {
	first_level_mask: u64,
	second_level_masks: [u64; FL_COUNT],
	bins: [[u32; SL_COUNT]; FL_COUNT],
	nodes: Vec<MemNode>,
	recycled: Vec<u32>,
	block_size: u64,
	free_bytes: u64,
}

// ── Mapping (authoritative) ──────────────────────────────────────────────────

/// Size → (fl, sl) for the exact bin where a free block of `size` belongs.
#[inline(always)]
pub fn mapping_insert(size: u64) -> (usize, usize) {
	debug_assert!(size >= MIN_ALLOC, "Size below minimum allocation");
	debug_assert!(MIN_FL >= SL_BITS, "MIN_FL must be >= SL_BITS");
	
	if size < (1 << MIN_FL) {
		let sl = (size - MIN_ALLOC) >> SMALL_STEP_SHIFT;
		debug_assert!(sl < SL_COUNT as u64, "Small block sl out of bounds");
		(0, sl as usize)
	} else {
		let raw_fl = 63 - size.leading_zeros();
		let shift = raw_fl - SL_BITS;
		let sl = (size ^ (1u64 << raw_fl)) >> shift;
		debug_assert!(sl < SL_COUNT as u64, "sl overflowed");
		((raw_fl - FL_OFFSET) as usize, sl as usize)
	}
}

/// Size → (fl, sl) for a bin guaranteed to contain a block >= `size`.
#[inline(always)]
pub fn mapping_search(mut size: u64) -> (usize, usize) {
	size = size.max(MIN_ALLOC);
	
	let step = if size < (1 << MIN_FL) {
		1u64 << SMALL_STEP_SHIFT
	} else {
		let raw_fl = 63 - size.leading_zeros();
		1u64 << (raw_fl - SL_BITS)
	};
	
	mapping_insert(size.saturating_add(step - 1))
}

// ── Utility ──────────────────────────────────────────────────────────────────

#[inline(always)]
const fn align_up(value: u64, alignment: u64) -> u64 {
	(value + alignment - 1) & !(alignment - 1)
}

// ── Implementation ───────────────────────────────────────────────────────────

impl FreeList {
	/// Create a free-list managing a single block of `block_size` bytes.
	pub fn new(block_size: u64) -> Self {
		assert!(block_size >= MIN_ALLOC);
		
		debug!(block_size, "FreeList::new — creating TLSF free-list");
		
		let mut list = Self {
			first_level_mask: 0,
			second_level_masks: [0; FL_COUNT],
			bins: [[NULL; SL_COUNT]; FL_COUNT],
			nodes: Vec::new(),
			recycled: Vec::new(),
			block_size,
			free_bytes: 0,
		};
		
		let idx = list.alloc_node(MemNode {
			offset: 0,
			size: block_size,
			prev_phys: NULL,
			next_phys: NULL,
			prev_free: NULL,
			next_free: NULL,
			is_free: true,
			generation: NULL,
		});
		
		list.free_bytes = block_size;
		list.insert_into_bin(idx);
		
		trace!(node_idx = idx, "Initial free node inserted");
		list
	}
	
	pub fn block_size(&self) -> u64 {
		self.block_size
	}
	
	pub fn free_bytes(&self) -> u64 {
		self.free_bytes
	}
	
	#[inline(always)]
	pub fn node_count(&self) -> usize {
		self.nodes.len()
	}
	
	/// True when every byte in the block is free (single coalesced region).
	pub fn is_empty(&self) -> bool {
		self.free_bytes == self.block_size
	}
	
	/// Allocate `size` bytes aligned to `alignment`.
	/// Returns `(offset, node_index)` or `None` if no fit.
	pub fn allocate(&mut self, size: u64, alignment: u64) -> Option<(u64, u32)> {
		debug_assert!(alignment.is_power_of_two(), "Alignment must be power of two");
		
		let size = align_up(size.max(MIN_ALLOC), MIN_ALLOC);
		
		let max_gap = alignment.saturating_sub(MIN_ALLOC);
		let search_size = size.checked_add(max_gap)?;
		
		let (fl, sl) = mapping_search(search_size);
		trace!(
              requested = size,
              alignment,
              search_size,
              fl, sl,
              free_before = self.free_bytes,
              "FreeList::allocate — searching"
          );
		
		let candidate = self.find_suitable_block(fl, sl)?;
		self.remove_from_bin(candidate);
		
		let offset = self.nodes[candidate as usize].offset;
		let aligned_offset = align_up(offset, alignment);
		let front_padding = aligned_offset - offset;
		
		// Split front padding into a separate free node.
		let alloc_idx = if front_padding > 0 {
			debug_assert!(front_padding >= MIN_ALLOC);
			trace!(
                  candidate,
                  front_padding,
                  "Splitting front padding for alignment"
              );
			let back = self.split_node(candidate, front_padding);
			self.insert_into_bin(candidate); // front stays free
			back
		} else {
			candidate
		};
		
		// Split remainder if large enough for a standalone free node.
		let node_size = self.nodes[alloc_idx as usize].size;
		let remainder = node_size - size;
		if remainder >= MIN_ALLOC {
			trace!(alloc_idx, remainder, "Splitting remainder");
			let rem = self.split_node(alloc_idx, size);
			self.insert_into_bin(rem);
		}
		
		// Mark allocated.
		self.nodes[alloc_idx as usize].is_free = false;
		self.free_bytes -= self.nodes[alloc_idx as usize].size;
		
		let final_offset = self.nodes[alloc_idx as usize].offset;
		let final_size = self.nodes[alloc_idx as usize].size;
		debug_assert_eq!(final_offset % alignment, 0);
		
		trace!(
              node = alloc_idx,
              offset = final_offset,
              size = final_size,
              free_after = self.free_bytes,
              "FreeList::allocate — success"
          );
		
		Some((final_offset, alloc_idx))
	}
	
	/// Free a previously allocated node. Coalesces with adjacent free regions.
	pub fn free(&mut self, mut node_idx: u32) {
		debug_assert!(!self.nodes[node_idx as usize].is_free, "Double free");
		
		let offset = self.nodes[node_idx as usize].offset;
		let original_size = self.nodes[node_idx as usize].size;
		
		trace!(
              node = node_idx,
              offset,
              size = original_size,
              free_before = self.free_bytes,
              "FreeList::free — releasing"
          );
		
		// Mark free first
		self.nodes[node_idx as usize].is_free = true;
		self.free_bytes += original_size;
		
		// ---- forward merge ----
		loop {
			let next = self.nodes[node_idx as usize].next_phys;
			if next != NULL && self.nodes[next as usize].is_free {
				trace!(
                      node = node_idx,
                      merging = next,
                      merged_size = self.nodes[next as usize].size,
                      "Forward coalesce"
                  );
				self.remove_from_bin(next);
				self.merge_next(node_idx, next);
			} else {
				break;
			}
		}
		
		// ---- backward merge ----
		loop {
			let prev = self.nodes[node_idx as usize].prev_phys;
			if prev != NULL && self.nodes[prev as usize].is_free {
				trace!(
                      node = node_idx,
                      merging_into = prev,
                      merged_size = self.nodes[node_idx as usize].size,
                      "Backward coalesce"
                  );
				self.remove_from_bin(prev);
				self.merge_next(prev, node_idx);
				node_idx = prev;
			} else {
				break;
			}
		}
		
		// IMPORTANT: clear any stale links (defensive)
		self.nodes[node_idx as usize].prev_free = NULL;
		self.nodes[node_idx as usize].next_free = NULL;
		
		self.insert_into_bin(node_idx);
		
		trace!(
              node = node_idx,
              final_size = self.nodes[node_idx as usize].size,
              free_after = self.free_bytes,
              is_empty = self.is_empty(),
              "FreeList::free — done"
          );
		
		// DEBUG invariant
		debug_assert!(
			self.free_bytes != self.block_size ||
				(self.nodes[node_idx as usize].prev_phys == NULL &&
					self.nodes[node_idx as usize].next_phys == NULL),
			"Full free but not single node"
		);
	}
	
	// ── Bin operations ───────────────────────────────────────────────────
	
	/// Find a free block at (fl, sl) or the next non-empty bin.
	fn find_suitable_block(&self, fl: usize, sl: usize) -> Option<u32> {
		// Same fl, sl or higher.
		let sl_map = self.second_level_masks[fl] & (!0u64 << sl);
		if sl_map != 0 {
			let found_sl = sl_map.trailing_zeros() as usize;
			let idx = self.bins[fl][found_sl];
			debug_assert_ne!(idx, NULL);
			trace!(fl, found_sl, node = idx, "Found block in same FL");
			return Some(idx);
		}
		
		// Higher fl.
		let fl_map = self.first_level_mask & (!0u64 << (fl + 1));
		if fl_map == 0 {
			trace!(fl, sl, "No suitable block found — OOM");
			return None;
		}
		let found_fl = fl_map.trailing_zeros() as usize;
		let sl_map = self.second_level_masks[found_fl];
		debug_assert_ne!(sl_map, 0);
		let found_sl = sl_map.trailing_zeros() as usize;
		let idx = self.bins[found_fl][found_sl];
		debug_assert_ne!(idx, NULL);
		trace!(
              searched_fl = fl, searched_sl = sl,
              found_fl, found_sl, node = idx,
              "Found block in higher FL"
          );
		Some(idx)
	}
	
	/// Insert node at front of its bin's doubly-linked free list.
	fn insert_into_bin(&mut self, idx: u32) {
		let size = self.nodes[idx as usize].size;
		let (fl, sl) = mapping_insert(size);
		
		let head = self.bins[fl][sl];
		self.nodes[idx as usize].prev_free = NULL;
		self.nodes[idx as usize].next_free = head;
		
		if head != NULL {
			self.nodes[head as usize].prev_free = idx;
		}
		
		self.bins[fl][sl] = idx;
		self.first_level_mask |= 1u64 << fl;
		self.second_level_masks[fl] |= 1u64 << sl;
		
		trace!(node = idx, size, fl, sl, "Inserted into bin");
	}
	
	/// Remove node from its bin's doubly-linked free list. O(1).
	fn remove_from_bin(&mut self, idx: u32) {
		let prev = self.nodes[idx as usize].prev_free;
		let next = self.nodes[idx as usize].next_free;
		let size = self.nodes[idx as usize].size;
		
		if prev != NULL {
			self.nodes[prev as usize].next_free = next;
		} else {
			// Head of list — update bin head and bitmaps.
			let (fl, sl) = mapping_insert(size);
			self.bins[fl][sl] = next;
			if next == NULL {
				self.second_level_masks[fl] &= !(1u64 << sl);
				if self.second_level_masks[fl] == 0 {
					self.first_level_mask &= !(1u64 << fl);
				}
			}
		}
		
		if next != NULL {
			self.nodes[next as usize].prev_free = prev;
		}
		
		self.nodes[idx as usize].prev_free = NULL;
		self.nodes[idx as usize].next_free = NULL;
		
		trace!(node = idx, size, "Removed from bin");
	}
	
	// ── Physical chain operations ────────────────────────────────────────
	
	/// Split node at `cut_size` from its start.
	/// Original becomes the front (size = cut_size). Returns the new back node.
	fn split_node(&mut self, idx: u32, cut_size: u64) -> u32 {
		let orig = &self.nodes[idx as usize];
		debug_assert!(cut_size >= MIN_ALLOC);
		debug_assert!(orig.size - cut_size >= MIN_ALLOC);
		
		let back_offset = orig.offset + cut_size;
		let back_size = orig.size - cut_size;
		let next_phys = orig.next_phys;
		let is_free = orig.is_free;
		
		let back = self.alloc_node(MemNode {
			offset: back_offset,
			size: back_size,
			prev_phys: idx,
			next_phys,
			prev_free: NULL,
			next_free: NULL,
			is_free,
			generation: NULL,
		});
		
		self.nodes[idx as usize].size = cut_size;
		self.nodes[idx as usize].next_phys = back;
		
		if next_phys != NULL {
			self.nodes[next_phys as usize].prev_phys = back;
		}
		
		trace!(
              front = idx, front_size = cut_size,
              back_node = back, back_offset, back_size,
              "Split node"
          );
		
		back
	}
	
	/// Merge `next` into `idx` (absorb size, relink chain, recycle `next`).
	fn merge_next(&mut self, idx: u32, next: u32) -> u32 {
		let next_next = self.nodes[next as usize].next_phys;
		let next_size = self.nodes[next as usize].size;
		
		self.nodes[idx as usize].size += next_size;
		self.nodes[idx as usize].next_phys = next_next;
		
		if next_next != NULL {
			self.nodes[next_next as usize].prev_phys = idx;
		}
		
		trace!(
              survivor = idx,
              absorbed = next,
              absorbed_size = next_size,
              new_size = self.nodes[idx as usize].size,
              "Merged nodes"
          );
		
		self.recycle_node(next);
		idx
	}
	
	// ── Node pool ────────────────────────────────────────────────────────
	
	fn alloc_node(&mut self, node: MemNode) -> u32 {
		if let Some(idx) = self.recycled.pop() {
			self.nodes[idx as usize] = node;
			idx
		} else {
			let idx = self.nodes.len() as u32;
			self.nodes.push(node);
			idx
		}
	}
	
	fn recycle_node(&mut self, idx: u32) {
		self.nodes[idx as usize] = MemNode::default();
		self.recycled.push(idx);
	}
	
	pub fn dump_state(&self) {
		println!("block_size: {}", self.block_size);
		println!("free_bytes: {}", self.free_bytes);
		
		println!("--- physical chain ---");
		let mut head = None;
		
		for (i, n) in self.nodes.iter().enumerate() {
			if n.size > 0 && n.prev_phys == NULL && n.offset == 0 {
				head = Some(i as u32);
				break;
			}
		}
		
		let mut cur = head.unwrap_or(NULL);
		
		while cur != NULL {
			let n = &self.nodes[cur as usize];
			println!(
				"[{}] off={} size={} free={} prev={} next={}",
				cur, n.offset, n.size, n.is_free, n.prev_phys, n.next_phys
			);
			cur = n.next_phys;
		}
		
		println!("--- bins ---");
		for fl in 0..FL_COUNT {
			for sl in 0..SL_COUNT {
				let idx = self.bins[fl][sl];
				if idx != NULL {
					print!("bin[{}][{}]: ", fl, sl);
					let mut cur = idx;
					while cur != NULL {
						let n = &self.nodes[cur as usize];
						print!("{}(sz={}) -> ", cur, n.size);
						cur = n.next_free;
					}
					println!("NULL");
				}
			}
		}
		
		println!("=======================");
	}
}

impl FreeList {
	/// Verify all TLSF invariants. Panics on violation. Debug-only.
	pub fn check_invariants(&self) {
		let mut total_size = 0u64;
		let mut total_free = 0u64;
		
		// Find the physical head (offset 0, prev_phys NULL, size > 0).
		let head = self
			.nodes
			.iter()
			.enumerate()
			.find(|(_, n)| n.prev_phys == NULL && n.offset == 0 && n.size > 0);
		
		if let Some((head_pos, _)) = head {
			let mut current = head_pos as u32;
			let mut prev = NULL;
			
			while current != NULL {
				let node = &self.nodes[current as usize];
				// Reciprocal physical pointers.
				assert_eq!(
					node.prev_phys, prev,
					"Reciprocal phys pointer broken at node {current}"
				);
				
				total_size += node.size;
				if node.is_free {
					total_free += node.size;
				}
				
				// Contiguity + no adjacent free.
				if node.next_phys != NULL {
					let next = &self.nodes[node.next_phys as usize];
					assert_eq!(
						node.offset + node.size,
						next.offset,
						"Gap/overlap between nodes {current} and {}",
						node.next_phys
					);
					assert!(
						!(node.is_free && next.is_free),
						"Adjacent free nodes {} and {}",
						current,
						node.next_phys
					);
				}
				
				// Free state integrity.
				if !node.is_free {
					assert_eq!(node.prev_free, NULL, "Used node {current} has prev_free");
					assert_eq!(node.next_free, NULL, "Used node {current} has next_free");
				}
				
				prev = current;
				current = node.next_phys;
			}
		}
		
		// No gaps, no overlaps.
		assert_eq!(total_size, self.block_size, "Total size != block_size");
		// Conservation of memory.
		assert_eq!(total_free, self.free_bytes, "free_bytes mismatch");
		
		// Bitmap truth / clean masking / cascade masking.
		for fl in 0..FL_COUNT {
			for sl in 0..SL_COUNT {
				if self.bins[fl][sl] != NULL {
					assert_ne!(
						self.second_level_masks[fl] & (1u64 << sl),
						0,
						"Bitmap truth: bin ({fl},{sl}) occupied but bit clear"
					);
					assert_ne!(
						self.first_level_mask & (1u64 << fl),
						0,
						"FL bitmap truth: fl={fl} has occupied bin but bit clear"
					);
				} else {
					assert_eq!(
						self.second_level_masks[fl] & (1u64 << sl),
						0,
						"Clean masking: bin ({fl},{sl}) empty but bit set"
					);
				}
			}
			if self.second_level_masks[fl] == 0 {
				assert_eq!(
					self.first_level_mask & (1u64 << fl),
					0,
					"Cascade masking: fl={fl} all bins empty but FL bit set"
				);
			}
		}
		
		// Free list reciprocal pointers.
		for fl in 0..FL_COUNT {
			for sl in 0..SL_COUNT {
				let mut current = self.bins[fl][sl];
				let mut prev = NULL;
				while current != NULL {
					let node = &self.nodes[current as usize];
					assert!(node.is_free, "Non-free node in bin ({fl},{sl})");
					assert_eq!(
						node.prev_free, prev,
						"Free list reciprocal broken in bin ({fl},{sl})"
					);
					prev = current;
					current = node.next_free;
				}
			}
		}
	}
}
// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	
	// ── Mapping tests ────────────────────────────────────────────────────
	
	#[test]
	fn mapping_insert_small_regime() {
		// Small: size < 256, sl = (size - 16) >> 2
		assert_eq!(mapping_insert(16), (0, 0));
		assert_eq!(mapping_insert(20), (0, 1));
		assert_eq!(mapping_insert(24), (0, 2));
		assert_eq!(mapping_insert(32), (0, 4));
		assert_eq!(mapping_insert(255), (0, 59));
	}
	
	#[test]
	fn mapping_insert_large_regime() {
		// 256: raw_fl=8, fl=1, sl=0
		assert_eq!(mapping_insert(256), (1, 0));
		// 512: raw_fl=9, fl=2, sl=0
		assert_eq!(mapping_insert(512), (2, 0));
		// 1024: raw_fl=10, fl=3, sl=0
		assert_eq!(mapping_insert(1024), (3, 0));
		// 300: raw_fl=8, shift=2, sl=(300^256)>>2 = 44>>2 = 11
		assert_eq!(mapping_insert(300), (1, 11));
		// 511: raw_fl=8, sl=(511^256)>>2 = 255>>2 = 63
		assert_eq!(mapping_insert(511), (1, 63));
	}
	
	#[test]
	fn mapping_search_rounds_up() {
		// 256 exact → same bin (step=4, 256+3=259 → (1,0))
		assert_eq!(mapping_search(256), (1, 0));
		// 257 → rounds to next bin (257+3=260 → (1,1))
		assert_eq!(mapping_search(257), (1, 1));
		// Small→large transition: 253+3=256 → large regime (1,0)
		assert_eq!(mapping_search(253), (1, 0));
	}
	
	// ── Allocation tests ─────────────────────────────────────────────────
	
	#[test]
	fn basic_allocate_and_free() {
		let mut fl = FreeList::new(1024);
		assert_eq!(fl.free_bytes(), 1024);
		
		let (offset, idx) = fl.allocate(64, 1).unwrap();
		assert_eq!(offset, 0);
		assert_eq!(fl.free_bytes(), 1024 - 64);
		
		fl.free(idx);
		assert_eq!(fl.free_bytes(), 1024);
		fl.check_invariants();
	}
	
	#[test]
	fn aligned_allocation() {
		let mut fl = FreeList::new(4096);
		
		let (off1, _idx1) = fl.allocate(16, 1).unwrap();
		assert_eq!(off1, 0);
		
		// Next free offset is 16. Alignment 256 → aligns to 256.
		let (off2, _idx2) = fl.allocate(64, 256).unwrap();
		assert_eq!(off2 % 256, 0);
		assert_eq!(off2, 256);
		
		fl.check_invariants();
	}
	
	#[test]
	fn coalescing_all_directions() {
		let mut fl = FreeList::new(1024);
		
		let (_, a) = fl.allocate(256, 1).unwrap();
		let (_, b) = fl.allocate(256, 1).unwrap();
		let (_, c) = fl.allocate(256, 1).unwrap();
		assert_eq!(fl.free_bytes(), 256);
		
		// Free middle → no coalescing (neighbors are used).
		fl.free(b);
		assert_eq!(fl.free_bytes(), 512);
		fl.check_invariants();
		
		// Free left → coalesces a+b.
		fl.free(a);
		assert_eq!(fl.free_bytes(), 768);
		fl.check_invariants();
		
		// Free right → coalesces everything.
		fl.free(c);
		assert_eq!(fl.free_bytes(), 1024);
		assert!(fl.is_empty());
		fl.check_invariants();
	}
	
	#[test]
	fn exhaustion_returns_none() {
		let mut fl = FreeList::new(64);
		let (_, a) = fl.allocate(32, 1).unwrap();
		let (_, b) = fl.allocate(32, 1).unwrap();
		assert!(fl.allocate(16, 1).is_none());
		
		fl.free(a);
		fl.free(b);
		assert!(fl.is_empty());
		fl.check_invariants();
	}
	
	#[test]
	fn many_small_allocations() {
		let mut fl = FreeList::new(1024);
		let mut handles = Vec::new();
		
		// Fill with 16-byte allocations.
		while let Some((_, idx)) = fl.allocate(16, 1) {
			handles.push(idx);
		}
		assert_eq!(handles.len(), 64); // 1024 / 16
		assert_eq!(fl.free_bytes(), 0);
		
		// Free all — should coalesce back.
		for idx in handles {
			fl.free(idx);
		}
		assert!(fl.is_empty());
		fl.check_invariants();
	}
	
	#[test]
	fn alternating_free_coalesces() {
		let mut fl = FreeList::new(512);
		let (_, a) = fl.allocate(128, 1).unwrap();
		let (_, b) = fl.allocate(128, 1).unwrap();
		let (_, c) = fl.allocate(128, 1).unwrap();
		let (_, d) = fl.allocate(128, 1).unwrap();
		
		// Free b and d (non-adjacent).
		fl.free(b);
		fl.free(d);
		fl.check_invariants();
		
		// Free a → coalesces with b.
		fl.free(a);
		fl.check_invariants();
		
		// Free c → coalesces a+b+c+d = full block.
		fl.free(c);
		assert!(fl.is_empty());
		fl.check_invariants();
	}
	
	// ── Invariant checker ────────────────────────────────────────────────
	
	
}