mod pipeline;
mod renderbuilder;
pub mod cache;

use crate::core::{cmd::{
	CommandBuffer,
	Inside,
	Outside,
	Recording
}, Backend, CommandOps, RenderingDesc};
pub use renderbuilder::RenderingInfoBuilder;
use std::marker::PhantomData;


impl<'dev, B: Backend> CommandBuffer<'dev, Recording, B, Outside> {
	pub fn begin_rendering(
		self,
		desc: &RenderingDesc<B>,
	) -> CommandBuffer<'dev, Recording, B, Inside> {
		self.device.cmd_begin_rendering(self.buffer, desc);
		CommandBuffer {
			device: self.device,
			buffer: self.buffer,
			family: self.family(),
			_state: PhantomData,
			_render: PhantomData,
		}
	}
}

impl<'dev, B: Backend> CommandBuffer<'dev, Recording, B, Inside> {
	pub fn end_rendering(
		self,
	) -> CommandBuffer<'dev, Recording, B, Outside> {
		self.device.cmd_end_rendering(self.buffer);
		CommandBuffer {
			device: self.device,
			buffer: self.buffer,
			family: self.family(),
			_state: PhantomData,
			_render: PhantomData,
		}
	}
}