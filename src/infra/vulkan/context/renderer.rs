use super::GpuContext;
use crate::{
	core::{Backend, DescriptorLayout, DescriptorSetInterface, PipelineManager},
	infra::{Presentation, VulkanBackend},
};

pub struct Rendering<'dev, I>
where
	I: DescriptorSetInterface,
{
	pub pipelines: PipelineManager<'dev, VulkanBackend>,
	pub sampler_layout: DescriptorLayout<'dev, VulkanBackend, I>,
	format: <VulkanBackend as Backend>::Format,
}
impl<'dev, I> Rendering<'dev, I>
where
	I: DescriptorSetInterface,
{
	pub fn upload(
		gpu: &mut GpuContext<'dev, VulkanBackend>,
		presentation: &Presentation,
	) -> Result<Self, Box<dyn std::error::Error>> {
		let device = gpu.device();
		let format = presentation.format();
		let pipelines = PipelineManager::new(device);
		
		let sampler_layout =
			DescriptorLayout::<VulkanBackend, I>::new(device)?;
		Ok(Self { pipelines, sampler_layout, format })
	}
	
	pub fn format(&self) -> <VulkanBackend as Backend>::Format {
		self.format
	}
}