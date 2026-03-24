use super::buf_state;
use super::img_state::TransferDst;
use super::Image;
use crate::core::backend::types::{AccessFlags2, BufferUsage, IndexType,
								  PipelineBindPoint, PipelineStageFlags2, ShaderStages, QUEUE_FAMILY_IGNORED};
use crate::core::backend::{Backend, BufferBarrierInfo2, CommandOps, DeviceOps};
use crate::core::cmd::{CommandBuffer, Inside, Outside, Recording};
use crate::core::resource::desc_state::{Bound, Updated};
use crate::core::resource::DescriptorSet;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use crate::core::Allocation;

/// # Buffer Typestate System
///
/// The `Buffer<S>` architecture models resource legality through the Rust type system.
/// Instead of checking states at runtime, it enforces Vulkan synchronization
/// requirements during compilation.buffers are long-lived entities that move through the system.
///
/// ### Core Guarantees
///
/// #### 1. Access Correctness (Read-After-Write)
/// The system prevents data hazards by forcing state transitions.
/// *   **Example:** A `Buffer<Undefined>` cannot be bound as an `IndexBuffer`.
/// *   **Example:** You cannot perform a shader read until the `into_transfer_dst`
///     write operation has been transitioned via `into_uniform` or `into_storage`.
///
/// #### 2. Queue Ownership Correctness
/// The `family` field tracks the "Current Owner" of the resource across execution domains.
/// *   **Handshake Modeling:** The system identifies if `self.family != cmd.family()`.
/// *   **Release/Acquire:** If families differ, the `barrier` logic automatically
///     encodes the required Vulkan Release/Acquire semantics to move the resource
///     between `PassDomain` queues (e.g., Transfer -> Graphics).
///
/// #### 3. Pipeline-Stage Correctness
/// Transitions explicitly define valid Source and Destination stages:
/// *   **Transfer -> Vertex:** `TRANSFER` stage write completion is synchronized
///     with `VERTEX_INPUT` stage attribute reading.
/// *   **Compute -> Transfer:** `COMPUTE_SHADER` stage writes are synchronized
///     with `TRANSFER` stage read/copy operations.
///
/// ### Resource-Centric History
/// Unlike a global state machine, the `Buffer` carries its own history. The
/// transition methods (`into_...`) act as a **Finite State Machine (FSM)** where
/// each node is a valid Vulkan state and each edge is a `vkCmdPipelineBarrier2`.
///
///
///
/// // Illegal: Cannot use as index buffer while in TransferDst state
/// // let buffer = my_buffer.allocate(...);
/// // cmd.bind_index_buffer(&buffer); // COMPILER ERROR
///
/// // Legal: Explicit transition required
///
/// ### Implementation Details
/// *   **Zero-Cost:** Uses `PhantomData<S>` to track state; the state does not
///     exist at runtime.
/// *   **Move Semantics:** Consumes the old state and returns the new state,
///     preventing "Double-Action" bugs on the same resource.
pub struct Buffer<'dev, S, B: Backend> {
	device: &'dev B::Device,
	handle: B::Buffer,
	sub_allocator: Option<B::Allocation>,
	logical_size: u64,
	pub(crate) family: u32, // "Current Owner"
	_state: PhantomData<S>,
}

impl<S, B: Backend> Buffer<'_, S, B> {
	pub fn handle(&self) -> B::Buffer { self.handle }
	pub fn size(&self) -> u64 { self.logical_size }
	pub fn family(&self) -> u32 { self.family }
	
	/// Bridges the Buffer wrapper to the underlying Arena Allocation offset.
	/// Without this, Descriptors and Viewports will always look at Byte 0.
	// --- critical: descriptor offset ---
	pub fn offset(&self) -> u64 {
		self.sub_allocator
			.as_ref()
			.map(|a| a.memory_offset())
			.unwrap_or(0)
	}
	
	// --- critical: descriptor range ---
	pub fn range(&self) -> u64 {
		self.sub_allocator
			.as_ref()
			.map(|a| a.size())
			.unwrap_or(self.logical_size)
	}
	
	// --- critical: memory handle (for debugging / binding validation) ---
	pub fn memory(&self) -> Option<B::DeviceMemory> {
		self.sub_allocator
			.as_ref()
			.map(|a| a.memory())
	}
}


// CHANGE: added `where` bound so sub.memory() resolves to B::DeviceMemory,
// which is what map_memory / unmap_memory expect.
impl<'dev, S, B: Backend> Buffer<'dev, S, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory>
{
	pub fn map<T>(&self, size: u64) -> Result<*mut T, B::Error> {
		let sub = self.sub_allocator.as_ref().expect("REASON");
		// CHANGE: was self.memory — now routed through the sub-allocation.
		self.device
			.map_memory(sub.memory(), sub.memory_offset(), size)
			.map(|p| p as *mut T)
	}
	
	pub fn with_mapped<T: Copy, F, R>(&self, count: usize, f: F) -> Result<R, B::Error> where
		F: FnOnce(&mut [T]) -> R,
	{
		let size = (count * std::mem::size_of::<T>()) as u64;
		let sub  = self.sub_allocator.as_ref().expect("with_mapped called on non-owning buffer");
		
		let ptr = self.device.map_memory(sub.memory(), sub.memory_offset(), size)?;
		let slice = unsafe { std::slice::from_raw_parts_mut(ptr as *mut T, count) };
		
		let result = f(slice);
		
		self.device.unmap_memory(sub.memory());
		
		Ok(result)
	}
}
impl<'dev, S, B: Backend> Buffer<'dev, S, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory>
{
	pub fn finalize_lifetime(&mut self, t: u64) {
		if let Some(sub) = self.sub_allocator.as_mut() {
			sub.finalize_lifetime(t);
		}
	}
}

// CHANGE: `allocate` no longer calls allocate_memory internally.
// The caller (GpuContext) already holds a SubAllocation from the arena;
// it passes it in here and Buffer binds the VkBuffer to it.
// Old signature: (device, size, usage, memory_type_index, family)
// New signature: (device, sub, usage, family)
impl<'dev, B: Backend> Buffer<'dev, buf_state::Undefined, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory, Buffer = B::Buffer>,
{
	pub fn allocate(
		device: &'dev B::Device,
		logical_size: u64,
		sub_allocator: B::Allocation,
		family: u32,
	) -> Result<Self, B::Error> {
		let handle = sub_allocator.buffer();
		
		Ok(Self {
			device,
			handle,
			sub_allocator: Some(sub_allocator),
			logical_size,
			family,
			_state: PhantomData,
		})
	}
}


// ─────────────────────────────────────────────────────────────
// Buffer state transitions (barriers happen outside render pass)
// ─────────────────────────────────────────────────────────────

impl<'dev, B: Backend> Buffer<'dev, buf_state::Undefined, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory>
{
	pub fn into_transfer_dst(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::TransferDst, B> {
		barrier(&self, cmd,
				PipelineStageFlags2::NONE, AccessFlags2::NONE,
				PipelineStageFlags2::TRANSFER, AccessFlags2::TRANSFER_WRITE,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_transfer_src(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::TransferSrc, B> {
		barrier(&self, cmd,
				PipelineStageFlags2::NONE, AccessFlags2::NONE,
				PipelineStageFlags2::TRANSFER, AccessFlags2::TRANSFER_READ,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_vertex_buffer(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::VertexBuffer, B> {
		barrier(&self, cmd,
				PipelineStageFlags2::NONE, AccessFlags2::NONE,
				PipelineStageFlags2::VERTEX_INPUT, AccessFlags2::VERTEX_ATTRIBUTE_READ,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_storage(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::StorageReadWrite, B> {
		barrier(&self, cmd,
				PipelineStageFlags2::NONE, AccessFlags2::NONE,
				PipelineStageFlags2::COMPUTE_SHADER,
				AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
}

fn barrier<S, R, B: Backend>(
	buffer: &Buffer<'_, S, B>,
	cmd: &CommandBuffer<'_, Recording, B, R>,
	src_stage: PipelineStageFlags2,
	src_access: AccessFlags2,
	dst_stage: PipelineStageFlags2,
	dst_access: AccessFlags2,
	src_family: u32,
	dst_family: u32,
) {
	let (src_f, dst_f) = if src_family == dst_family {
		(QUEUE_FAMILY_IGNORED, QUEUE_FAMILY_IGNORED)
	} else {
		(src_family, dst_family)
	};
	cmd.device().cmd_buffer_barrier(cmd.handle(), &[BufferBarrierInfo2 {
		buffer: buffer.handle,
		src_stage,
		src_access,
		dst_stage,
		dst_access,
		src_queue_family: src_f,
		dst_queue_family: dst_f,
	}]);
}

fn retype<'dev, Old, New, B: Backend>(b: Buffer<'dev, Old, B>) -> Buffer<'dev, New, B> {
	let mut b = ManuallyDrop::new(b);
	Buffer {
		device: b.device,
		handle: b.handle,
		// across the typestate transition. ManuallyDrop prevents double-drop.
		// take() moves the Option out and leaves None in its place.
		// ManuallyDrop ensures the original Buffer destructor never fires,
		// so the None left behind is never dropped either.
		sub_allocator: std::mem::take(&mut b.sub_allocator),
		logical_size: b.logical_size,
		family: b.family,
		_state: PhantomData,
	}
}

// ─────────────────────────────────────────────────────────────
// Transfer commands (outside render pass)
// ─────────────────────────────────────────────────────────────

impl<B: Backend> CommandBuffer<'_, Recording, B> {
	pub fn copy_buffer(
		&self,
		src: &Buffer<'_, buf_state::TransferSrc, B>,
		dst: &Buffer<'_, buf_state::TransferDst, B>,
		size: u64,
	) {
		let src_base = src.offset();
		let dst_base = dst.offset();
		
		self.device.cmd_copy_buffer(
			self.buffer,
			src.handle(),
			dst.handle(),
			src_base,
			dst_base,
			size,
		);
	}
	
	pub fn copy_buffer_to_image(
		&self,
		src: &Buffer<'_, buf_state::TransferSrc, B>,
		src_offset: u64,
		dst: &Image<'_, TransferDst, B>,
	) {
		let ext = dst.extent();
		
		debug_assert!(src_offset < src.size());
		let src_base = src.offset();
		
		self.device.cmd_copy_buffer_to_image(
			self.buffer,
			src.handle(),
			src_base + src_offset,
			dst.handle(),
			ext,
		);
	}
	
	pub fn copy_buffer_offset(
		&self,
		src: &Buffer<'_, buf_state::TransferSrc, B>,
		dst: &Buffer<'_, buf_state::TransferDst, B>,
		size: u64,
		src_offset: u64,
		dst_offset: u64,
	) {
		debug_assert!(src_offset + size <= src.size());
		debug_assert!(dst_offset + size <= dst.size());
		let src_base = src.offset();
		let dst_base = dst.offset();
		
		self.device.cmd_copy_buffer(
			self.buffer,
			src.handle(),
			dst.handle(),
			src_base + src_offset,
			dst_base + dst_offset,
			size,
		);
	}
}


// ─────────────────────────────────────────────────────────────
// Binding commands (inside render pass)
// ─────────────────────────────────────────────────────────────
// ─── Binding commands (Recording, Inside) ────────────────────

impl<B: Backend> CommandBuffer<'_, Recording, B, Inside> {
	pub fn bind_vertex_buffer(
		&self,
		vb: &Buffer<'_, buf_state::VertexBuffer, B>,
	) {
		self.device.cmd_bind_vertex_buffers(self.buffer, 0, &[vb.handle()], &[0]);
	}
	
	pub fn bind_index_buffer(
		&self,
		ib: &Buffer<'_, buf_state::IndexBuffer, B>,
	) {
		self.device.cmd_bind_index_buffer(self.buffer, ib.handle(), 0, IndexType::U32);
	}
	
	pub fn bind_descriptor_set<'d, Iface>(
		&self,
		pipeline_layout: B::PipelineLayout,
		set: DescriptorSet<'d, Updated, B, Iface>,
	) -> DescriptorSet<'d, Bound, B, Iface> {
		self.device.cmd_bind_descriptor_sets(
			self.buffer, PipelineBindPoint::GRAPHICS, pipeline_layout, 0,
			&[set.handle], &[],
		);
		DescriptorSet {
			device: set.device,
			handle: set.handle,
			_state: PhantomData,
			_iface: PhantomData,
		}
	}
	pub fn bind_descriptor_set_ref<Iface>(
		&self,
		pipeline_layout: B::PipelineLayout,
		set: &DescriptorSet<'_, Updated, B, Iface>,
	) {
		self.device.cmd_bind_descriptor_sets(
			self.buffer,
			PipelineBindPoint::GRAPHICS,
			pipeline_layout,
			0,
			&[set.handle],
			&[],
		);
	}
	
	pub fn push_constants<T: Copy>(
		&self,
		layout: B::PipelineLayout,
		stages: ShaderStages,
		offset: u32,
		data: &T,
	) {
		let bytes = unsafe {
			std::slice::from_raw_parts(data as *const T as *const u8, std::mem::size_of::<T>())
		};
		self.device.cmd_push_constants(self.buffer, layout, stages, offset, bytes);
	}
}


// ─── TransferSrc transitions ─────────────────────────────────

impl<'dev, B: Backend> Buffer<'dev, buf_state::TransferSrc, B> {
	pub fn into_transfer_dst(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::TransferDst, B> {
		let acq = self.family != cmd.family();
		barrier(&self, cmd,
				if acq { PipelineStageFlags2::NONE } else { PipelineStageFlags2::TRANSFER },
				if acq { AccessFlags2::NONE } else { AccessFlags2::TRANSFER_READ },
				PipelineStageFlags2::TRANSFER, AccessFlags2::TRANSFER_WRITE,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
}

// ─── StorageReadWrite transitions ────────────────────────────

impl<'dev, B: Backend> Buffer<'dev, buf_state::StorageReadWrite, B> {
	pub fn into_transfer_src(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::TransferSrc, B> {
		let acq = self.family != cmd.family();
		barrier(&self, cmd,
				if acq { PipelineStageFlags2::NONE } else { PipelineStageFlags2::COMPUTE_SHADER },
				if acq { AccessFlags2::NONE } else { AccessFlags2::SHADER_WRITE },
				PipelineStageFlags2::TRANSFER, AccessFlags2::TRANSFER_READ,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_storage(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::StorageReadWrite, B> {
		let acq = self.family != cmd.family();
		barrier(&self, cmd,
				if acq { PipelineStageFlags2::NONE } else { PipelineStageFlags2::COMPUTE_SHADER },
				if acq { AccessFlags2::NONE } else { AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE },
				PipelineStageFlags2::COMPUTE_SHADER,
				AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
}


// ─── VertexBuffer back to TransferDst ────────────────────────

impl<'dev, B: Backend> Buffer<'dev, buf_state::VertexBuffer, B> {
	pub fn into_transfer_dst(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::TransferDst, B> {
		let acq = self.family != cmd.family();
		barrier(&self, cmd,
				if acq { PipelineStageFlags2::NONE } else { PipelineStageFlags2::VERTEX_INPUT },
				if acq { AccessFlags2::NONE } else { AccessFlags2::VERTEX_ATTRIBUTE_READ },
				PipelineStageFlags2::TRANSFER, AccessFlags2::TRANSFER_WRITE,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
}

// ─── UniformReadOnly back to TransferDst ─────────────────────

impl<'dev, B: Backend> Buffer<'dev, buf_state::UniformReadOnly, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory> + Default
{
	pub fn into_transfer_dst(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::TransferDst, B> {
		let acq = self.family != cmd.family();
		barrier(&self, cmd,
				if acq { PipelineStageFlags2::NONE } else { PipelineStageFlags2::VERTEX_SHADER |
					PipelineStageFlags2::FRAGMENT_SHADER },
				if acq { AccessFlags2::NONE } else { AccessFlags2::UNIFORM_READ },
				PipelineStageFlags2::TRANSFER, AccessFlags2::TRANSFER_WRITE,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
}




// CHANGE: `staging` now takes a SubAllocation from the caller instead of
// calling allocate_memory internally. Same pattern as `allocate` above —
// GpuContext calls sub_alloc() first and passes the ticket in.
impl<'dev, B: Backend> Buffer<'dev, buf_state::TransferSrc, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory>
{
	pub fn staging(
		device: &'dev B::Device,
		logical_size: u64,
		sub_allocator:    B::Allocation,
		family: u32,
	) -> Result<Self, B::Error> {
		let offset = sub_allocator.memory_offset();
		let memory = sub_allocator.memory();
		
		let handle = device.create_buffer(logical_size, BufferUsage::TRANSFER_SRC)?;
		device.bind_buffer_memory(handle, memory, offset)?;
		
		Ok(Self {
			device,
			handle,
			sub_allocator: Some(sub_allocator),
			logical_size,
			family,
			_state: PhantomData,
		})
	}
	pub fn write<T: Copy>(&self, data: &[T]) -> Result<(), B::Error> {
		let bytes = std::mem::size_of_val(data) as u64;
		assert!(bytes <= self.logical_size, "data exceeds buffer logical size");
		self.with_mapped(data.len(), |slice| {
			slice.copy_from_slice(data);
		})
	}
}

//from_raw constructor to Buffer that allows the memory to be null or "ignored" for cases
// where the buffer doesn't own its memory (like a view into a staging buffer).
// CHANGE: from_raw_view no longer stores B::null_memory().
// Non-owning views (e.g. swapchain buffer views) do not participate in the
// arena at all. The sub field is filled with B::null_allocation() — a
// sentinel whose drop is a guaranteed no-op. This replaces the old
// `if self.memory != B::null_memory()` guard in Drop.
impl<'dev, S, B: Backend> Buffer<'dev, S, B> {
	pub(crate) fn from_raw_view(
		device: &'dev B::Device,
		handle: B::Buffer,
		size:   u64,
		family: u32,
	) -> Self {
		Self {
			device,
			handle,
			// CHANGE: was `memory: B::null_memory()` — now uses the null
			// allocation sentinel so Drop stays uniform (always calls
			// destroy_buffer, never free_memory).
			sub_allocator: None,
			logical_size: size,
			family,
			_state: PhantomData,
		}
	}
}


impl<'dev, B: Backend> Buffer<'dev, buf_state::TransferDst, B>
	where B::Allocation: Allocation<Memory = B::DeviceMemory>
{
	pub fn into_vertex_buffer(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::VertexBuffer, B> {
		let acq = self.family != cmd.family();
		barrier(&self, cmd,
				if acq { PipelineStageFlags2::NONE } else { PipelineStageFlags2::TRANSFER },
				if acq { AccessFlags2::NONE } else { AccessFlags2::TRANSFER_WRITE },
				PipelineStageFlags2::VERTEX_INPUT, AccessFlags2::VERTEX_ATTRIBUTE_READ,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn release_to_vertex(self, cmd: &CommandBuffer<'_, Recording, B>, dst_family: u32) -> Self {
		barrier(&self, cmd,
				PipelineStageFlags2::TRANSFER, AccessFlags2::TRANSFER_WRITE,
				PipelineStageFlags2::NONE, AccessFlags2::NONE,
				self.family, dst_family,
		);
		retype(self)
	}
	
	pub fn into_index_buffer(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::IndexBuffer, B> {
		let acq = self.family != cmd.family();
		barrier(&self, cmd,
				if acq { PipelineStageFlags2::NONE } else { PipelineStageFlags2::TRANSFER },
				if acq { AccessFlags2::NONE } else { AccessFlags2::TRANSFER_WRITE },
				PipelineStageFlags2::VERTEX_INPUT, AccessFlags2::INDEX_READ,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_uniform(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::UniformReadOnly, B> {
		let acq = self.family != cmd.family();
		barrier(&self, cmd,
				if acq { PipelineStageFlags2::NONE } else { PipelineStageFlags2::TRANSFER },
				if acq { AccessFlags2::NONE } else { AccessFlags2::TRANSFER_WRITE },
				PipelineStageFlags2::VERTEX_SHADER | PipelineStageFlags2::FRAGMENT_SHADER,
				AccessFlags2::UNIFORM_READ,
				self.family, cmd.family(),
		);
		self.family = cmd.family();
		retype(self)
	}
	
	pub fn into_storage(
		mut self,
		cmd: &CommandBuffer<'_, Recording, B>,
	) -> Buffer<'dev, buf_state::StorageReadWrite, B> {
		let acq = self.family != cmd.family();
		if acq {
			barrier(&self, cmd,
					PipelineStageFlags2::NONE, AccessFlags2::NONE,
					PipelineStageFlags2::COMPUTE_SHADER,
					AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
					self.family, cmd.family(),
			);
			self.family = cmd.family();
		}
		retype(self)
	}
	
	pub fn into_storage_release(
		self,
		cmd: &CommandBuffer<'_, Recording, B>,
		dst_family: u32,
	) -> Buffer<'dev, buf_state::StorageReadWrite, B> {
		barrier(&self, cmd,
				PipelineStageFlags2::TRANSFER, AccessFlags2::TRANSFER_WRITE,
				PipelineStageFlags2::NONE, AccessFlags2::NONE,
				self.family, dst_family,
		);
		retype(self)
	}
}

// ─── Compute binding (Recording, Outside) ────────────────────

impl<B: Backend> CommandBuffer<'_, Recording, B, Outside> {
	pub fn bind_compute_descriptor_set<'d, Iface>(
		&self,
		pipeline_layout: B::PipelineLayout,
		set: DescriptorSet<'d, Updated, B, Iface>,
	) -> DescriptorSet<'d, Bound, B, Iface> {
		self.device.cmd_bind_descriptor_sets(
			self.buffer, PipelineBindPoint::COMPUTE, pipeline_layout, 0,
			&[set.handle], &[],
		);
		DescriptorSet {
			device: set.device,
			handle: set.handle,
			_state: PhantomData,
			_iface: PhantomData,
		}
	}
	
	pub fn push_compute_constants<T: Copy>(
		&self,
		layout: B::PipelineLayout,
		stages: ShaderStages,
		offset: u32,
		data: &T,
	) {
		let bytes = unsafe {
			std::slice::from_raw_parts(data as *const T as *const u8, std::mem::size_of::<T>())
		};
		self.device.cmd_push_constants(self.buffer, layout, stages, offset, bytes);
	}
	
	pub fn pipeline_barrier_raw(&self, barriers: &[BufferBarrierInfo2<B>]) {
		self.device.cmd_buffer_barrier(self.buffer, barriers);
	}
}