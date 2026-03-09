use crate::core::backend::{Backend, CommandOps};
use crate::core::backend::types::{PipelineBindPoint, Extent2D, Viewport, Rect2D, Offset2D};
use crate::core::cmd::Recording;
use crate::core::cmd::CommandBuffer;
use super::{Inside, Outside};

impl<B: Backend> CommandBuffer<'_, Recording, B, Inside> {
	pub fn bind_graphics_pipeline(&self, handle: B::Pipeline) {
		self.device.cmd_bind_pipeline(self.buffer, PipelineBindPoint::GRAPHICS, handle);
	}
	
	pub fn draw(&self, vertices: u32) {
		self.device.cmd_draw(self.buffer, vertices, 1, 0, 0);
	}
	
	pub fn draw_indexed(&self, index_count: u32, instance_count: u32, first_index: u32) {
		self.device.cmd_draw_indexed(self.buffer, index_count, instance_count, first_index, 0, 0);
	}
	
	pub fn set_viewport(&self, extent: Extent2D) {
		self.device.cmd_set_viewport(self.buffer, Viewport {
			x: 0.0,
			y: 0.0,
			width: extent.width() as f32,
			height: extent.height() as f32,
			min_depth: 0.0,
			max_depth: 1.0,
		});
	}
	
	pub fn set_scissor(&self, extent: Extent2D) {
		self.device.cmd_set_scissor(
			self.buffer,
			Rect2D::new(
				Offset2D::new(0, 0),
				extent,
			),
		);
	}
}

impl<B: Backend> CommandBuffer<'_, Recording, B, Outside> {
	pub fn bind_compute_pipeline(&self, handle: B::Pipeline) {
		self.device.cmd_bind_pipeline(self.buffer, PipelineBindPoint::COMPUTE, handle);
	}
	
	pub fn dispatch(&self, x: u32, y: u32, z: u32) {
		self.device.cmd_dispatch(self.buffer, x, y, z);
	}
}