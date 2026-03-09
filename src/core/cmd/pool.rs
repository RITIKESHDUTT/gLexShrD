use std::marker::PhantomData;
use crate::core::{Backend, DeviceOps};
use crate::core::types::CommandPoolFlags;
use crate::core::type_state_queue::sealed::QueueHandle;
use super::state;
use super::CommandBuffer;

pub struct CommandPool<'dev, B: Backend> {
	device: &'dev B::Device,
	pool: B::CommandPool,
	family: u32,
}

impl<'dev, B: Backend> CommandPool<'dev, B> {
	pub fn new(
		device: &'dev B::Device,
		queue: &impl QueueHandle,
	) -> Result<Self, B::Error> {
		let family = queue.family();
		let pool = device.create_command_pool(family, CommandPoolFlags::RESET_COMMAND_BUFFER)?;
		Ok(Self { device, pool, family })
	}
	
	pub fn allocate(&self) -> Result<CommandBuffer<'dev, state::Initial, B>, B::Error> {
		let buffer = self.device.allocate_command_buffer(self.pool)?;
		Ok(CommandBuffer {
			device: self.device,
			buffer,
			family: self.family,
			_state: PhantomData,
			_render: PhantomData,
		})
	}
	
	pub fn handle(&self) -> B::CommandPool {
		self.pool
	}
}

impl<B: Backend> Drop for CommandPool<'_, B> {
	fn drop(&mut self) {
		self.device.destroy_command_pool(self.pool);
	}
}