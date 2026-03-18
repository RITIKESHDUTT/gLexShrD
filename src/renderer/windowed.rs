use glex_platform::csd::layout::DecorationLayout;
use crate::domain::{DescriptorSetId, UsageIntent};
use crate::domain::PassId;
use super::pipelines::{RECT_PUSH_RANGE, TEXT_PUSH_RANGE};
use super::prelude::*;
use super::*;
use glex_platform::csd::{DecorationLayer, FontAtlas, GLYPH_H, GLYPH_W};

pub struct TextAtlas<'dev> {
	_image: Image<'dev, img_state::ShaderReadOnly, VulkanBackend>,
	_view: ImageView<'dev, VulkanBackend>,
	_sampler: Sampler<'dev, VulkanBackend>,
	_descriptor_pool: DescriptorPool<'dev, VulkanBackend>,
	pub descriptor_set: DescriptorSet<'dev, desc_state::Updated, VulkanBackend, TextSet>,
}

pub struct QuadMesh<'dev> {
	pub unit_quad_vert_buf: Buffer<'dev, buf_state::VertexBuffer, VulkanBackend>,
}

pub struct CsdResources<'dev> {
	pub atlas: TextAtlas<'dev>,
	pub quad: QuadMesh<'dev>,
}

impl<'dev> CsdResources<'dev> {
	pub fn upload(
		ctx: &'dev VulkanContext,
		gpu: &mut GpuContext<'dev, VulkanBackend>,
		sampler_layout: &DescriptorLayout<'dev, VulkanBackend, TextSet>,
	) -> Result<Self, Box<dyn std::error::Error>> {
		let device = ctx.device();
		let lane   = gpu.graphics_lane_mut();
		let fam    = lane.family();
		let size   = data_size(&UNIT_QUAD);
		
		let staging_buf = ctx.staging_upload(size, fam)?;
		staging_buf.with_mapped::<Vertex2D, _, _>(UNIT_QUAD.len(), |dst| {
			dst.copy_from_slice(&UNIT_QUAD);
		})?;
		
		let vb_index      = ctx.indices.device_local;
		let usage = BufferUsage::VERTEX | BufferUsage::TRANSFER_DST;
		let vertex_buffer = Buffer::allocate(device, size, usage, vb_index, fam)?;
		
		let cmd = lane.allocate()?.begin()?;
		let staging_buf   = staging_buf.into_transfer_src(&cmd);
		let vertex_buffer = vertex_buffer.into_transfer_dst(&cmd);
		cmd.copy_buffer(&staging_buf, &vertex_buffer, size);
		
		let vertex_buffer = vertex_buffer.into_vertex_buffer(&cmd);
		
		let cmd           = cmd.end()?;
		lane.submit(device, cmd, &[])?;
		
		// ── Build glyph atlas ────────────────────────────────────────────────
		let atlas    = FontAtlas::build();
		let (aw, ah) = atlas.dimensions();
		let pixels   = atlas.pixels();
		
		let atlas_staging = ctx.staging_upload(pixels.len() as u64, fam)?;
		atlas_staging.with_mapped::<u8, _, _>(pixels.len(), |dst| {
			dst.copy_from_slice(pixels);
		})?;
		
		let image = Image::allocate_2d(
			device,
			Format::R8_UNORM,
			Extent2D::new(aw, ah),
			ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
			ctx.indices.device_local,
			fam,
		)?;
		
		let cmd           = lane.allocate()?.begin()?;
		let atlas_staging = atlas_staging.into_transfer_src(&cmd);
		let image         = image.into_transfer_dst(&cmd);
		cmd.copy_buffer_to_image(&atlas_staging, &image);
		let image         = image.into_shader_read(&cmd);
		let cmd           = cmd.end()?;
		let val           = lane.submit(device, cmd, &[])?;
		
		device.wait_semaphore(lane.timeline_handle(), val)?;
		
		let view    = ImageView::color_2d(device, image.handle(), Format::R8_UNORM)?;
		let sampler = Sampler::new(device, Filter::LINEAR, SamplerAddressMode::CLAMP_TO_EDGE)?;
		
		let pool = DescriptorPool::new(
			device, 1,
			&[DescriptorPoolSize {
				descriptor_type: DescriptorType::CombinedImageSampler,
				count: 1,
			}],
		)?;
		
		let set = DescriptorSet::<desc_state::Unallocated, VulkanBackend, TextSet>
		::allocate(device, &pool, &sampler_layout)?
			.write_image_sampler::<GlyphAtlas>(&sampler, view.handle())
			.finish();
		
		Ok(Self {
			atlas: TextAtlas {
				_image: image,
				_view: view,
				_sampler: sampler,
				_descriptor_pool: pool,
				descriptor_set: set,
			},
			quad: QuadMesh { unit_quad_vert_buf: vertex_buffer },
		})
	}
}

// ─────────────────────────────────────────────────────────────────────────────

/// Record a decoration layer (rects + glyphs) into the frame graph.
///
/// Used for both the cached base layer (frame, title bar, separator, title
/// text — rebuilt only on resize/title change) and the per-frame button
/// layer (close/maximize/minimize — rebuilt every frame on hover/press).
///
/// Callers control caching policy; this function is a pure recorder.
pub fn record_csd_layer(
	graph:     &mut FrameGraph,
	resources: &CsdResources,
	pipelines: &CsdPipelines,
	layer:     &DecorationLayer,
	layout:    &DecorationLayout,
	atlas_set: DescriptorSetId,
) -> PassId {
	let screen_size = Vec2::new(
		layout.size().width()  as f32,
		layout.size().height() as f32,
	);

	let quad     = &resources.quad.unit_quad_vert_buf;
	let quad_res = graph.add_buffer(VulkanBackend::buffer_handle(quad.handle()));

	let mut builder = graph
		.add_graphics_pass(None)
		.reads(quad_res, UsageIntent::vertex_buffer_read())
		.bind_pipeline(pipelines.rect())
		.bind_vertex_buffer(quad_res);

	for rect in layer.rects() {
		let b    = rect.bounds();
		let c    = rect.color();
		let push = RectPush {
			screen_size,
			rect_pos:  [b.x() as f32, b.y() as f32].into(),
			rect_size: [b.width() as f32, b.height() as f32].into(),
			radius:    rect.radius() as f32,
			_pad:      0.0,
			color:     [c.r(), c.g(), c.b(), c.a()].into(),
		};
		builder = builder
			.push_constants(RECT_PUSH_RANGE, push_data(&push))
			.draw(6);
	}

	builder = builder
		.bind_pipeline(pipelines.text())
		.bind_descriptor_set(atlas_set);

	for glyph in layer.glyphs() {
		let c    = glyph.color();
		let uv   = glyph.uv();
		let push = TextPush {
			screen_size,
			glyph_pos:  [glyph.x() as f32, glyph.y() as f32].into(),
			glyph_size: [GLYPH_W as f32, GLYPH_H as f32].into(),
			uv_origin:  [uv[0], uv[1]].into(),
			uv_size:    [uv[2] - uv[0], uv[3] - uv[1]].into(),
			_pad:       [0.0, 0.0].into(),
			color:      [c.r(), c.g(), c.b(), c.a()].into(),
		};
		builder = builder
			.push_constants(TEXT_PUSH_RANGE, push_data(&push))
			.draw(6);
	}

	builder.submit()
}