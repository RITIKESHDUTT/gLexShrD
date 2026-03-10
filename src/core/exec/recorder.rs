use crate::core::backend::types::{Extent2D, Offset2D, PipelineBindPoint, Rect2D, ShaderStages, Viewport};
use crate::core::backend::{Backend, CommandOps};
use crate::core::cmd::CommandBuffer;
use crate::core::cmd::{Inside, Outside, Recording};
use crate::core::render::cache::{PipelineId, PipelineManager};
use crate::core::resource::{buf_state, img_state, Buffer, Image};
use crate::core::resource::{desc_state::{Bound, Updated}, DescriptorSet};

pub struct RenderRecorder2D<'a, 'dev, B: Backend> {
	pub(crate) cmd: &'a CommandBuffer<'dev, Recording, B, Inside>,
	pub(crate) pipelines: &'a PipelineManager<'dev, B>,
	pub(crate) offset_stack: Vec<[f32; 2]>,
	pub(crate) layout: Option<B::PipelineLayout>,
}

pub struct ComputeRecorder<'a, 'dev, B: Backend> {
	pub(crate) cmd: &'a CommandBuffer<'dev, Recording, B, Outside>,
	pub(crate) pipelines: &'a PipelineManager<'dev, B>,
	pub(crate) layout: Option<B::PipelineLayout>,
}

pub struct TransferRecorder<'a, 'dev, B: Backend> {
	pub(crate) cmd: &'a CommandBuffer<'dev, Recording, B, Outside>,
}

// ─── RenderRecorder2D ────────────────────────────────────────

impl<'a, 'dev, B: Backend> RenderRecorder2D<'a, 'dev, B> {
	pub fn new(
		cmd: &'a CommandBuffer<'dev, Recording, B, Inside>,
		pipelines: &'a PipelineManager<'dev, B>,
		layout: Option<B::PipelineLayout>,
	) -> Self {
		Self { cmd, pipelines, offset_stack: vec![[0.0, 0.0]], layout }
	}
	
	#[inline]
	pub fn push_offset(&mut self, x: f32, y: f32) {
		let base = self.offset_stack.last().copied().unwrap_or([0.0, 0.0]);
		self.offset_stack.push([base[0] + x, base[1] + y]);
	}
	
	#[inline]
	pub fn pop_offset(&mut self) {
		debug_assert!(!self.offset_stack.is_empty(), "pop_offset called with empty offset stack");
		self.offset_stack.pop();
	}
	
	#[inline]
	pub fn current_offset(&self) -> [f32; 2] {
		self.offset_stack.last().copied().unwrap_or([0.0, 0.0])
	}
	
	pub fn draw(&self, vertices: u32) {
		self.cmd.draw(vertices);
	}
	
	pub fn draw_indexed(&self, index_count: u32, instance_count: u32, first_index: u32) {
		self.cmd.draw_indexed(index_count, instance_count, first_index);
	}
	
	pub fn set_viewport(&self, extent: Extent2D) {
		self.cmd.set_viewport(extent);
	}
	
	pub fn set_scissor(&self, extent: Extent2D) {
		self.cmd.set_scissor(extent);
	}
	
	pub fn bind_vertex_buffer(&self, vb: &Buffer<'_, buf_state::VertexBuffer, B>) {
		self.cmd.bind_vertex_buffer(vb);
	}
	
	pub fn bind_index_buffer(&self, ib: &Buffer<'_, buf_state::IndexBuffer, B>) {
		self.cmd.bind_index_buffer(ib);
	}
	
	pub fn bind_descriptor_set<'d, Iface>(
		&self,
		set: DescriptorSet<'d, Updated, B, Iface>,
	) -> DescriptorSet<'d, Bound, B, Iface> {
		self.cmd.bind_descriptor_set(self.layout.unwrap(), set)
	}
	
	pub fn push_constants<T: Copy>(&self, stages: ShaderStages, offset: u32, data: &T) {
		let layout = self.layout.expect("Error");
		self.cmd.push_constants(layout, stages, offset, data);
	}
	
	pub fn bind_pipeline(&mut self, id: PipelineId) {
		self.cmd.bind_graphics_pipeline(self.pipelines.handle(id));
		self.layout = Some(self.pipelines.layout(id));
	}
	
	pub fn set_viewport_region(&self, x: f32, y: f32, width: f32, height: f32) {
		self.cmd.device().cmd_set_viewport(self.cmd.handle(), Viewport {
			x, y, width, height, min_depth: 0.0, max_depth: 1.0,
		});
	}
	
	pub fn set_scissor_region(&self, x: i32, y: i32, width: u32, height: u32) {
		self.cmd.device().cmd_set_scissor(
			self.cmd.handle(),
			Rect2D::new(
				Offset2D::new(x, y),
				Extent2D::new(width, height),
			),
		);
	}
	pub fn bind_descriptor_set_ref<Iface>(
		&self,
		set: &DescriptorSet<'_, Updated, B, Iface>,
	) {
		self.cmd.bind_descriptor_set_ref(self.layout.expect("No pipeline layout"), set);
	}
	
	pub(crate) fn bind_descriptor_set_raw(&self, set: B::DescriptorSet) {
		self.cmd.device().cmd_bind_descriptor_sets(
			self.cmd.handle(), PipelineBindPoint::GRAPHICS,
			self.layout.expect("Error: No pipeline layout"),
			0, &[set], &[],
		);
	}
}

// ─── ComputeRecorder ─────────────────────────────────────────

impl<'a, 'dev, B: Backend> ComputeRecorder<'a, 'dev, B> {
	pub fn dispatch(&self, x: u32, y: u32, z: u32) {
		self.cmd.dispatch(x, y, z);
	}
	
	pub fn bind_compute_descriptor_set<'d, Iface>(
		&self,
		set: DescriptorSet<'d, Updated, B, Iface>,
	) -> DescriptorSet<'d, Bound, B, Iface> {
		let layout = self.layout.expect("Context Error: No pipeline layout found.");
		self.cmd.bind_compute_descriptor_set(layout, set)
	}
	
	pub fn push_compute_constants<T: Copy>(&self, data: &T) {
		let layout = self.layout.expect("Error: No pipeline layout found for this compute pass.");
		self.cmd.push_compute_constants(layout, ShaderStages::COMPUTE, 0, data);
	}
	
	pub fn bind_pipeline(&mut self, id: PipelineId) {
		self.cmd.bind_compute_pipeline(self.pipelines.handle(id));
		self.layout = Some(self.pipelines.layout(id));
	}
	
	pub(crate) fn bind_compute_descriptor_set_raw(&self, set: B::DescriptorSet) {
		self.cmd.device().cmd_bind_descriptor_sets(
			self.cmd.handle(), PipelineBindPoint::COMPUTE,
			self.layout.expect("Error: No pipeline layout"),
			0, &[set], &[],
		);
	}
}

// ─── TransferRecorder ────────────────────────────────────────

impl<'a, 'dev, B: Backend> TransferRecorder<'a, 'dev, B> {
	pub fn copy_buffer(
		&mut self,
		src: &Buffer<'_, buf_state::TransferSrc, B>,
		dst: &Buffer<'_, buf_state::TransferDst, B>,
		size: u64,
		src_offset: u64,
		dst_offset: u64,
	) {
		self.cmd.copy_buffer_offset(src, dst, size, src_offset, dst_offset);
	}
	
	pub fn copy_buffer_to_image(
		&self,
		src: &Buffer<'_, buf_state::TransferSrc, B>,
		dst: &Image<'_, img_state::TransferDst, B>,
	) {
		self.cmd.copy_buffer_to_image(src, dst);
	}
	
	pub fn copy_buffer_raw(&self, src: B::Buffer, dst: B::Buffer, size: u64) {
		self.cmd.device().cmd_copy_buffer(self.cmd.handle(), src, dst, 0, 0, size);
	}
	
	pub fn copy_buffer_raw_offset(&self, src: B::Buffer, dst: B::Buffer, size: u64, dst_offset: u64) {
		self.cmd.device().cmd_copy_buffer(self.cmd.handle(), src, dst, 0, dst_offset, size);
	}
}