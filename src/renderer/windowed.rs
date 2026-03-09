use super::*;
use super::pipelines::{RECT_PUSH_RANGE, TEXT_PUSH_RANGE};
use super::prelude::*;
use glex_platform::csd::{GLYPH_W, GLYPH_H,DecorationDraw, FontAtlas};

pub struct TextAtlas<'dev> {
	pub image: Image<'dev, img_state::ShaderReadOnly, VulkanBackend>,
	pub view: ImageView<'dev, VulkanBackend>,
	pub sampler: Sampler<'dev, VulkanBackend>,
	pub descriptor_pool: DescriptorPool<'dev, VulkanBackend>,
	pub descriptor_set: DescriptorSet<'dev, desc_state::Updated, VulkanBackend, TextSet>,
	pub layout: DescriptorLayout<'dev, VulkanBackend, TextSet>,
}

pub struct QuadMesh<'dev> {
	pub unit_quad_vert_buf: Buffer<'dev, buf_state::VertexBuffer, VulkanBackend>,
}

pub struct CsdResources<'dev> {
	pub atlas: TextAtlas<'dev>,
	pub quad: QuadMesh<'dev>,
}

impl<'dev, > CsdResources<'dev> {
	pub fn upload (
		ctx: &'dev VulkanContext,
		gpu: &mut GpuContext<'dev, VulkanBackend>,
		sampler_layout: DescriptorLayout<'dev, VulkanBackend, TextSet>
	) -> Result<Self, Box<dyn std::error::Error>>{
		let device = ctx.device();
		let lane = gpu.graphics_lane_mut();
		
		let fam = lane.family();
		let size = data_size(&UNIT_QUAD);
		
		let staging_buf = ctx.staging_upload(size, fam,)?;
		
		// 4. copy quad data
		staging_buf.with_mapped::<Vertex2D, _, _>(UNIT_QUAD.len(), |dst| {
			dst.copy_from_slice(&UNIT_QUAD);
		})?;
		let vb_index = ctx.indices.device_local;
		let usage = BufferUsage::VERTEX | BufferUsage::TRANSFER_DST;
		let vertex_buffer = Buffer::allocate(device, size, usage, vb_index, fam, )?;
		
		let cmd = lane.allocate()?.begin()?;
		
		let staging_buf = staging_buf.into_transfer_src(&cmd);
		let vertex_buffer = vertex_buffer.into_transfer_dst(&cmd);
		
		cmd.copy_buffer(&staging_buf, &vertex_buffer, size, );
		let vertex_buffer = vertex_buffer.into_vertex_buffer(&cmd);
		
		let cmd = cmd.end()?;
		lane.submit(device, cmd, &[])?;
		
		
		// ── Build real glyph atlas ──────────────────────────
		let atlas = FontAtlas::build();
		let (atlas_w, atlas_h) = atlas.dimensions();
		let pixels = atlas.pixels();
		
		// Stage atlas pixels
		let atlas_staging = ctx.staging_upload(pixels.len() as u64, fam)?;
		atlas_staging.with_mapped::<u8, _, _>(pixels.len(), |dst| {
			dst.copy_from_slice(pixels);
		})?;
		
		// Allocate device-local image
		let image = Image::allocate_2d(
			device,
			Format::R8_UNORM,
			Extent2D::new(atlas_w, atlas_h),
			ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
			ctx.indices.device_local,
			fam,
		)?;
		
		// Upload atlas to GPU
		let cmd = lane.allocate()?.begin()?;
		let atlas_staging = atlas_staging.into_transfer_src(&cmd);
		let image = image.into_transfer_dst(&cmd);
		cmd.copy_buffer_to_image(&atlas_staging, &image);
		let image = image.into_shader_read(&cmd);
		let cmd = cmd.end()?;
		let val = lane.submit(device, cmd, &[])?;
		
		
		
		// Wait only on the final timeline value — covers both submits
		device.wait_semaphore(lane.timeline_handle(), val)?;
		
		
		let view = ImageView::color_2d(
			device,
			image.handle(),
			Format::R8_UNORM,
		)?;

		let sampler = Sampler::new(
			device,
			Filter::LINEAR,
			SamplerAddressMode::CLAMP_TO_EDGE,
			)?;
		let pool = DescriptorPool::new(device, 1, &[DescriptorPoolSize { descriptor_type: DescriptorType::CombinedImageSampler, count: 1, }], )?;
		let set = DescriptorSet::<desc_state::Unallocated, VulkanBackend, TextSet>::allocate(device, &pool, &sampler_layout, )?;
		
		let set = set
			.write_image_sampler::<GlyphAtlas>(&sampler, view.handle())
			.finish();
		
		Ok(Self {
			atlas: TextAtlas {
				image,
				view,
				sampler,
				descriptor_pool: pool,
				descriptor_set: set,
				layout: sampler_layout,
			},
			quad: QuadMesh {
				unit_quad_vert_buf: vertex_buffer,
			},
		})
	}
}

pub fn record_csd(
	recorder: &mut RenderRecorder2D<VulkanBackend>,
	resources: &CsdResources,
	pipelines: &CsdPipelines,
	draw: &DecorationDraw,
	screen_size: Vec2,
) {
	let quad = &resources.quad.unit_quad_vert_buf;
	
	// ── RECT ─────────────────────────────────────────────
	recorder.bind_pipeline(pipelines.rect());
	recorder.bind_vertex_buffer(quad);
	
	for rect in draw.rects() {
		let b = rect.bounds();
		let c = rect.color();
		let push = RectPush {
			screen_size,
			rect_pos:  [b.x() as f32, b.y() as f32].into(),
			rect_size: [b.width() as f32, b.height() as f32].into(),
			radius:    rect.radius() as f32,
			_pad:      0.0,
			color:     [c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32].into(),
		};
		
		
		recorder.push_constants(RECT_PUSH_RANGE.stages(), RECT_PUSH_RANGE.offset(), &push);
		
		recorder.draw(6);
	}
	
	// ── TEXT ─────────────────────────────────────────────
	recorder.bind_pipeline(pipelines.text());
	recorder.bind_vertex_buffer(quad);
	recorder.bind_descriptor_set_ref(&resources.atlas.descriptor_set);
	
	for glyph in draw.glyphs() {
			let c = glyph.color();
			let uv = glyph.uv();
			
			let push = TextPush {
				screen_size,
				glyph_pos:  [glyph.x() as f32, glyph.y() as f32].into(),
				glyph_size: [GLYPH_W as f32, GLYPH_H as f32].into(),
				uv_origin:  [uv[0], uv[1]].into(),
				uv_size:    [uv[2] - uv[0], uv[3] - uv[1]].into(),
				_pad:       [0.0, 0.0].into(),
				color:      [c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32].into(),
			};
		
		recorder.push_constants(TEXT_PUSH_RANGE.stages(), TEXT_PUSH_RANGE.offset(), &push);
	recorder.draw(6);
	}
}