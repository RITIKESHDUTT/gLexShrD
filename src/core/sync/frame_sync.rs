use crate::core::backend::{Backend, DeviceOps};
use tracing::{trace, warn};

pub struct FrameSync<'dev, const N: usize, B: Backend> {
	device: &'dev B::Device,
	slot_signal_values: [u64; N],
	graphics_timeline: B::Semaphore,
	frame: u64,
	current: usize,
}

impl<'dev, const N: usize, B: Backend> FrameSync<'dev, N, B> {
	pub fn new(device: &'dev B::Device) -> Result<Self, B::Error> {
		Ok(Self {
			device,
			slot_signal_values: [0u64; N],
			graphics_timeline: B::null_semaphore(),
			frame: 0,
			current: 0,
		})
	}
	
	pub fn drain(&self) -> Result<(), B::Error> {
		let max_val = self.slot_signal_values.iter().copied().max().unwrap_or(0);
		if max_val > 0 && self.graphics_timeline != B::null_semaphore() {
			trace!(max_val, "FrameSync::drain — waiting on timeline");
			self.device.wait_semaphore(self.graphics_timeline, max_val)?;
		}
		Ok(())
	}
	
	pub fn begin_frame(&self) -> Result<bool, B::Error> {
		let last_val = self.slot_signal_values[self.current];
		if self.frame >= N as u64 && last_val > 0 {
			let current_val = self.device.query_semaphore(self.graphics_timeline)?;
			if current_val < last_val {
				trace!(
                      frame = self.frame,
                      slot = self.current,
                      current_val,
                      last_val,
                      "Slot not ready"
                  );
				return Ok(false);
			}
		}
		trace!(
              frame = self.frame,
              slot = self.current,
              last_val,
              "Slot ready"
          );
		Ok(true)
	}
	
	pub fn set_graphics_timeline(&mut self, handle: B::Semaphore) {
		self.graphics_timeline = handle;
	}
	
	pub fn record_signal(&mut self, signal_val: u64) {
		trace!(
              frame = self.frame,
              slot = self.current,
              signal_val,
              "Recording signal"
          );
		self.slot_signal_values[self.current] = signal_val;
	}
	
	pub fn end_frame(&mut self) {
		let old_frame = self.frame;
		self.frame += 1;
		self.current = (self.frame % N as u64) as usize;
		trace!(
              old_frame,
              new_frame = self.frame,
              new_slot = self.current,
              "Frame advanced"
          );
	}
	
	pub fn frame(&self) -> u64 { self.frame }
	pub fn current_slot(&self) -> usize { self.current }
}

impl<const N: usize, B: Backend> Drop for FrameSync<'_, N, B> {
	fn drop(&mut self) {
		let max_val = self.slot_signal_values.iter().copied().max().unwrap_or(0);
		if max_val > 0 && self.graphics_timeline != B::null_semaphore() {
			let _ = self.device.wait_semaphore(self.graphics_timeline, max_val);
		}
	}
}