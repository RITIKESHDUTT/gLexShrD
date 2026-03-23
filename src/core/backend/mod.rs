pub mod types;
mod helpers_for_types;
pub use helpers_for_types::{data_size, push_range, push_size, shader_stages, vertex_attr, vertex_binding, vertex_config, vertex_stride};
use std::error::Error;
use std::fmt::Debug;
use crate::domain::{Access, ImageLayout, Stage};
use types::*;

// ── Backend Trait ───────────────────────────────────────────
pub trait Backend: 'static + Sized
	where Self::Allocation: Allocation<Memory = Self::DeviceMemory, Buffer = Self::Buffer>,
{
	type Device: DeviceOps<Self> + CommandOps<Self> + Clone;
	type Buffer: Copy + Eq + Debug + Send + Default;
	type Image: Copy + Eq + Debug + Send;
	type ImageView: Copy + Eq + Debug + Send;
	type CommandBuffer: Copy;
	type CommandPool: Copy;
	type Pipeline: Copy;
	type PipelineLayout: Copy;
	type ShaderModule: Copy;
	type Semaphore: Copy + Eq;
	type Fence: Copy;
	type Queue: Copy;
	type DeviceMemory: Copy + Debug + Send + Eq;
	type DescriptorSet: Copy + Eq + Debug + Send;
	type DescriptorSetLayout: Copy;
	type DescriptorPool: Copy;
	type Sampler: Copy;
	type Error: Error;
	type Format: Copy + PartialEq + From<crate::core::types::Format>;
	
	type Allocation: Allocation;
	fn image_from_raw(raw: u64) -> Self::Image;
	fn buffer_from_raw(raw: u64) -> Self::Buffer;
	fn buffer_handle(buf: Self::Buffer) -> u64;
	fn descriptor_set_from_raw(raw: u64) -> Self::DescriptorSet;
	fn null_semaphore() -> Self::Semaphore;
	fn null_fence() -> Self::Fence;
	fn null_pipeline() -> Self::Pipeline;
	
	fn null_memory() -> Self::DeviceMemory;
}

// ── Operation Traits (methods added incrementally) ──────────

pub trait DeviceOps<B: Backend>: Sized {
	// --- SEMAPHORES ---
	fn create_binary_semaphore(&self) -> Result<B::Semaphore, B::Error>;
	fn create_timeline_semaphore(&self, initial: u64) -> Result<B::Semaphore, B::Error>;
	fn wait_semaphore(&self, sem: B::Semaphore, value: u64) -> Result<(), B::Error>;
	fn signal_semaphore(&self, sem: B::Semaphore, value: u64) -> Result<(), B::Error>;
	fn query_semaphore(&self, sem: B::Semaphore) -> Result<u64, B::Error>;
	fn destroy_semaphore(&self, sem: B::Semaphore);
	
	// --- COMMANDS ---
	fn create_command_pool(&self, family: u32, flags: CommandPoolFlags) -> Result<B::CommandPool, B::Error>;
	fn destroy_command_pool(&self, pool: B::CommandPool);
	fn allocate_command_buffer(&self, pool: B::CommandPool) -> Result<B::CommandBuffer, B::Error>;
	// --- IMAGES & SAMPLERS ---
	fn create_image_view_2d(&self, image: B::Image, format: Format, aspect: ImageAspect) -> Result<B::ImageView, B::Error>;
	fn destroy_image_view(&self, view: B::ImageView);
	fn create_sampler(&self, filter: Filter, address: SamplerAddressMode) -> Result<B::Sampler, B::Error>;
	fn destroy_sampler(&self, sampler: B::Sampler);
	
	// --- DESCRIPTORS ---
	fn create_descriptor_pool(&self, max_sets: u32, sizes: &[DescriptorPoolSize]) -> Result<B::DescriptorPool, B::Error>;
	fn destroy_descriptor_pool(&self, pool: B::DescriptorPool);
	
	fn allocate_descriptor_set(&self, pool: B::DescriptorPool, layout: B::DescriptorSetLayout) -> Result<B::DescriptorSet,
		B::Error>;
	fn write_descriptor_buffer(&self, set: B::DescriptorSet, binding: u32, ty: DescriptorType, buffer: B::Buffer, offset: u64, range: u64);
	fn write_descriptor_image(&self, set: B::DescriptorSet, binding: u32, ty: DescriptorType, sampler: B::Sampler, view: B::ImageView, layout: ImageLayout);
	
	// --- MEMORY & BUFFERS ---
	fn create_buffer(&self, size: u64, usage: BufferUsage) -> Result<B::Buffer, B::Error>;
	fn get_buffer_memory_requirements(&self, buffer: B::Buffer) -> MemoryRequirements;
	fn allocate_memory(&self, size: u64, memory_type_index: u32) -> Result<B::DeviceMemory, B::Error>;
	fn bind_buffer_memory(&self, buffer: B::Buffer, memory: B::DeviceMemory, offset: u64) -> Result<(), B::Error>;
	fn destroy_buffer(&self, buffer: B::Buffer);
	fn free_memory(&self, memory: B::DeviceMemory);
	fn map_memory(&self, memory: B::DeviceMemory, offset: u64, size: u64) -> Result<*mut u8, B::Error>;
	fn unmap_memory(&self, memory: B::DeviceMemory);
	fn null_memory() -> B::DeviceMemory;
	
	fn create_image_2d(&self, format: Format, width: u32, height: u32, usage: ImageUsage) -> Result<B::Image, B::Error>;
	fn get_image_memory_requirements(&self, image: B::Image) -> MemoryRequirements;
	fn bind_image_memory(&self, image: B::Image, memory: B::DeviceMemory, offset: u64) -> Result<(), B::Error>;
	fn destroy_image(&self, image: B::Image);
	
	// --- PIPELINES & SHADERS ---
	fn create_shader_module(&self, spv: &[u8]) -> Result<B::ShaderModule, B::Error>;
	fn destroy_shader_module(&self, module: B::ShaderModule);
	fn create_pipeline_layout(&self, desc_layouts: &[B::DescriptorSetLayout], push_ranges: &[PushConstantRange]) -> Result<B::PipelineLayout, B::Error>;
	fn destroy_pipeline_layout(&self, layout: B::PipelineLayout);
	fn create_graphics_pipeline(&self, desc: &GraphicsPipelineDesc<'_, B>) -> Result<B::Pipeline, B::Error>;
	fn create_compute_pipeline(&self, shader: B::ShaderModule, layout: B::PipelineLayout) -> Result<B::Pipeline,
		B::Error>;
	fn destroy_pipeline(&self, pipeline: B::Pipeline);
	
	// --- EXECUTION ---
	fn queue_submit2(
		&self,
		queue: B::Queue,
		cmd: Option<B::CommandBuffer>,
		waits: &[SemaphoreSubmit<B>],
		signals: &[SemaphoreSubmit<B>],
	) -> Result<(), B::Error>;
	fn create_descriptor_set_layout(
		&self,
		bindings: &[DescriptorBinding],
	) -> Result<B::DescriptorSetLayout, B::Error>;
	
	fn destroy_descriptor_set_layout(
		&self,
		layout: B::DescriptorSetLayout,
	);
}

pub trait CommandOps<B: Backend> {
	fn begin_command_buffer(&self, cmd: B::CommandBuffer, usage: CommandBufferUsageFlags) -> Result<(), B::Error>;
	fn end_command_buffer(&self, cmd: B::CommandBuffer) -> Result<(), B::Error>;
	fn cmd_set_viewport(&self, cmd: B::CommandBuffer, viewport: Viewport);
	fn cmd_set_scissor(&self, cmd: B::CommandBuffer, scissor: Rect2D);
	fn cmd_buffer_barrier(&self, cmd: B::CommandBuffer, barriers: &[BufferBarrierInfo2<B>]);
	
	// Transfer
	fn cmd_copy_buffer(&self, cmd: B::CommandBuffer, src: B::Buffer, dst: B::Buffer, src_offset: u64, dst_offset: u64,
					   size: u64);
	fn cmd_copy_buffer_to_image(
		&self,
		cmd: B::CommandBuffer,
		src: B::Buffer,
		src_offset: u64,
		dst: B::Image,
		extent: Extent3D
	);
	
	// Binding
	fn cmd_bind_vertex_buffers(&self, cmd: B::CommandBuffer, first: u32, buffers: &[B::Buffer], offsets: &[u64]);
	fn cmd_bind_index_buffer(&self, cmd: B::CommandBuffer, buffer: B::Buffer, offset: u64, index_type: IndexType);
	fn cmd_bind_descriptor_sets(&self, cmd: B::CommandBuffer, bind_point: PipelineBindPoint, layout: B::PipelineLayout, first_set: u32, sets: &[B::DescriptorSet], dynamic_offsets: &[u32]);
	fn cmd_push_constants(&self, cmd: B::CommandBuffer, layout: B::PipelineLayout, stages: ShaderStages, offset: u32, data: &[u8]);
	fn cmd_bind_pipeline(&self, cmd: B::CommandBuffer, bind_point: PipelineBindPoint, pipeline: B::Pipeline);
	fn cmd_draw(&self, cmd: B::CommandBuffer, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32);
	fn cmd_draw_indexed(&self, cmd: B::CommandBuffer, index_count: u32, instance_count: u32, first_index: u32,
						vertex_offset: i32, first_instance: u32);
	fn cmd_dispatch(&self, cmd: B::CommandBuffer, x: u32, y: u32, z: u32);
	fn cmd_begin_rendering(&self, cmd: B::CommandBuffer, desc: &RenderingDesc<B>);
	fn cmd_end_rendering(&self, cmd: B::CommandBuffer);
	fn cmd_image_barrier(&self, cmd: B::CommandBuffer, barriers: &[ImageBarrierInfo<B>]);
}


// ── B-dependent structs ─────────────────────────────────────
#[derive(Debug)]
pub struct ImageBarrierInfo<B: Backend> {
	pub image: B::Image,
	pub old_layout: ImageLayout,
	pub new_layout: ImageLayout,
	pub src_stage: Stage,
	pub src_access: Access,
	pub dst_stage: Stage,
	pub dst_access: Access,
	pub aspect: ImageAspect,
	pub src_queue_family: u32,
	pub dst_queue_family: u32,
}


#[derive(Debug)]
pub struct BufferBarrierInfo2<B: Backend> {
	pub buffer: B::Buffer,
	pub src_stage: PipelineStageFlags2,
	pub src_access: AccessFlags2,
	pub dst_stage: PipelineStageFlags2,
	pub dst_access: AccessFlags2,
	pub src_queue_family: u32,
	pub dst_queue_family: u32,
}

pub struct SemaphoreSubmit<B: Backend> {
	pub semaphore: B::Semaphore,
	pub value: u64,
	pub stage: Stage,
}
impl<B: Backend> Copy for SemaphoreSubmit<B> {}
impl<B: Backend> Clone for SemaphoreSubmit<B> {
	fn clone(&self) -> Self { *self }
}

pub struct ColorAttachment<B: Backend> {
	pub view: B::ImageView,
	pub layout: ImageLayout,
	pub load_op: AttachmentLoadOp,
	pub store_op: AttachmentStoreOp,
	pub clear_value: ClearValue,
}

pub struct RenderingDesc<B: Backend> {
	pub area: Rect2D,
	pub color_attachments: Vec<ColorAttachment<B>>,
	pub depth_attachment: Option<DepthAttachment<B>>,
}

pub struct DepthAttachment<B: Backend> {
	pub view: B::ImageView,
	pub layout: ImageLayout,
	pub load_op: AttachmentLoadOp,
	pub store_op: AttachmentStoreOp,
	pub clear_depth: f32,
}

/// An owned GPU memory ticket. Dropped when the resource is destroyed,
/// triggering return to the arena. Must have `finalize_lifetime` called
/// before drop to record when the GPU is done with it.
pub trait Allocation: Send {
	type Memory: Copy + Debug + Send + Eq;
	type Buffer: Copy + Debug + Eq;
	fn memory(&self) -> Self::Memory;
	fn buffer(&self) -> Self::Buffer;
	fn finalize_lifetime(&mut self, t: u64);
	// MEMORY SPACE
	fn memory_offset(&self) -> u64;
	// LOGICAL SIZE
	fn size(&self)   -> u64;
	// INTERNAL (allocator only)
	fn block_idx(&self) -> u32;
	

}


//offset() + size() <= underlying VkDeviceMemory allocation