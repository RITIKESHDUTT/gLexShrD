use std::fmt::Debug;
use glex_shader_macro::shader_binding;
use crate::renderer::prelude::*;
use crate::renderer::shader_utils::build_graphics_pipeline;

#[push_constant]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct TextPush {
	pub(crate) screen_size: Vec2,
	pub(crate) glyph_pos:   Vec2,
	pub(crate) glyph_size:  Vec2,
	pub(crate) uv_origin:   Vec2,
	pub(crate) uv_size:     Vec2,
	pub(crate) _pad:        Vec2,
	pub(crate) color:       Vec4,
}


// size_of = 64


binding!(
	GlyphAtlas,
	index = 0,
	set = 0,
	type = DescriptorType::CombinedImageSampler,
	stages = ShaderStages::FRAGMENT
);

descriptor_set!(TextSet: GlyphAtlas);

pub(crate) const TEXT_PUSH_RANGE: PushConstantRange =
	push_range::<TextPush>(shader_stages(ShaderStages::VERTEX, ShaderStages::FRAGMENT), 0);

pub const TEXT_PUSH_RANGES: &[PushConstantRange] = &[TEXT_PUSH_RANGE];

pub fn create_text_pipeline<B: Backend>(
	pipelines: &mut PipelineManager<B>,
	device: &B::Device,
	vert_spv: &[u8],
	frag_spv: &[u8],
	color_format: Format,
) -> Result<PipelineId, B::Error>
	where
		B::Device: DeviceOps<B>, <B as Backend>::Pipeline: Debug
{
	let layout = TextSet::BINDINGS;
	let desc_layout = device.create_descriptor_set_layout(layout)
		.expect("GPU descriptor set layout creation failed for TextSet (binding 0: CombinedImageSampler)");
	let result =	build_graphics_pipeline(
		pipelines,
		device,
		vert_spv,
		frag_spv,
		&[desc_layout],
		TEXT_PUSH_RANGES,
		VERTEX_CONFIG,
		BlendConfig::alpha(),
		color_format,
	);
	device.destroy_descriptor_set_layout(desc_layout);
	result
}