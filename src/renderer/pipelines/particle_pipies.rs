use std::fmt::Debug;
use crate::renderer::{ComputeStorage, GfxStorage};
use crate::renderer::shader_utils::build_compute_pipeline;
use crate::renderer::build_graphics_pipeline;
use crate::renderer::PARTICLE_COMP_SPV;
use crate::renderer::PARTICLE_VERT_SPV;
use crate::renderer::PARTICLE_FRAG_SPV;
use crate::renderer::prelude::*;
use crate::renderer::shaders::{ComputePush, GfxPush};

#[allow(dead_code)]
pub const COMP_PUSH_RANGE: PushConstantRange =
	push_range::<ComputePush>(ShaderStages::COMPUTE, 0);

#[allow(dead_code)]
pub const GFX_PUSH_RANGE: PushConstantRange =
	push_range::<GfxPush>(
		shader_stages(ShaderStages::VERTEX, ShaderStages::FRAGMENT),
		0
	);

#[allow(dead_code)]
pub struct ParticleCPipelines<'a, B: Backend> {
	compute: PipelineId,
	layout: DescriptorLayout<'a, B, ComputeStorage>,
}
#[allow(dead_code)]
impl<'a, B: Backend> ParticleCPipelines<'a, B>
	where
		B::Device: DeviceOps<B>,
{
	
	pub fn compute(&self) -> PipelineId { self.compute }
	
	pub fn load(
		pipelines: &mut PipelineManager<'a, B>,
	) -> Result<Self, B::Error>
		where
			B::Device: DeviceOps<B>, <B as Backend>::DescriptorSetLayout: Debug, <B as Backend>::Pipeline: Debug
	{
		let device = pipelines.device();
		
		let desc_layout =
			DescriptorLayout::<B, ComputeStorage>::new(device)?;
		
		let compute = build_compute_pipeline(
			pipelines,
			device,
			PARTICLE_COMP_SPV,
			&[desc_layout.handle()],
			&[COMP_PUSH_RANGE],
		)?;
		
		Ok(Self { compute, layout:desc_layout })
	}
}

#[allow(dead_code)]
pub struct ParticleGPipelines<'a, B: Backend> {
	render: PipelineId,
	layout: DescriptorLayout<'a, B, GfxStorage>,
}
impl<'a, B: Backend> ParticleGPipelines<'a, B>
	where
		B::Device: DeviceOps<B>,
{
	#[allow(dead_code)]
	pub fn render(&self) -> PipelineId { self.render }
	
	#[allow(dead_code)]
	pub fn load(
		pipelines: &mut PipelineManager<'a, B>,
		format: Format,
	) -> Result<Self, B::Error>
		where
			B::Device: DeviceOps<B>, <B as crate::core::Backend>::DescriptorSetLayout: Debug, <B as Backend>::Pipeline: Debug
	{
		let device = pipelines.device();
		let desc_layout = DescriptorLayout::<B, GfxStorage>::new(device)?;
		
		let render = build_graphics_pipeline(
			pipelines,
			device,
			PARTICLE_VERT_SPV,
			PARTICLE_FRAG_SPV,
			&[desc_layout.handle()],
			&[GFX_PUSH_RANGE],
			VertexConfig {
				bindings: &[],
				attributes: &[],
				topology: PrimitiveTopology::POINT_LIST,
			},
			BlendConfig::additive(),
			format,
		)?;
		
		Ok(Self { render, layout:desc_layout})
	}
}