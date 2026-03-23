use crate::core::{Sampler};
use crate::core::types::{SamplerAddressMode, Filter};
use crate::core::types::Extent2D;
use crate::core::img_state;
use crate::core::Image;
use crate::core::{Backend, DeviceOps};
use crate::core::buf_state::Undefined;
use crate::core::types::{BufferUsage, MemoryPropertyFlags};
use crate::core::type_state_queue::{Graphics, Queue, Transfer};
use crate::core::{Buffer, Executor, FrameSync, PresentSync, WorkLane};
use crate::domain::ResourceKind;
use crate::infra::vulkan::backend::{VulkanBackend, VulkanDevice};
use crate::infra::vulkan::context::init::VulkanContext;
use crate::infra::vulkan::memory::{AllocationError, GpuAllocator};
use std::sync::Arc;
use crate::core::types::ImageUsage;
use tracing::{debug, info, trace, warn};

pub struct GpuContext<'dev, B: Backend> {
	pub sync:      FrameSync<'dev, 3, VulkanBackend>,
	pub executor:  Executor<'dev, B>,
	pub allocator: Arc<GpuAllocator>,
}

impl<'dev> GpuContext<'dev, VulkanBackend> {
	pub fn new(ctx: &'dev VulkanContext) -> Result<Self, <VulkanBackend as Backend>::Error> {
		info!("GpuContext::new — initializing");
		
		let mut executor = Executor::new(ctx.device());
		
		executor.attach_graphics(*ctx.lanes.graphics())
				.unwrap_or_else(|_| unreachable!());
		debug!("Graphics lane attached");
		
		if let Some(&cq) = ctx.lanes.compute() {
			executor.attach_compute(cq)?;
			debug!("Compute lane attached");
		}
		if let Some(&tq) = ctx.lanes.transfer(0) {
			executor.attach_transfer(tq)?;
			debug!("Transfer lane attached");
		}
		
		let mut sync = FrameSync::<3, VulkanBackend>::new(ctx.device())
			.expect("FrameSync creation failed");
		sync.set_graphics_timeline(executor.graphics_lane().timeline_handle());
		debug!("FrameSync created, timeline wired to graphics lane");
		
		let memory_properties = ctx.physical().memory_properties(ctx.instance());
		let allocator = GpuAllocator::new(ctx.device_arc(), memory_properties);
		debug!("GpuAllocator created");
		
		info!("GpuContext::new — complete");
		Ok(Self { sync, executor, allocator })
	}
	
	// ── Allocator ─────────────────────────────────────────────────────────────
	
	pub fn tick_allocator(&self) {
		let gpu_t = self.timeline_completed();
		trace!(gpu_t, "tick_allocator — reap + flush");
		self.allocator.reap(gpu_t);
		self.allocator.flush_device_frees();
	}
	
	// ── Buffer helpers ────────────────────────────────────────────────────────
	
	pub fn staging_upload(
		&self,
		size:   u64,
		family: u32,
	) -> Result<Buffer<'dev, Undefined, VulkanBackend>, AllocationError> {
		trace!(size, family, "staging_upload");
		let usage = BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC;
		let sub   = self.sub_alloc(size, usage, MemoryPropertyFlags::HOST_VISIBLE |
			MemoryPropertyFlags::HOST_COHERENT)?;
		Buffer::allocate(self.device(), size, sub, family)
			.map_err(AllocationError::DeviceOom)
	}
	
	pub fn vertex_buffer(
		&self,
		size:   u64,
		family: u32,
	) -> Result<Buffer<'dev, Undefined, VulkanBackend>, AllocationError> {
		trace!(size, family, "vertex_buffer");
		let usage = BufferUsage::VERTEX | BufferUsage::TRANSFER_DST;
		let sub   = self.sub_alloc(size, usage, MemoryPropertyFlags::DEVICE_LOCAL)?;
		Buffer::allocate(self.device(), size, sub, family)
			.map_err(AllocationError::DeviceOom)
	}
	
	pub fn storage_buffer(
		&self,
		size:   u64,
		family: u32,
	) -> Result<Buffer<'dev, Undefined, VulkanBackend>, AllocationError> {
		trace!(size, family, "storage_buffer");
		let usage = BufferUsage::STORAGE | BufferUsage::TRANSFER_DST;
		let sub   = self.sub_alloc(size, usage, MemoryPropertyFlags::DEVICE_LOCAL)?;
		Buffer::allocate(self.device(), size, sub, family)
			.map_err(AllocationError::DeviceOom)
	}
	
	pub fn staging_from_slice<T: Copy>(
		&self,
		data:   &[T],
		family: u32,
	) -> Result<Buffer<'dev, Undefined, VulkanBackend>, AllocationError> {
		let size = (data.len() * std::mem::size_of::<T>()) as u64;
		trace!(
              elements = data.len(),
              elem_size = std::mem::size_of::<T>(),
              total_size = size,
              family,
              "staging_from_slice"
          );
		let buf  = self.staging_upload(size, family)?;
		buf.with_mapped::<T, _, _>(data.len(), |dst| dst.copy_from_slice(data))
		   .map_err(AllocationError::DeviceOom)?;
		Ok(buf)
	}
	
	// ── FrameSync delegates ───────────────────────────────────────────────────
	
	pub fn begin_frame(&self) -> Result<bool, <VulkanBackend as Backend>::Error> {
		self.sync.begin_frame()
	}
	
	pub fn end_frame(&mut self) {
		self.sync.end_frame();
	}
	
	pub fn record_signal(&mut self, signal_val: u64) {
		self.sync.record_signal(signal_val);
	}
	
	pub fn frame(&self)        -> u64   { self.sync.frame()        }
	pub fn current_slot(&self) -> usize { self.sync.current_slot() }
	
	pub fn drain(&self) -> Result<(), <VulkanBackend as Backend>::Error> {
		debug!("GpuContext::drain — waiting for all in-flight work");
		self.sync.drain()
	}
	
	// ── Executor delegates ────────────────────────────────────────────────────
	
	pub fn executor(&self)     -> &Executor<'dev, VulkanBackend>     { &self.executor }
	pub fn executor_mut(&mut self) -> &mut Executor<'dev, VulkanBackend> { &mut self.executor }
	
	pub fn has_transfer(&self) -> bool { self.executor.has_transfer() }
	
	pub fn transfer_lane(&self)
						 -> &WorkLane<'dev, Queue<Transfer, VulkanBackend>, VulkanBackend>
	{
		self.executor.transfer_lane()
	}
	
	pub fn transfer_lane_mut(&mut self)
							 -> &mut WorkLane<'dev, Queue<Transfer, VulkanBackend>, VulkanBackend>
	{
		self.executor.transfer_lane_mut()
	}
	
	pub fn graphics_lane(&self)
						 -> &WorkLane<'dev, Queue<Graphics, VulkanBackend>, VulkanBackend>
	{
		self.executor.graphics_lane()
	}
	
	pub fn graphics_lane_mut(&mut self)
							 -> &mut WorkLane<'dev, Queue<Graphics, VulkanBackend>, VulkanBackend>
	{
		self.executor.graphics_lane_mut()
	}
	
	pub fn device(&self) -> &'dev VulkanDevice { self.executor.device() }
	
	pub fn timeline_completed(&self) -> u64 {
		unsafe {
			self.executor.device()
				.inner
				.get_semaphore_counter_value(self.executor.graphics_timeline_handle())
				.expect("timeline query failed")
		}
	}
	
	pub fn last_graphics_signal(&self) -> u64 {
		self.executor.graphics_lane().last_signal_value()
	}
	
	// ── Private ───────────────────────────────────────────────────────────────
	
	fn sub_alloc(
		&self,
		size:   u64,
		usage:  BufferUsage,
		flags:  MemoryPropertyFlags,
	) -> Result<<VulkanBackend as Backend>::Allocation, AllocationError> {
		trace!(size, ?flags, "sub_alloc — probing memory requirements");
		
		let tmp = self.device().create_buffer(size, usage).map_err(AllocationError::DeviceOom)?;
		let req = self.device().get_buffer_memory_requirements(tmp);
		self.device().destroy_buffer(tmp);
		
		trace!(
              req_size = req.size,
              req_align = req.alignment,
              type_bits = req.memory_type_bits,
              "sub_alloc — requirements probed, allocating"
          );
		
		self.allocator.allocate(req, flags, ResourceKind::Buffer, self.timeline_completed())
	}
	
	
	// ── Image helpers ─────────────────────────────────────────────────────────
	
	pub fn allocate_image_2d(
		&self,
		format: <VulkanBackend as Backend>::Format,
		extent: Extent2D,
		usage:  ImageUsage,
		family: u32,
	) -> Result<Image<'dev, img_state::Undefined, VulkanBackend>, AllocationError> {
		trace!(
              width = extent.width(),
              height = extent.height(),
              family,
              "allocate_image_2d — probing memory requirements"
          );
		
		let tmp = self.device()
					  .create_image_2d(format, extent.width(), extent.height(), usage)
					  .map_err(AllocationError::DeviceOom)?;
		let req = self.device().get_image_memory_requirements(tmp);
		self.device().destroy_image(tmp);
		
		trace!(
              req_size = req.size,
              req_align = req.alignment,
              type_bits = req.memory_type_bits,
              "allocate_image_2d — requirements probed, allocating"
          );
		
		let sub = self.allocator.allocate(
			req,
			MemoryPropertyFlags::DEVICE_LOCAL,
			ResourceKind::Image,
			self.timeline_completed(),
		)?;
		
		Image::allocate_2d(self.device(), sub, format, extent, usage, family)
			.map_err(AllocationError::DeviceOom)
	}
	
	pub fn create_sampler(
		&self,
		filter: Filter,
		address: SamplerAddressMode,
	) -> Result<Sampler<'dev, VulkanBackend>, <VulkanBackend as Backend>::Error> {
		trace!("create_sampler");
		Sampler::new(self.executor.device(), filter, address)
	}
}

impl<'dev, B: Backend> Drop for GpuContext<'dev, B> {
	fn drop(&mut self) {
		debug!("GpuContext::drop");
		let _ = self.executor.device();
	}
}