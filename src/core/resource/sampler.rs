use crate::core::types::{Filter, SamplerAddressMode};
use crate::core::backend::{Backend, DeviceOps};
/// RAII wrapper for a Vulkan sampler.
pub struct Sampler<'dev, B: Backend> {
	device: &'dev B::Device,
	handle: B::Sampler,
}

impl<'dev, B: Backend> Sampler<'dev, B> {
	pub fn new(
		device: &'dev B::Device,
		filter: Filter,
		address: SamplerAddressMode,
	) -> Result<Self, B::Error> {
		let handle = device.create_sampler(filter, address)?;
		Ok(Self { device, handle })
	}
	
	pub fn handle(&self) -> B::Sampler {
		self.handle
	}
}
impl<B: Backend> Drop for Sampler<'_, B> {
	fn drop(&mut self) {
		self.device.destroy_sampler(self.handle);
	}
}