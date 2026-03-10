use crate::core::backend::types::DescriptorPoolSize;
use crate::core::backend::{Backend, DeviceOps};

pub struct DescriptorPool<'dev, B: Backend> {
	device: &'dev B::Device,
	pool: B::DescriptorPool,
}

impl<'dev, B: Backend> DescriptorPool<'dev, B> {
	pub fn new(
		device: &'dev B::Device,
		max_sets: u32,
		pool_sizes: &[DescriptorPoolSize],
	) -> Result<Self, B::Error> {
		let pool = device.create_descriptor_pool(max_sets, pool_sizes)?;
		Ok(Self { device, pool })
	}
	
	pub fn handle(&self) -> B::DescriptorPool {
		self.pool
	}
}

impl<B: Backend> Drop for DescriptorPool<'_, B> {
	fn drop(&mut self) {
		self.device.destroy_descriptor_pool(self.pool);
	}
}