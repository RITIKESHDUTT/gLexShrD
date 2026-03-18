use crate::core::PresentSync;
use crate::core::type_state_queue::{Graphics, Queue, Transfer};
use crate::core::Backend;
use crate::core::FrameSync;
use crate::core::WorkLane;
use crate::core::{Executor};
use crate::infra::vulkan::backend::{VulkanBackend, VulkanDevice};
use crate::infra::vulkan::context::init::VulkanContext;

pub struct GpuContext<'dev, B: Backend> {
	pub sync:     FrameSync<'dev, 3, VulkanBackend>,
	pub executor: Executor<'dev, B>,
}

impl<'dev,> GpuContext<'dev, VulkanBackend> {
	pub fn new(ctx: &'dev VulkanContext) -> Result<Self, <VulkanBackend as Backend>::Error> {
		let mut executor = Executor::new(ctx.device());
		
		// Graphics queue is mandatory.
		executor.attach_graphics(*ctx.lanes.graphics()).unwrap_or_else(|_e| unreachable!());
		
		if let Some(&cq) = ctx.lanes.compute() {
			executor.attach_compute(cq)?;
		}
		
		if let Some(&tq) = ctx.lanes.transfer(0) {
			executor.attach_transfer(tq)?;
		}
		
		let mut sync = FrameSync::<3, VulkanBackend>::new(ctx.device()).expect("FrameSync Dropped");
		sync.set_graphics_timeline(executor.graphics_lane().timeline_handle());
		
		Ok(Self { sync, executor })
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
	
	pub fn frame(&self) -> u64 {
		self.sync.frame()
	}
	
	pub fn current_slot(&self) -> usize {
		self.sync.current_slot()
	}
	
	pub fn drain(&self) -> Result<(), <VulkanBackend as Backend>::Error> {
		self.sync.drain()
	}
	
	pub fn acquire_semaphore(&self) -> <VulkanBackend as Backend>::Semaphore {
		self.sync.acquire_semaphore().handle()
	}
	
	pub fn present_sync(&self, render_finished: <VulkanBackend as Backend>::Semaphore) -> PresentSync<VulkanBackend> {
		self.sync.present_sync(render_finished)
	}
	// ── Executor delegates ────────────────────────────────────────────────────
	
	pub fn executor(&self) -> &Executor<'dev, VulkanBackend> {
		&self.executor
	}
	
	pub fn executor_mut(&mut self) -> &mut Executor<'dev, VulkanBackend> {
		&mut self.executor
	}
	
	pub fn has_transfer(&self) -> bool {
		self.executor.has_transfer()
	}
	
	pub fn transfer_lane(&self) -> &WorkLane<'dev, Queue<Transfer, VulkanBackend>, VulkanBackend> {
		self.executor.transfer_lane()
	}
	
	pub fn transfer_lane_mut(&mut self) -> &mut WorkLane<'dev, Queue<Transfer, VulkanBackend>, VulkanBackend> {
		self.executor.transfer_lane_mut()
	}
	
	pub fn graphics_lane(&self) -> &WorkLane<'dev, Queue<Graphics, VulkanBackend>, VulkanBackend> {
		self.executor.graphics_lane()
	}
	
	pub fn graphics_lane_mut(&mut self) -> &mut WorkLane<'dev, Queue<Graphics, VulkanBackend>, VulkanBackend> {
		self.executor.graphics_lane_mut()
	}
	
	pub fn device(&self) -> &'dev VulkanDevice {
		self.executor.device()
	}
	
	pub fn timeline_completed(&self) -> u64 {
		unsafe {
			self.executor.device()
				.inner.get_semaphore_counter_value(self.executor.graphics_timeline_handle())
				.expect("timeline query failed")
		}
	}
	
	pub fn last_graphics_signal(&self) -> u64 {
		self.executor.graphics_lane().last_signal_value()
	}

	pub fn bump_after_present(&mut self) -> Result<u64, <VulkanBackend as Backend>::Error> {
		let device = self.executor.device();
		self.executor.graphics_lane_mut().bump_timeline(device)
	}
	pub fn drain_for_configure(&mut self) -> Result<(), <VulkanBackend as Backend>::Error>{
		self.executor.device().wait_idle()
	}
}

impl<'dev, B: Backend> Drop for GpuContext<'dev, B> {
	fn drop(&mut self) {
		let _ = self.executor.device();
	}
}