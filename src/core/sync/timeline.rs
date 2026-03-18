use crate::core::backend::{Backend, DeviceOps};

pub struct TimelineSemaphore<'dev, B: Backend> {
	device: &'dev B::Device,
	semaphore: B::Semaphore,
}

impl<'dev, B: Backend> TimelineSemaphore<'dev, B> {
	pub fn new(device: &'dev B::Device, initial: u64) -> Result<Self, B::Error> {
		let semaphore = device.create_timeline_semaphore(initial)?;
		Ok(Self { device, semaphore })
	}
	
	pub fn handle(&self) -> B::Semaphore {
		self.semaphore
	}
	
	
	pub fn wait(&self, value: u64) -> Result<(), B::Error> {
		self.device.wait_semaphore(self.semaphore, value)
	}
	
	
	pub fn signal_cpu(&self, value: u64) -> Result<(), B::Error> {
		self.device.signal_semaphore(self.semaphore, value)
	}

	/// Non-blocking query of GPU-side semaphore value.
	pub fn query(&self) -> Result<u64, B::Error> {
		self.device.query_semaphore(self.semaphore)
	}
}

impl<B: Backend> Drop for TimelineSemaphore<'_, B> {
	fn drop(&mut self) {
		self.device.destroy_semaphore(self.semaphore);
	}
}