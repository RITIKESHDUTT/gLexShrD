use crate::core::PresentSync;
use super::BinarySemaphore;
use crate::core::backend::{Backend, DeviceOps};


pub struct FrameSync<'dev, const N: usize, B: Backend> {
	device: &'dev B::Device,
	acquire_semaphores: [BinarySemaphore<'dev, B>; N],
	render_semaphores: [BinarySemaphore<'dev, B>; N],
	slot_signal_values: [u64; N],
	graphics_timeline: B::Semaphore,
	frame: u64,
	current: usize,
}


impl<'dev, const N: usize, B: Backend> FrameSync<'dev, N, B> {
	pub fn new(device: &'dev B::Device) -> Result<Self, B::Error> {
		let acquire_semaphores: Vec<_> = (0..N)
			.map(|_| BinarySemaphore::new(device))
			.collect::<Result<_, _>>()?;
		let render_semaphores: Vec<_> = (0..N)
			.map(|_| BinarySemaphore::new(device))
			.collect::<Result<_, _>>()?;
		
		Ok(Self {
			device,
			acquire_semaphores: acquire_semaphores
				.try_into()
				.unwrap_or_else(|_| unreachable!()),
			render_semaphores: render_semaphores
				.try_into()
				.unwrap_or_else(|_| unreachable!()),
			slot_signal_values: [0u64; N],
			graphics_timeline: B::null_semaphore(),
			frame: 0,
			current: 0,
		})
	}
	
	pub fn drain(&self) -> Result<(), B::Error> {
		let max_val = self.slot_signal_values.iter().copied().max().unwrap_or(0);
		if max_val > 0 && self.graphics_timeline != B::null_semaphore() {
			self.device.wait_semaphore(self.graphics_timeline, max_val)?;
		}
		Ok(())
	}
	
	
	/// Wait until this frame slot's previous work is complete.
	///
	/// If frame < N, all slots are fresh — no wait needed.
	/// Otherwise, waits for the GPU to finish the frame that last used this slot.
	pub fn begin_frame(&self) -> Result<bool, B::Error> {
		let last_val = self.slot_signal_values[self.current];
		if self.frame >= N as u64 && last_val > 0 {
			let current_val = self.device.query_semaphore(self.graphics_timeline)?;
			if current_val < last_val {
				return Ok(false);
			}
		}
		Ok(true)
	}
	
	
	
	pub fn set_graphics_timeline(&mut self, handle: B::Semaphore) {
		self.graphics_timeline = handle;
	}


	/// Binary semaphore for swapchain acquire (current slot).
	pub fn acquire_semaphore(&self) -> &BinarySemaphore<'dev, B> {
		&self.acquire_semaphores[self.current]
	}
	/// Binary semaphore for render finished signal (current slot).
	pub fn render_semaphore(&self) -> B::Semaphore {
		self.render_semaphores[self.current].handle()
	}
	/// Call after executor.execute(), passing the WorkLane's last signal value.
	/// Records it against the current slot so begin_frame can wait on it next time.
	pub fn record_signal(&mut self, signal_val: u64) {
		self.slot_signal_values[self.current] = signal_val;
	}
	

	/// Advance to the next frame slot. Call after present.
	pub fn end_frame(&mut self) {
		self.frame += 1;
		self.current = (self.frame % N as u64) as usize;
	}
	
	pub fn frame(&self) -> u64 {
		self.frame
	}
	
	pub fn present_sync(&self, render_finished: B::Semaphore) -> PresentSync<B> {
		PresentSync {
			wait_acquire: self.acquire_semaphores[self.current].handle(),
			signal_render_finished: render_finished,
		}
	}
	/// Returns the current slot index (frame % N).
	pub fn current_slot(&self) -> usize {
		self.current
	}
	
	
}

impl<const N: usize, B: Backend> Drop for FrameSync<'_, N, B> {
	fn drop(&mut self) {
		let max_val = self.slot_signal_values.iter().copied().max().unwrap_or(0);
		if max_val > 0 && self.graphics_timeline != B::null_semaphore() {
			let _ = self.device.wait_semaphore(self.graphics_timeline, max_val);
		}
	}
}