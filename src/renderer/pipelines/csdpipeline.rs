use crate::renderer::pipelines::{create_rect_pipeline, create_text_pipeline};
use crate::renderer::prelude::*;
use crate::renderer::shaders::{*};

pub struct CsdPipelines{
	rect: PipelineId,
	text: PipelineId,
}

impl CsdPipelines {
	/// Gets the ID for the rounded rectangle pipeline.
	#[inline]
	pub fn rect(&self) -> PipelineId {
		self.rect
	}
	
	/// Gets the ID for the SDF/Atlas text pipeline.
	#[inline]
	pub fn text(&self) -> PipelineId {
		self.text
	}
	pub fn load<B: Backend>(
		pipelines: &mut PipelineManager<B>,
		format: Format,
	) -> Result<Self, B::Error>
		where
			B::Device: DeviceOps<B>,
	{
		let device = pipelines.device();
		
		let rect = create_rect_pipeline(
			pipelines,
			device,
			CSD_VERT_SPV,
			CSD_FRAG_SPV,
			format,
		)?;
		
		let text = create_text_pipeline(
			pipelines,
			device,
			TEXT_VERT_SPV,
			TEXT_FRAG_SPV,
			format,
		)?;
		
		Ok(Self { rect, text })
	}
}