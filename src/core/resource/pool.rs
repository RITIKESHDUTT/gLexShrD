use crate::core::backend::types::DescriptorPoolSize;
use crate::core::backend::{Backend, DeviceOps};
use crate::core::types::DescriptorType;

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
impl<'dev, B: Backend> DescriptorPool<'dev, B>
	where
		B::Device: DeviceOps<B>,
{
	pub fn compute_storage(
		device: &'dev B::Device,
		max_sets: u32,
	) -> Result<Self, B::Error> {
		Self::new(
			device,
			max_sets,
			&[DescriptorPoolSize {
				descriptor_type: DescriptorType::StorageBuffer,
				count: max_sets * 2, // read + write per set
			}],
		)
	}
}

impl<'dev, B: Backend> DescriptorPool<'dev, B>
	where
		B::Device: DeviceOps<B>,
{
	pub fn gfx_vertex_storage(
		device: &'dev B::Device,
		max_sets: u32,
	) -> Result<Self, B::Error> {
		Self::new(
			device,
			max_sets,
			&[DescriptorPoolSize {
				descriptor_type: DescriptorType::StorageBuffer,
				count: max_sets,
			}],
		)
	}
}

impl<B: Backend> Drop for DescriptorPool<'_, B> {
	fn drop(&mut self) {
		self.device.destroy_descriptor_pool(self.pool);
	}
}