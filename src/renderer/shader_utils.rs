use crate::renderer::prelude::*;
pub fn build_graphics_pipeline<B: Backend>(
	pipelines: &mut PipelineManager<B>,
	device: &B::Device,
	vert_spv: &[u8],
	frag_spv: &[u8],
	desc_layouts: &[B::DescriptorSetLayout],
	push_ranges: &[PushConstantRange],
	vertex: VertexConfig,
	blend: BlendConfig,
	color_format: Format,
) -> Result<PipelineId, B::Error>
	where
		B::Device: DeviceOps<B>,
{
	let vert = device.create_shader_module(vert_spv)?;
	let frag = device.create_shader_module(frag_spv)?;
	
	let shaders = ShaderConfig {
		vert,
		frag,
		entry: c"main",
	};
	
	let slot = pipelines.create_graphics(
		desc_layouts,
		push_ranges,
		|layout| GraphicsPipelineDesc {
			shaders,
			layout,
			vertex,
			raster: RasterConfig::no_cull(),
			depth: DepthConfig::disabled(),
			blend,
			target: RenderTargetConfig { color_format },
		},
	)?;
	
	device.destroy_shader_module(vert);
	device.destroy_shader_module(frag);
	
	Ok(pipelines.push(slot))
}
// ── Vertex data for CSD ─────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex2D {
	pub pos: [f32; 2],
	pub uv: [f32; 2],
}

pub const UNIT_QUAD: [Vertex2D; 6] = [
	Vertex2D { pos: [0.0, 0.0], uv: [0.0, 0.0] },
	Vertex2D { pos: [1.0, 0.0], uv: [1.0, 0.0] },
	Vertex2D { pos: [1.0, 1.0], uv: [1.0, 1.0] },
	Vertex2D { pos: [0.0, 0.0], uv: [0.0, 0.0] },
	Vertex2D { pos: [1.0, 1.0], uv: [1.0, 1.0] },
	Vertex2D { pos: [0.0, 1.0], uv: [0.0, 1.0] },
];

vertex_layout!(
    Vertex2D,
    binding = 0,
    rate = VertexInputRate::VERTEX,
    attrs = [
        0 => (Format::R32G32_SFLOAT, pos),
        1 => (Format::R32G32_SFLOAT, uv),
    ],
    topology = PrimitiveTopology::TRIANGLE_LIST
);