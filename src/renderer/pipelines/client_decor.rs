use std::fmt::Debug;
use crate::lin_al::{Vec2, Vec4};
use crate::renderer::prelude::*;
use crate::renderer::shader_utils::build_graphics_pipeline;
// ── Push constants (match shader layout) ────────────────────
#[push_constant]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct RectPush {
	pub(crate) screen_size: Vec2,
	pub(crate) rect_pos:    Vec2,
	pub(crate) rect_size:   Vec2,
	pub(crate) radius:      f32,
	pub(crate) _pad:        f32,
	pub(crate) color:       Vec4,
}


pub(crate) const RECT_PUSH_RANGE: PushConstantRange = push_range::<RectPush>(shader_stages(ShaderStages::VERTEX, ShaderStages::FRAGMENT), 0);
pub(crate) const RECT_PUSH_RANGES: &[PushConstantRange] = &[RECT_PUSH_RANGE];

pub fn create_rect_pipeline<B: Backend>(
	pipelines: &mut PipelineManager<B>,
	device: &B::Device,
	vert_spv: &[u8],
	frag_spv: &[u8],
	color_format: Format,
) -> Result<PipelineId, B::Error>
	where
		B::Device: DeviceOps<B>, <B as Backend>::Pipeline: Debug
{
	build_graphics_pipeline(
		pipelines,
		device,
		vert_spv,
		frag_spv,
		&[],
		RECT_PUSH_RANGES,
		VERTEX_CONFIG,
		BlendConfig::alpha(),
		color_format,
	)
}