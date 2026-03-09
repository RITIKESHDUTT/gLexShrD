use crate::core::backend::{Backend, DeviceOps};

/// Binary semaphore for swapchain acquire/present synchronization.
///
/// Unlike TimelineSemaphore, binary semaphores have no counter —
/// they are signaled by one operation and waited on by the next.
pub struct BinarySemaphore<'dev, B: Backend> {
	device: &'dev B::Device,
	semaphore: B::Semaphore,
}

impl<'dev, B: Backend> BinarySemaphore<'dev, B> {
	pub fn new(device: &'dev B::Device) -> Result<Self, B::Error> {
		let semaphore = device.create_binary_semaphore()?;
		Ok(Self { device, semaphore })
	}
	pub fn handle(&self) -> B::Semaphore {
		self.semaphore
	}
}

impl<B: Backend> Drop for BinarySemaphore<'_, B> {
	fn drop(&mut self) {
		self.device.destroy_semaphore(self.semaphore);
	}
}