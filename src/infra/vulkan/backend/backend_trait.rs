use crate::core::types::MemoryPropertyFlags;
use crate::{
	domain::ImageLayout,
	core::types::{
		AccessFlags2,
		Viewport,
		DescriptorBinding,
		BufferUsage,
		ClearValue,
		CommandBufferUsageFlags,
		CommandPoolFlags,
		CullMode,
		DescriptorPoolSize,
		DescriptorType,
		Extent3D,
		Filter,
		Format,
		FrontFace,
		GraphicsPipelineDesc,
		ImageAspect,
		Rect2D,
		ImageUsage,
		IndexType,
		MemoryRequirements,
		PipelineBindPoint,
		PipelineStageFlags2,
		PushConstantRange,
	
		SamplerAddressMode,
		ShaderStages,
	},
	core::{Backend, CommandOps, DeviceOps, ImageBarrierInfo,SemaphoreSubmit, BufferBarrierInfo2,	RenderingDesc,},
};

use ash::vk;
use ash::vk::Handle;

pub struct VulkanBackend;

#[derive(Clone)]
pub struct VulkanDevice {
	pub(crate) inner: ash::Device
}

impl Drop for VulkanDevice {
	fn drop(&mut self) {
		unsafe {
			let _ = self.inner.device_wait_idle();
			self.inner.destroy_device(None);
		}
	}
}
impl VulkanDevice {
	/// Blocks the CPU until all submitted GPU work across all queues has completed.
	///
	/// # Safety
	/// While this is a "blocking" call, it is safe in the sense that it handles
	/// Vulkan errors gracefully. However, ensure no other threads are
	/// destroying the device while this is called.
	pub fn wait_idle(&self) -> Result<(), ash::vk::Result> {
		unsafe {
			// We map the Vulkan Result to a Rust Result
			self.inner.device_wait_idle()
		}
	}
}

// impl std::ops::Deref for VulkanDevice {
// 	type Target = ash::Device;
// 	fn deref(&self) -> &Self::Target {
// 		&self.inner
// 	}
// }

impl Backend for VulkanBackend {
	type Device             = VulkanDevice;
	type Buffer             = vk::Buffer;
	type Image              = vk::Image;
	type ImageView          = vk::ImageView;
	type CommandBuffer      = vk::CommandBuffer;
	type CommandPool        = vk::CommandPool;
	type Pipeline           = vk::Pipeline;
	type PipelineLayout     = vk::PipelineLayout;
	type ShaderModule       = vk::ShaderModule;
	type Semaphore          = vk::Semaphore;
	type Fence              = vk::Fence;
	type Queue              = vk::Queue;
	type DeviceMemory       = vk::DeviceMemory;
	type DescriptorSet      = vk::DescriptorSet;
	type DescriptorSetLayout = vk::DescriptorSetLayout;
	type DescriptorPool     = vk::DescriptorPool;
	type Sampler            = vk::Sampler;
	type Error              = vk::Result;
	type Format 			= Format;
	fn image_from_raw(raw: u64) -> vk::Image { vk::Image::from_raw(raw) }
	fn buffer_from_raw(raw: u64) -> vk::Buffer { vk::Buffer::from_raw(raw) }
	fn descriptor_set_from_raw(raw: u64) -> vk::DescriptorSet { vk::DescriptorSet::from_raw(raw) }
	
	fn null_semaphore() -> vk::Semaphore { vk::Semaphore::null() }
	fn null_fence() -> vk::Fence { vk::Fence::null() }
	fn null_pipeline() -> vk::Pipeline { vk::Pipeline::null() }
	fn null_memory() -> vk::DeviceMemory { vk::DeviceMemory::null() }
}

impl DeviceOps<VulkanBackend> for VulkanDevice {
	fn create_binary_semaphore(&self) -> Result<vk::Semaphore, vk::Result> {
		let info = vk::SemaphoreCreateInfo::default();
		unsafe { self.inner.create_semaphore(&info, None) }
	}
	
	
	fn destroy_semaphore(&self, sem: vk::Semaphore) {
		unsafe { self.inner.destroy_semaphore(sem, None); }
	}
	
	fn create_timeline_semaphore(&self, initial: u64) -> Result<vk::Semaphore, vk::Result> {
		let mut type_info = vk::SemaphoreTypeCreateInfo::default()
			.semaphore_type(vk::SemaphoreType::TIMELINE)
			.initial_value(initial);
		let info = vk::SemaphoreCreateInfo::default().push_next(&mut type_info);
		unsafe { self.inner.create_semaphore(&info, None) }
	}
	fn wait_semaphore(&self, sem: vk::Semaphore, value: u64) -> Result<(), vk::Result> {
		let sems = [sem];
		let values = [value];
		let wait_info = vk::SemaphoreWaitInfo::default()
			.semaphores(&sems)
			.values(&values);
		unsafe { self.inner.wait_semaphores(&wait_info, u64::MAX) }
	}
	
	fn signal_semaphore(&self, sem: vk::Semaphore, value: u64) -> Result<(), vk::Result> {
		let signal_info = vk::SemaphoreSignalInfo::default()
			.semaphore(sem)
			.value(value);
		unsafe { self.inner.signal_semaphore(&signal_info) }
	}
	
	fn query_semaphore(&self, sem: vk::Semaphore) -> Result<u64, vk::Result> {
		unsafe { self.inner.get_semaphore_counter_value(sem) }
	}
	fn create_command_pool(&self, family: u32, flags: CommandPoolFlags) -> Result<vk::CommandPool, vk::Result> {
		let info = vk::CommandPoolCreateInfo::default()
			.queue_family_index(family)
			.flags(vk::CommandPoolCreateFlags::from_raw(flags.0 as u32));
		unsafe { self.inner.create_command_pool(&info, None) }
	}
	
	fn destroy_command_pool(&self, pool: vk::CommandPool) {
		unsafe { self.inner.destroy_command_pool(pool, None); }
	}
	
	fn allocate_command_buffer(&self, pool: vk::CommandPool) -> Result<vk::CommandBuffer, vk::Result> {
		let info = vk::CommandBufferAllocateInfo::default()
			.command_pool(pool)
			.level(vk::CommandBufferLevel::PRIMARY)
			.command_buffer_count(1);
		unsafe { Ok(self.inner.allocate_command_buffers(&info)?[0]) }
	}
	fn create_image_view_2d(&self, image: vk::Image, format: Format, aspect: ImageAspect) -> Result<vk::ImageView,
		vk::Result> {
		let vk_aspect = match aspect {
			ImageAspect::Color => vk::ImageAspectFlags::COLOR,
			ImageAspect::Depth => vk::ImageAspectFlags::DEPTH,
			ImageAspect::Stencil => vk::ImageAspectFlags::STENCIL,
			ImageAspect::DepthStencil => vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
		};
		let info = vk::ImageViewCreateInfo::default()
			.image(image)
			.view_type(vk::ImageViewType::TYPE_2D)
			.format(format.to_vk())
			.subresource_range(
				vk::ImageSubresourceRange::default()
					.aspect_mask(vk_aspect)
					.level_count(1)
					.layer_count(1),
			);
		unsafe { self.inner.create_image_view(&info, None) }
	}
	
	fn destroy_image_view(&self, view: vk::ImageView) {
		unsafe { self.inner.destroy_image_view(view, None); }
	}
	fn create_sampler(&self, filter: Filter, address: SamplerAddressMode) -> Result<vk::Sampler, vk::Result> {
		let info = vk::SamplerCreateInfo::default()
			.mag_filter(vk::Filter::from_raw(filter.as_raw()))
			.min_filter(vk::Filter::from_raw(filter.as_raw()))
			.address_mode_u(vk::SamplerAddressMode::from_raw(address.as_raw()))
			.address_mode_v(vk::SamplerAddressMode::from_raw(address.as_raw()))
			.address_mode_w(vk::SamplerAddressMode::from_raw(address.as_raw()))
			.mipmap_mode(vk::SamplerMipmapMode::LINEAR)
			.max_lod(vk::LOD_CLAMP_NONE);
		unsafe { self.inner.create_sampler(&info, None) }
	}
	
	
	fn destroy_sampler(&self, sampler: vk::Sampler) {
		unsafe { self.inner.destroy_sampler(sampler, None); }
	}
	
	fn create_descriptor_pool(&self, max_sets: u32, sizes: &[DescriptorPoolSize]) -> Result<vk::DescriptorPool, vk::Result>
	{
		let vk_sizes: Vec<vk::DescriptorPoolSize> = sizes.iter().map(|s| {
			vk::DescriptorPoolSize {
				ty: vk::DescriptorType::from(s.descriptor_type),
				descriptor_count: s.count,
			}
		}).collect();
		let info = vk::DescriptorPoolCreateInfo::default()
			.max_sets(max_sets)
			.pool_sizes(&vk_sizes);
		unsafe { self.inner.create_descriptor_pool(&info, None) }
	}
	
	fn destroy_descriptor_pool(&self, pool: vk::DescriptorPool) {
		unsafe { self.inner.destroy_descriptor_pool(pool, None); }
	}
	fn allocate_descriptor_set(&self, pool: vk::DescriptorPool, layout: vk::DescriptorSetLayout) ->
	Result<vk::DescriptorSet, vk::Result> {
		let info = vk::DescriptorSetAllocateInfo::default()
			.descriptor_pool(pool)
			.set_layouts(std::slice::from_ref(&layout));
		unsafe { Ok(self.inner.allocate_descriptor_sets(&info)?[0]) }
	}
	
	fn write_descriptor_buffer(&self, set: vk::DescriptorSet, binding: u32, ty: DescriptorType, buffer: vk::Buffer, offset:
	u64, range: u64) {
		let buf_info = vk::DescriptorBufferInfo::default()
			.buffer(buffer)
			.offset(offset)
			.range(range);
		let write = vk::WriteDescriptorSet::default()
			.dst_set(set)
			.dst_binding(binding)
			.descriptor_type(vk::DescriptorType::from(ty))
			.buffer_info(std::slice::from_ref(&buf_info));
		unsafe { self.inner.update_descriptor_sets(&[write], &[]); }
	}
	
	fn write_descriptor_image(&self, set: vk::DescriptorSet, binding: u32, ty: DescriptorType, sampler: vk::Sampler, view:
	vk::ImageView, layout: ImageLayout) {
		let vk_layout = match layout {
			ImageLayout::ShaderReadOnly => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
			ImageLayout::General => vk::ImageLayout::GENERAL,
			ImageLayout::TransferSrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
			ImageLayout::TransferDst => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
			ImageLayout::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
			ImageLayout::DepthAttachment => vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
			ImageLayout::Present => vk::ImageLayout::PRESENT_SRC_KHR,
			ImageLayout::Undefined => vk::ImageLayout::UNDEFINED,
			_ => vk::ImageLayout::GENERAL,
		};
		let img_info = vk::DescriptorImageInfo::default()
			.sampler(sampler)
			.image_view(view)
			.image_layout(vk_layout);
		let write = vk::WriteDescriptorSet::default()
			.dst_set(set)
			.dst_binding(binding)
			.descriptor_type(vk::DescriptorType::from(ty))
			.image_info(std::slice::from_ref(&img_info));
		unsafe { self.inner.update_descriptor_sets(&[write], &[]); }
	}
	
	fn create_buffer(&self, size: u64, usage: BufferUsage) -> Result<vk::Buffer, vk::Result> {
		let info = vk::BufferCreateInfo::default()
			.size(size)
			.usage(vk::BufferUsageFlags::from_raw(usage.0 as u32))
			.sharing_mode(vk::SharingMode::EXCLUSIVE);
		unsafe { self.inner.create_buffer(&info, None) }
	}
	
	fn get_buffer_memory_requirements(&self, buffer: vk::Buffer) -> MemoryRequirements {
		let req = unsafe { self.inner.get_buffer_memory_requirements(buffer) };
		MemoryRequirements {
			size: req.size,
			alignment: req.alignment,
			memory_type_bits: req.memory_type_bits,
		}
	}
	
	fn allocate_memory(&self, size: u64, memory_type_index: u32) -> Result<vk::DeviceMemory, vk::Result> {
		let info = vk::MemoryAllocateInfo::default()
			.allocation_size(size)
			.memory_type_index(memory_type_index);
		unsafe { self.inner.allocate_memory(&info, None) }
	}
	
	fn bind_buffer_memory(&self, buffer: vk::Buffer, memory: vk::DeviceMemory, offset: u64) -> Result<(), vk::Result> {
		unsafe { self.inner.bind_buffer_memory(buffer, memory, offset) }
	}
	
	fn destroy_buffer(&self, buffer: vk::Buffer) {
		unsafe { self.inner.destroy_buffer(buffer, None); }
	}
	
	fn free_memory(&self, memory: vk::DeviceMemory) {
		unsafe { self.inner.free_memory(memory, None); }
	}
	
	fn map_memory(&self, memory: vk::DeviceMemory, offset: u64, size: u64) -> Result<*mut u8, vk::Result> {
		unsafe {
			self.inner.map_memory(memory, offset, size, vk::MemoryMapFlags::empty())
				.map(|p| p as *mut u8)
		}
	}
	
	fn unmap_memory(&self, memory: vk::DeviceMemory) {
		unsafe { self.inner.unmap_memory(memory); }
	}
	
	fn null_memory() -> vk::DeviceMemory {
		vk::DeviceMemory::null()
	}
	fn create_image_2d(&self, format: Format, width: u32, height: u32, usage: ImageUsage) -> Result<vk::Image, vk::Result>
	{
		let info = vk::ImageCreateInfo::default()
			.image_type(vk::ImageType::TYPE_2D)
			.format(vk::Format::from_raw(format.0))
			.extent(vk::Extent3D { width, height, depth: 1 })
			.mip_levels(1).array_layers(1)
			.samples(vk::SampleCountFlags::TYPE_1)
			.tiling(vk::ImageTiling::OPTIMAL)
			.usage(vk::ImageUsageFlags::from_raw(usage.0 as u32))
			.sharing_mode(vk::SharingMode::EXCLUSIVE)
			.initial_layout(vk::ImageLayout::UNDEFINED);
		unsafe { self.inner.create_image(&info, None) }
	}
	
	
	fn get_image_memory_requirements(&self, image: vk::Image) -> MemoryRequirements {
		let req = unsafe { self.inner.get_image_memory_requirements(image) };
		MemoryRequirements { size: req.size, alignment: req.alignment, memory_type_bits: req.memory_type_bits }
	}
	
	fn bind_image_memory(&self, image: vk::Image, memory: vk::DeviceMemory, offset: u64) -> Result<(), vk::Result> {
		unsafe { self.inner.bind_image_memory(image, memory, offset) }
	}
	
	fn destroy_image(&self, image: vk::Image) {
		unsafe { self.inner.destroy_image(image, None); }
	}
	
	fn create_shader_module(&self, spv: &[u8]) -> Result<vk::ShaderModule, vk::Result> {
		assert!(spv.len() % 4 == 0, "SPIR-V must be 4-byte aligned in size");
		let code: Vec<u32> = spv
			.chunks_exact(4)
			.map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
			.collect();
		let info = vk::ShaderModuleCreateInfo::default().code(&code);
		unsafe { self.inner.create_shader_module(&info, None) }
	}
	
	
	fn destroy_shader_module(&self, module: vk::ShaderModule) {
		unsafe { self.inner.destroy_shader_module(module, None) }
	}
	
	fn create_pipeline_layout(
		&self,
		set_layouts: &[vk::DescriptorSetLayout],
		push_ranges: &[PushConstantRange],
	) -> Result<vk::PipelineLayout, vk::Result> {
		let vk_ranges: Vec<vk::PushConstantRange> = push_ranges.iter().map(|pc| {
			vk::PushConstantRange {
				stage_flags: vk::ShaderStageFlags::from_raw(pc.stages.0.try_into().unwrap()),
				offset: pc.offset,
				size: pc.size,
			}
		}).collect();
		let info = vk::PipelineLayoutCreateInfo::default()
			.set_layouts(set_layouts)
			.push_constant_ranges(&vk_ranges);
		unsafe { self.inner.create_pipeline_layout(&info, None) }
	}
	
	
	fn destroy_pipeline_layout(&self, layout: vk::PipelineLayout) {
		unsafe { self.inner.destroy_pipeline_layout(layout, None) }
	}
	
	fn create_graphics_pipeline(
		&self,
		desc: &GraphicsPipelineDesc<'_, VulkanBackend>,
	) -> Result<vk::Pipeline, vk::Result> {
		let entry = c"main";
		
		let stages = [
			vk::PipelineShaderStageCreateInfo::default()
				.stage(vk::ShaderStageFlags::VERTEX)
				.module(desc.shaders.vert)
				.name(entry),
			vk::PipelineShaderStageCreateInfo::default()
				.stage(vk::ShaderStageFlags::FRAGMENT)
				.module(desc.shaders.frag)
				.name(entry),
		];
		
		let vk_bindings: Vec<vk::VertexInputBindingDescription>
			=desc.vertex.bindings
				 .iter()
				 .map(|vb| vk::VertexInputBindingDescription {
					 binding:    vb.binding,
					 stride:     vb.stride,
					 input_rate: vk::VertexInputRate::from_raw(vb.input_rate.as_raw()),
				 })
				 .collect();
		
		let vk_attrs: Vec<vk::VertexInputAttributeDescription>
			= desc.vertex.attributes
				  .iter()
				  .map(|va| vk::VertexInputAttributeDescription {
					  location: va.location,
					  binding:  va.binding,
					  format:   va.format.to_vk(),
					  offset:   va.offset,
				  })
				  .collect();
		
		let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
			.vertex_binding_descriptions(&vk_bindings)
			.vertex_attribute_descriptions(&vk_attrs);
		
		let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
			.topology(vk::PrimitiveTopology::from_raw(desc.vertex.topology.as_raw()))
			.primitive_restart_enable(false);
		
		let raster = vk::PipelineRasterizationStateCreateInfo::default()
			.polygon_mode(vk::PolygonMode::from_raw(desc.raster.polygon_mode.as_raw()))
			.cull_mode(cull_mode_to_vk(desc.raster.cull))
			.front_face(front_face_to_vk(desc.raster.front_face))
			.line_width(1.0);
		
		let multisample = vk::PipelineMultisampleStateCreateInfo::default()
			.rasterization_samples(vk::SampleCountFlags::TYPE_1);
		
		let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
			.depth_test_enable(desc.depth.test)
			.depth_write_enable(desc.depth.write)
			.depth_compare_op(vk::CompareOp::from_raw(desc.depth.compare.as_raw()));
		
		let attachment = if  desc.blend.enable {
			vk::PipelineColorBlendAttachmentState::default()
				.blend_enable(true)
				.src_color_blend_factor(vk::BlendFactor::from_raw( desc.blend.src_color.as_raw()))
				.dst_color_blend_factor(vk::BlendFactor::from_raw(  desc.blend.dst_color.as_raw()))
				.color_blend_op(vk::BlendOp::from_raw(desc.blend.color_op.as_raw()))
				.src_alpha_blend_factor(vk::BlendFactor::from_raw( desc.blend.src_alpha.as_raw()))
				.dst_alpha_blend_factor(vk::BlendFactor::from_raw(desc.blend.dst_alpha.as_raw()))
				.alpha_blend_op(vk::BlendOp::from_raw(desc.blend.alpha_op.as_raw()))
				.color_write_mask(vk::ColorComponentFlags::RGBA)
		} else {
			vk::PipelineColorBlendAttachmentState::default()
				.blend_enable(false)
				.color_write_mask(vk::ColorComponentFlags::RGBA)
		};
		
		let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
			.attachments(std::slice::from_ref(&attachment));
		
		let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
		let dynamic = vk::PipelineDynamicStateCreateInfo::default()
			.dynamic_states(&dynamic_states);
		
		let viewport_state = vk::PipelineViewportStateCreateInfo::default()
			.viewport_count(1)
			.scissor_count(1);
		
		let color_formats = [desc.target.color_format.to_vk()];
		let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
			.color_attachment_formats(&color_formats);
		
		let create_info = vk::GraphicsPipelineCreateInfo::default()
			.stages(&stages)
			.vertex_input_state(&vertex_input)
			.input_assembly_state(&input_assembly)
			.viewport_state(&viewport_state)
			.rasterization_state(&raster)
			.multisample_state(&multisample)
			.depth_stencil_state(&depth_stencil)
			.color_blend_state(&color_blend)
			.dynamic_state(&dynamic)
			.layout(desc.layout)
			.push_next(&mut rendering_info);
		
		let pipelines = unsafe {
			self.inner.create_graphics_pipelines(
				vk::PipelineCache::null(), &[create_info], None,
			).map_err(|e| e.1)?
		};
		Ok(pipelines[0])
	}
	
	
	
	fn create_compute_pipeline(
		&self,
		module: vk::ShaderModule,
		layout: vk::PipelineLayout,
	) -> Result<vk::Pipeline, vk::Result> {
		let entry = c"main";
		let stage = vk::PipelineShaderStageCreateInfo::default()
			.stage(vk::ShaderStageFlags::COMPUTE)
			.module(module)
			.name(entry);
		
		let create_info = vk::ComputePipelineCreateInfo::default()
			.stage(stage)
			.layout(layout);
		
		let pipelines = unsafe {
			self.inner.create_compute_pipelines(
				vk::PipelineCache::null(),
				&[create_info],
				None,
			).map_err(|e| e.1)?
		};
		
		Ok(pipelines[0])
	}
	
	fn destroy_pipeline(&self, pipeline: vk::Pipeline) {
		unsafe { self.inner.destroy_pipeline(pipeline, None) }
	}
	
	fn queue_submit2(
		&self,
		queue: vk::Queue,
		cmd: Option<vk::CommandBuffer>,
		waits: &[SemaphoreSubmit<VulkanBackend>],
		signals: &[SemaphoreSubmit<VulkanBackend>],
	) -> Result<(), vk::Result> {
		let wait_infos: Vec<vk::SemaphoreSubmitInfo> = waits.iter()
															.map(|w| vk::SemaphoreSubmitInfo::default()
																.semaphore(w.semaphore)
																.value(w.value)
																.stage_mask(stage_to_vk(w.stage)))
															.collect();
		
		let signal_infos: Vec<vk::SemaphoreSubmitInfo> = signals.iter()
																.map(|s| vk::SemaphoreSubmitInfo::default()
																	.semaphore(s.semaphore)
																	.value(s.value)
																	.stage_mask(stage_to_vk(s.stage)))
																.collect();
		
		let cmd_info = cmd.map(|c| vk::CommandBufferSubmitInfo::default().command_buffer(c));
		
		let mut submit = vk::SubmitInfo2::default()
			.wait_semaphore_infos(&wait_infos)
			.signal_semaphore_infos(&signal_infos);
		
		if let Some(ref info) = cmd_info {
			submit = submit.command_buffer_infos(std::slice::from_ref(info));
		}
		
		unsafe { self.inner.queue_submit2(queue, &[submit], vk::Fence::null()) }
	}
	fn create_descriptor_set_layout(
		&self,
		bindings: &[DescriptorBinding],
	) -> Result<vk::DescriptorSetLayout, vk::Result> {
		let vk_bindings: Vec<vk::DescriptorSetLayoutBinding> =
			bindings.iter().map(|b| {
				vk::DescriptorSetLayoutBinding::default()
					.binding(b.binding)
					.descriptor_type(b.descriptor_type.into())
					.descriptor_count(b.count)
					.stage_flags(b.stages.into())
			}).collect();
		
		let info = vk::DescriptorSetLayoutCreateInfo::default()
			.bindings(&vk_bindings);
		
		unsafe { self.inner.create_descriptor_set_layout(&info, None) }
	}
	fn destroy_descriptor_set_layout(
		&self,
		layout: vk::DescriptorSetLayout,
	) {
		unsafe {
			self.inner.destroy_descriptor_set_layout(layout, None);
		}
	}
}


impl Format {
	pub fn to_vk(self) -> vk::Format {
		vk::Format::from_raw(self.0)
	}
}


impl CommandOps<VulkanBackend> for VulkanDevice {
	fn begin_command_buffer(&self, cmd: vk::CommandBuffer, usage: CommandBufferUsageFlags) -> Result<(), vk::Result> {
		let info = vk::CommandBufferBeginInfo::default()
			.flags(vk::CommandBufferUsageFlags::from_raw(usage.0 as u32));
		unsafe { self.inner.begin_command_buffer(cmd, &info) }
	}
	
	
	fn end_command_buffer(&self, cmd: vk::CommandBuffer) -> Result<(), vk::Result> {
		unsafe { self.inner.end_command_buffer(cmd) }
	}
	
	fn cmd_set_viewport(&self, cmd: vk::CommandBuffer, viewport: Viewport) {
		let vk_vp = vk::Viewport {
			x: viewport.x,
			y: viewport.y,
			width: viewport.width,
			height: viewport.height,
			min_depth: viewport.min_depth,
			max_depth: viewport.max_depth,
		};
		unsafe {
			self.inner.cmd_set_viewport(cmd, 0, std::slice::from_ref(&vk_vp));
		}
	}
	
	fn cmd_set_scissor(&self, cmd: vk::CommandBuffer, scissor: Rect2D) {
		let vk_rect = vk::Rect2D {
			offset: vk::Offset2D {
				x: scissor.offset().x(),
				y: scissor.offset().y(),
			},
			extent: vk::Extent2D {
				width: scissor.extent().width(),
				height: scissor.extent().height(),
			},
		};
		unsafe {
			self.inner.cmd_set_scissor(cmd, 0, std::slice::from_ref(&vk_rect));
		}
	}
	
	fn cmd_buffer_barrier(&self, cmd: vk::CommandBuffer, barriers: &[BufferBarrierInfo2<VulkanBackend>]) {
		let vk_barriers: Vec<vk::BufferMemoryBarrier2> = barriers.iter().map(|b| {
			vk::BufferMemoryBarrier2::default()
				.src_stage_mask(vk::PipelineStageFlags2::from_raw(b.src_stage.0))
				.src_access_mask(vk::AccessFlags2::from_raw(b.src_access.0))
				.dst_stage_mask(vk::PipelineStageFlags2::from_raw(b.dst_stage.0))
				.dst_access_mask(vk::AccessFlags2::from_raw(b.dst_access.0))
				.src_queue_family_index(b.src_queue_family)
				.dst_queue_family_index(b.dst_queue_family)
				.buffer(b.buffer)
				.offset(0)
				.size(vk::WHOLE_SIZE)
		}).collect();
		let dep = vk::DependencyInfo::default()
			.buffer_memory_barriers(&vk_barriers);
		unsafe { self.inner.cmd_pipeline_barrier2(cmd, &dep); }
	}
	
	fn cmd_copy_buffer(&self, cmd: vk::CommandBuffer, src: vk::Buffer, dst: vk::Buffer, src_offset: u64, dst_offset: u64,
					   size: u64) {
		let region = vk::BufferCopy { src_offset, dst_offset, size };
		unsafe { self.inner.cmd_copy_buffer(cmd, src, dst, &[region]); }
	}
	
	fn cmd_copy_buffer_to_image(&self, cmd: vk::CommandBuffer, src: vk::Buffer, dst: vk::Image, extent: Extent3D) {
		let region = vk::BufferImageCopy::default()
			.image_subresource(
				vk::ImageSubresourceLayers::default()
					.aspect_mask(vk::ImageAspectFlags::COLOR)
					.layer_count(1),
			)
			.image_extent(vk::Extent3D { width: extent.width(), height: extent.height(), depth: extent.depth() });
		unsafe {
			self.inner.cmd_copy_buffer_to_image(cmd, src, dst, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]);
		}
	}
	
	fn cmd_bind_vertex_buffers(
		&self, cmd: vk::CommandBuffer, first: u32, buffers: &[vk::Buffer], offsets: &[u64],
	) {
		unsafe { self.inner.cmd_bind_vertex_buffers(cmd, first, buffers, offsets); }
	}
	
	
	
	fn cmd_bind_index_buffer(
		&self, cmd: vk::CommandBuffer, buffer: vk::Buffer, offset: u64, index_type: IndexType,
	) {
		unsafe {
			self.inner.cmd_bind_index_buffer(
				cmd, buffer, offset, vk::IndexType::from_raw(index_type.as_raw()),
			);
		}
	}
	
	fn cmd_bind_descriptor_sets(
		&self, cmd: vk::CommandBuffer, bind_point: PipelineBindPoint,
		layout: vk::PipelineLayout, first_set: u32,
		sets: &[vk::DescriptorSet], dynamic_offsets: &[u32],
	) {
		unsafe {
			self.inner.cmd_bind_descriptor_sets(
				cmd,
				vk::PipelineBindPoint::from_raw(bind_point.as_raw()),
				layout, first_set, sets, dynamic_offsets,
			);
		}
	}
	
	
	
	fn cmd_push_constants(&self, cmd: vk::CommandBuffer, layout: vk::PipelineLayout, stages: ShaderStages, offset: u32,
						  data: &[u8]) {
		unsafe {
			self.inner.cmd_push_constants(cmd, layout, vk::ShaderStageFlags::from_raw(stages.0 as u32), offset, data);
		}
	}
	fn cmd_bind_pipeline(
		&self, cmd: vk::CommandBuffer, bind_point: PipelineBindPoint, pipeline: vk::Pipeline,
	) {
		unsafe {
			self.inner.cmd_bind_pipeline(
				cmd, vk::PipelineBindPoint::from_raw(bind_point.as_raw()), pipeline,
			);
		}
	}
	
	
	fn cmd_draw(&self, cmd: vk::CommandBuffer, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance:
	u32) {
		unsafe { self.inner.cmd_draw(cmd, vertex_count, instance_count, first_vertex, first_instance); }
	}
	
	fn cmd_draw_indexed(&self, cmd: vk::CommandBuffer, index_count: u32, instance_count: u32, first_index: u32,
						vertex_offset: i32, first_instance: u32) {
		unsafe {
			self.inner.cmd_draw_indexed(cmd, index_count, instance_count, first_index, vertex_offset, first_instance);
		}
	}
	
	fn cmd_dispatch(&self, cmd: vk::CommandBuffer, x: u32, y: u32, z: u32) {
		unsafe { self.inner.cmd_dispatch(cmd, x, y, z); }
	}
	fn cmd_begin_rendering(&self, cmd: vk::CommandBuffer, desc: &RenderingDesc<VulkanBackend>) {
		let vk_color: Vec<vk::RenderingAttachmentInfo> = desc.color_attachments.iter().map(|a| {
			let clear = clear_value_to_vk(&a.clear_value);
			vk::RenderingAttachmentInfo::default()
				.image_view(a.view)
				.image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
				.load_op(vk::AttachmentLoadOp::from_raw(a.load_op.as_raw()))
				.store_op(vk::AttachmentStoreOp::from_raw(a.store_op.as_raw()))
				.clear_value(clear)
		}).collect();
		
		let vk_depth = desc.depth_attachment.as_ref().map(|d| {
			vk::RenderingAttachmentInfo::default()
				.image_view(d.view)
				.image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
				.load_op(vk::AttachmentLoadOp::from_raw(d.load_op.as_raw()))
				.store_op(vk::AttachmentStoreOp::from_raw(d.store_op.as_raw()))
				.clear_value(vk::ClearValue {
					depth_stencil: vk::ClearDepthStencilValue { depth: d.clear_depth, stencil: 0 },
				})
		});
		
		let area = vk::Rect2D {
			offset: vk::Offset2D { x: desc.area.offset().x(), y: desc.area.offset().y() },
			extent: vk::Extent2D { width: desc.area.extent().width(), height: desc.area.extent().height() },
		};
		
		let mut info = vk::RenderingInfo::default()
			.render_area(area)
			.layer_count(1)
			.color_attachments(&vk_color);
		if let Some(ref depth) = vk_depth {
			info = info.depth_attachment(depth);
		}
		unsafe { self.inner.cmd_begin_rendering(cmd, &info); }
	}
	
	fn cmd_image_barrier(&self, cmd: vk::CommandBuffer, barriers: &[ImageBarrierInfo<VulkanBackend>]) {
		let vk_barriers: Vec<vk::ImageMemoryBarrier2> = barriers.iter().map(|b| {
			let aspect = match b.aspect {
				ImageAspect::Color => vk::ImageAspectFlags::COLOR,
				ImageAspect::Depth => vk::ImageAspectFlags::DEPTH,
				ImageAspect::Stencil => vk::ImageAspectFlags::STENCIL,
				ImageAspect::DepthStencil => vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
			};
			let src_stage = PipelineStageFlags2::from(b.src_stage);
			let dst_stage = PipelineStageFlags2::from(b.dst_stage);
			let src_access = AccessFlags2::from(b.src_access);
			let dst_access = AccessFlags2::from(b.dst_access);
			vk::ImageMemoryBarrier2::default()
				.old_layout(layout_to_vk(b.old_layout))
				.new_layout(layout_to_vk(b.new_layout))
				.src_stage_mask(vk::PipelineStageFlags2::from_raw(src_stage.0))
				.src_access_mask(vk::AccessFlags2::from_raw(src_access.0))
				.dst_stage_mask(vk::PipelineStageFlags2::from_raw(dst_stage.0))
				.dst_access_mask(vk::AccessFlags2::from_raw(dst_access.0))
				.src_queue_family_index(b.src_queue_family)
				.dst_queue_family_index(b.dst_queue_family)
				.image(b.image)
				.subresource_range(vk::ImageSubresourceRange::default().aspect_mask(aspect).level_count(1).layer_count(1))
		}).collect();
		let dep = vk::DependencyInfo::default().image_memory_barriers(&vk_barriers);
		unsafe { self.inner.cmd_pipeline_barrier2(cmd, &dep); }
	}
	
	
	fn cmd_end_rendering(&self, cmd: vk::CommandBuffer) {
		unsafe { self.inner.cmd_end_rendering(cmd); }
	}
	
}

fn layout_to_vk(l: ImageLayout) -> vk::ImageLayout {
	match l {
		ImageLayout::Undefined => vk::ImageLayout::UNDEFINED,
		ImageLayout::General => vk::ImageLayout::GENERAL,
		ImageLayout::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
		ImageLayout::DepthAttachment => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
		ImageLayout::ShaderReadOnly => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
		ImageLayout::TransferSrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
		ImageLayout::TransferDst => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
		ImageLayout::Present => vk::ImageLayout::PRESENT_SRC_KHR,
		ImageLayout::DepthReadOnly => vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL,
	}
}
use  crate::domain::Stage;
fn stage_to_vk(s: Stage) -> vk::PipelineStageFlags2 {
	match s {
		Stage::None => vk::PipelineStageFlags2::NONE,
		Stage::Top => vk::PipelineStageFlags2::TOP_OF_PIPE,
		Stage::DrawIndirect => vk::PipelineStageFlags2::DRAW_INDIRECT,
		Stage::VertexInput => vk::PipelineStageFlags2::VERTEX_INPUT,
		Stage::Vertex => vk::PipelineStageFlags2::VERTEX_SHADER,
		Stage::Fragment => vk::PipelineStageFlags2::FRAGMENT_SHADER,
		Stage::EarlyFragmentTests => vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
		Stage::LateFragmentTests => vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
		Stage::ColorOutput => vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
		Stage::Compute => vk::PipelineStageFlags2::COMPUTE_SHADER,
		Stage::Transfer => vk::PipelineStageFlags2::TRANSFER,
		Stage::Host => vk::PipelineStageFlags2::HOST,
		Stage::Bottom => vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
		Stage::All => vk::PipelineStageFlags2::ALL_COMMANDS,
	}
}


fn cull_mode_to_vk(c: CullMode) -> vk::CullModeFlags {
	match c {
		CullMode::None         => vk::CullModeFlags::NONE,
		CullMode::Front        => vk::CullModeFlags::FRONT,
		CullMode::Back         => vk::CullModeFlags::BACK,
		CullMode::FrontAndBack => vk::CullModeFlags::FRONT_AND_BACK,
	}
}

fn front_face_to_vk(f: FrontFace) -> vk::FrontFace {
	match f {
		FrontFace::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE,
		FrontFace::Clockwise        => vk::FrontFace::CLOCKWISE,
	}
}
fn clear_value_to_vk(cv: &ClearValue) -> vk::ClearValue {
	match cv {
		ClearValue::Color(c) => vk::ClearValue {
			color: vk::ClearColorValue { float32: *c },
		},
		ClearValue::DepthStencil(d, s) => vk::ClearValue {
			depth_stencil: vk::ClearDepthStencilValue { depth: *d, stencil: *s },
		},
	}
}
impl From<ShaderStages> for vk::ShaderStageFlags {
	fn from(stages: ShaderStages) -> Self {
		let mut flags = vk::ShaderStageFlags::empty();
		
		if stages.0 & ShaderStages::VERTEX.0 != 0 {
			flags |= vk::ShaderStageFlags::VERTEX;
		}
		if stages.0 & ShaderStages::FRAGMENT.0 != 0 {
			flags |= vk::ShaderStageFlags::FRAGMENT;
		}
		if stages.0 & ShaderStages::COMPUTE.0 != 0 {
			flags |= vk::ShaderStageFlags::COMPUTE;
		}
		
		flags
	}
}
impl From<DescriptorType> for vk::DescriptorType {
	fn from(ty: DescriptorType) -> Self {
		match ty {
			DescriptorType::Sampler => vk::DescriptorType::SAMPLER,
			DescriptorType::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
			DescriptorType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
			DescriptorType::StorageImage => vk::DescriptorType::STORAGE_IMAGE,
			DescriptorType::UniformTexelBuffer => vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
			DescriptorType::StorageTexelBuffer => vk::DescriptorType::STORAGE_TEXEL_BUFFER,
			DescriptorType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
			DescriptorType::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
			DescriptorType::UniformBufferDynamic => vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
			DescriptorType::StorageBufferDynamic => vk::DescriptorType::STORAGE_BUFFER_DYNAMIC,
			DescriptorType::InputAttachment => vk::DescriptorType::INPUT_ATTACHMENT,
		}
	}
}

impl From<MemoryPropertyFlags> for vk::MemoryPropertyFlags {
	fn from(v: MemoryPropertyFlags) -> Self {
		vk::MemoryPropertyFlags::from_raw(v.0)
	}
}


#[derive(Debug)]
pub enum BackendError {
	Vulkan(vk::Result),
	Other(String),
}
impl std::fmt::Display for BackendError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			BackendError::Vulkan(res) => write!(f, "Vulkan error: {:?}", res),
			BackendError::Other(msg) => write!(f, "Error: {}", msg),
		}
	}
}

// This allows '?' to work with vk::Result inside functions returning BackendError
impl From<vk::Result> for BackendError {
	fn from(err: vk::Result) -> Self {
		Self::Vulkan(err)
	}
}