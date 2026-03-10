use super::state;
use crate::core::backend::{Backend, BufferBarrierInfo2, CommandOps, ImageBarrierInfo};
use crate::core::types::{CommandBufferUsageFlags, Rect2D, Viewport};
use std::marker::PhantomData;

pub struct CommandBuffer<'dev, S, B: Backend, R = state::Outside> {
	pub device: &'dev B::Device,
	pub(crate) buffer: B::CommandBuffer,
	pub(crate) family: u32,
	pub(crate) _state: PhantomData<S>,
	pub(crate) _render: PhantomData<R>,
}

// Handle access: available in any state, any render scope
impl<S, R, B: Backend> CommandBuffer<'_, S, B, R> {
	pub fn handle(&self) -> B::CommandBuffer {
		self.buffer
	}
	pub fn family(&self) -> u32 {
		self.family
	}
	pub fn device(&self) -> &B::Device { self.device }
	pub fn image_barrier(&self, barriers: &[ImageBarrierInfo<B>]) {
		self.device.cmd_image_barrier(self.buffer, barriers);
	}
	pub fn buffer_barrier(&self, barriers: &[BufferBarrierInfo2<B>]) {
		self.device.cmd_buffer_barrier(self.buffer, barriers);
	}
}


// ─────────────────────────────────────────────────────────────
// Initial → Recording (starts outside render pass)
// ─────────────────────────────────────────────────────────────
impl<'dev, B: Backend> CommandBuffer<'dev, state::Initial, B> {
	pub fn begin(self) -> Result<CommandBuffer<'dev, state::Recording, B>, B::Error> {
		self.device.begin_command_buffer(self.buffer, CommandBufferUsageFlags::ONE_TIME_SUBMIT)?;
		Ok(CommandBuffer {
			device: self.device,
			buffer: self.buffer,
			family: self.family,
			_state: PhantomData,
			_render: PhantomData,
		})
	}
}


// ─────────────────────────────────────────────────────────────
// Recording → Executable (only when outside render pass)
// ─────────────────────────────────────────────────────────────
impl<'dev, B: Backend> CommandBuffer<'dev, state::Recording, B> {
	pub fn end(self) -> Result<CommandBuffer<'dev, state::Executable, B>, B::Error> {
		self.device.end_command_buffer(self.buffer)?;
		Ok(CommandBuffer {
			device: self.device,
			buffer: self.buffer,
			family: self.family,
			_state: PhantomData,
			_render: PhantomData,
		})
	}
}

// ─────────────────────────────────────────────────────────────
// Dynamic State (Recording, any render scope)
// ─────────────────────────────────────────────────────────────
impl<'dev, R, B: Backend> CommandBuffer<'dev, state::Recording, B, R> {
	pub fn set_viewport_rect(&self, viewport: Viewport) {
		self.device.cmd_set_viewport(self.buffer, viewport);
	}
	
	pub fn set_scissor_rect(&self, rect: Rect2D) {
		self.device.cmd_set_scissor(self.buffer, rect);
	}
}