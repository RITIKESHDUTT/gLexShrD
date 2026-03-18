use super::{Inside, Outside};
use crate::core::backend::types::{Extent2D, Offset2D, PipelineBindPoint, Rect2D, Viewport};
use crate::core::backend::{Backend, CommandOps};
use crate::core::cmd::CommandBuffer;
use crate::core::cmd::Recording;

impl<B: Backend> CommandBuffer<'_, Recording, B, Inside> {
	pub fn bind_graphics_pipeline(&self, handle: B::Pipeline) {
		self.device.cmd_bind_pipeline(self.buffer, PipelineBindPoint::GRAPHICS, handle);
	}
	pub fn set_viewport_rect(&self, x: f32, y: f32, w: f32, h: f32) {
		self.device.cmd_set_viewport(self.buffer, Viewport { x, y, width: w, height: h, min_depth: 0.0, max_depth: 1.0, });
	}
	
	pub fn set_scissor_rect(&self, x: i32, y: i32, w: u32, h: u32) {
		self.device.cmd_set_scissor(
			self.buffer,
			Rect2D::new(
				Offset2D::new(x, y),
				Extent2D::new(w, h),
			),
		);
	}
	pub fn draw(&self, vertices: u32) {
		self.device.cmd_draw(self.buffer, vertices, 1, 0, 0);
	}
	
	pub fn draw_indexed(&self, index_count: u32, instance_count: u32, first_index: u32) {
		self.device.cmd_draw_indexed(self.buffer, index_count, instance_count, first_index, 0, 0);
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