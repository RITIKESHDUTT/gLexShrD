use crate::core::types::{
	BlendFactor,
	BlendOp, CompareOp, CullMode,
	Format, FrontFace, GraphicsPipelineDesc, PolygonMode,
	PrimitiveTopology, PushConstantRange, VertexAttributeDesc, VertexBindingDesc,
};
use crate::core::{Backend, DeviceOps, };

// ── Storage ──────────────────────────────────────────────
pub struct PipelineSlot<B: Backend> {
	handle: B::Pipeline,
	layout: B::PipelineLayout,
}
pub struct PipelineManager<'dev, B: Backend> {
	device: &'dev B::Device,
	entries: Vec<PipelineSlot<B>>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PipelineId(pub u32);

pub struct VertexConfig<'a> {
	pub bindings:   &'a [VertexBindingDesc],
	pub attributes: &'a [VertexAttributeDesc],
	pub topology:   PrimitiveTopology,
}

pub struct RasterConfig {
	pub cull:         CullMode,
	pub front_face:   FrontFace,
	pub polygon_mode: PolygonMode,
}

pub struct DepthConfig {
	pub test: bool,
	pub write: bool,
	pub compare: CompareOp,
}

pub struct BlendConfig {
	pub enable: bool,
	pub src_color: BlendFactor,
	pub dst_color: BlendFactor,
	pub color_op: BlendOp,
	pub src_alpha: BlendFactor,
	pub dst_alpha: BlendFactor,
	pub alpha_op: BlendOp,
}

pub struct RenderTargetConfig {
	pub color_format: Format,
}


impl BlendConfig {
	pub fn opaque() -> Self {
		Self {
			enable: false,
			src_color: BlendFactor::ONE,
			dst_color: BlendFactor::ZERO,
			color_op: BlendOp::ADD,
			src_alpha: BlendFactor::ONE,
			dst_alpha: BlendFactor::ZERO,
			alpha_op: BlendOp::ADD,
		}
	}
	pub fn alpha() -> Self {
		Self {
			enable: true,
			src_color: BlendFactor::SRC_ALPHA,
			dst_color: BlendFactor::ONE_MINUS_SRC_ALPHA,
			color_op: BlendOp::ADD,
			src_alpha: BlendFactor::ONE,
			dst_alpha: BlendFactor::ONE_MINUS_SRC_ALPHA,
			alpha_op: BlendOp::ADD,
		}
	}
	pub fn additive() -> Self {
		Self {
			enable: true,
			src_color: BlendFactor::SRC_ALPHA,
			dst_color: BlendFactor::ONE,
			color_op: BlendOp::ADD,
			src_alpha: BlendFactor::ONE,
			dst_alpha: BlendFactor::ONE,
			alpha_op: BlendOp::ADD,
		}
	}
	pub fn premultiplied() -> Self {
		Self {
			enable: true,
			src_color: BlendFactor::ONE,
			dst_color: BlendFactor::ONE_MINUS_SRC_ALPHA,
			color_op: BlendOp::ADD,
			src_alpha: BlendFactor::ONE,
			dst_alpha: BlendFactor::ONE_MINUS_SRC_ALPHA,
			alpha_op: BlendOp::ADD,
		}
	}
	pub fn additive_preserve_alpha() -> Self {
		Self {
			enable: true,
			src_color: BlendFactor::SRC_ALPHA,
			dst_color: BlendFactor::ONE,
			color_op: BlendOp::ADD,
			src_alpha: BlendFactor::ZERO,   // don't write particle alpha
			dst_alpha: BlendFactor::ONE,    // keep CSD's alpha
			alpha_op: BlendOp::ADD,
		}
	}
	pub fn additive_premul() -> Self {
		Self {
			enable: true,
			src_color: BlendFactor::ONE,     // was SRC_ALPHA — now uses premultiplied color
			dst_color: BlendFactor::ONE,
			color_op: BlendOp::ADD,
			src_alpha: BlendFactor::ZERO,    // don't add particle alpha to framebuffer
			dst_alpha: BlendFactor::ONE,     // keep CSD's window alpha
			alpha_op: BlendOp::ADD,
		}
	}
}

impl RasterConfig {
	pub fn no_cull() -> Self {
		Self { cull: CullMode::None, front_face: FrontFace::CounterClockwise, polygon_mode: PolygonMode::FILL }
	}
	pub fn back_cull() -> Self {
		Self { cull: CullMode::Back, front_face: FrontFace::CounterClockwise, polygon_mode: PolygonMode::FILL }
	}
}
impl Default for  RasterConfig {
	fn default() -> Self {
		Self::no_cull()
	}
}

impl DepthConfig {
	pub fn disabled() -> Self {
		Self { test: false, write: false, compare: CompareOp::LESS }
	}
}


impl<'dev, B: Backend> PipelineManager<'dev, B>
	where B::Device: DeviceOps<B>
{
	pub fn create_graphics<'a>(
		&self,
		desc_layouts: &[B::DescriptorSetLayout],
		push_ranges: &[PushConstantRange],
		desc: impl FnOnce(B::PipelineLayout) -> GraphicsPipelineDesc<'a, B>,
	) -> Result<PipelineSlot<B>, B::Error> {
		let layout = self.device.create_pipeline_layout(desc_layouts, push_ranges)?;
		let pipeline_desc = desc(layout);
		let handle = self.device.create_graphics_pipeline(&pipeline_desc)?;
		Ok(PipelineSlot { handle, layout })
	}
	pub fn create_compute(
		&self,
		desc_layouts: &[B::DescriptorSetLayout],
		push_ranges: &[PushConstantRange],
		module: B::ShaderModule,
	) -> Result<PipelineSlot<B>, B::Error> {
		let layout = self.device.create_pipeline_layout(desc_layouts, push_ranges)?;
		let handle = self.device.create_compute_pipeline(module, layout)?;
		Ok(PipelineSlot { handle, layout })
	}

	
	pub fn push(&mut self, slot: PipelineSlot<B>) -> PipelineId {
		let id = PipelineId(self.entries.len() as u32);
		self.entries.push(slot);
		id
	}
	pub fn new(device: &'dev B::Device) -> Self {
		Self { device, entries: Vec::new() }
	}
	
	pub fn device(&self) -> &'dev B::Device { self.device }
	
	pub fn handle(&self, id: PipelineId) -> B::Pipeline {
		self.entries[id.0 as usize].handle
	}
	
	pub fn layout(&self, id: PipelineId) -> B::PipelineLayout {
		self.entries[id.0 as usize].layout
	}
}

impl<B: Backend> Drop for PipelineManager<'_, B>
	where B::Device: DeviceOps<B>
{
	fn drop(&mut self) {
		for slot in &self.entries {
			self.device.destroy_pipeline(slot.handle);
			self.device.destroy_pipeline_layout(slot.layout);
		}
	}
}