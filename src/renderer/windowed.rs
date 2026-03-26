use glex_platform::csd::state::DecorationState;
use crate::domain::{DescriptorSetId, UsageIntent};
use crate::domain::PassId;
use super::pipelines::{RECT_PUSH_RANGE, TEXT_PUSH_RANGE};
use super::prelude::*;
use super::*;
use glex_platform::csd::{DecorationLayer, FontAtlas, GLYPH_H, GLYPH_W};
use glex_platform::csd::build::{DecorationBuilder, StandardDecorations};
use glex_platform::csd::layout::DecorationLayout;
use crate::infra::{FrameInfo, Pass};
use crate::Glex;
use tracing::{debug, info, instrument, trace, warn};

// =============================================================================
// TextAtlas
// =============================================================================

pub struct TextAtlas<'dev> {
	_image:           Image<'dev, img_state::ShaderReadOnly, VulkanBackend>,
	_view:            ImageView<'dev, VulkanBackend>,
	_sampler:         Sampler<'dev, VulkanBackend>,
	pub descriptor_set: DescriptorSet<'dev, desc_state::Updated, VulkanBackend, TextSet>,
}

impl<'dev> TextAtlas<'dev> {
	#[instrument(skip(self), level = "trace")]
	pub(crate) fn finalize_lifetime(&mut self, t: u64) {
		trace!(timeline_val = t, "Finalizing TextAtlas lifetime");
		self._image.finalize_lifetime(t);
	}
}

// =============================================================================
// QuadMesh
// =============================================================================

pub struct QuadMesh<'dev> {
	pub quad_buf: Buffer<'dev, buf_state::VertexBuffer, VulkanBackend>,
}

impl<'dev> QuadMesh<'dev> {
	#[instrument(skip(self), level = "trace")]
	pub(crate) fn finalize_lifetime(&mut self, t: u64) {
		trace!(timeline_val = t, "Finalizing QuadMesh lifetime");
		self.quad_buf.finalize_lifetime(t);
	}
}

// =============================================================================
// CsdResources
// =============================================================================

pub struct CsdResources<'dev> {
	pub atlas: TextAtlas<'dev>,
	pub quad:  QuadMesh<'dev>,
	_descriptor_pool: DescriptorPool<'dev, VulkanBackend>,
}

impl<'dev> CsdResources<'dev> {
	#[instrument(skip_all, name = "CsdResources::setup")]
	pub fn setup<'glex>(
		ctx:  &'dev VulkanContext,
		glex: &'glex mut Glex<'dev>,
	) -> Result<Self, Box<dyn std::error::Error>> {
		info!("Setting up CSD resources");
		
		let device = ctx.device();
		let fam    = glex.gpu_mut().graphics_lane().family();
		
		// ---------------------------------------------------------
		// QUAD BUFFER
		// ---------------------------------------------------------
		
		let quad_size = data_size(&UNIT_QUAD);
		trace!(quad_size, family = fam, "Allocating quad buffer");
		
		let quad_staging = glex.gpu_mut().staging_upload(quad_size, fam)?;
		let quad_buf         = glex.gpu_mut().vertex_buffer(quad_size, fam)?;
		
		quad_staging.with_mapped::<_, _, _>(UNIT_QUAD.len(), |dst| {
			dst.copy_from_slice(&UNIT_QUAD);
		})?;
		
		let lane = glex.gpu_mut().graphics_lane_mut();
		let cmd  = lane.allocate()?.begin()?;
		
		let quad_src = quad_staging.into_transfer_src(&cmd);
		let quad_dst = quad_buf.into_transfer_dst(&cmd);
		
		cmd.copy_buffer(&quad_src, &quad_dst, quad_size);
		
		let quad_buf = quad_dst.into_vertex_buffer(&cmd);
		
		let cmd = cmd.end()?;
		let val = lane.submit(device, cmd, &[])?;
		trace!(signal_val = val, "Quad buffer upload submitted");
		
		let mut quad_src = quad_src;
		quad_src.finalize_lifetime(val);
		
		// ---------------------------------------------------------
		// ATLAS IMAGE
		// ---------------------------------------------------------
		
		let atlas    = FontAtlas::build();
		let (aw, ah) = atlas.dimensions();
		let pixels   = atlas.pixels();
		
		let bytes_per_pixel = 1u64;
		let row_size        = aw as u64 * bytes_per_pixel;
		let aligned_row     = (row_size + 3) & !3;
		let atlas_size      = aligned_row * ah as u64;
		
		trace!(atlas_w = aw, atlas_h = ah, atlas_size, aligned_row, "Uploading font atlas");
		
		let atlas_staging = glex.gpu_mut().staging_upload(atlas_size, fam)?;
		
		atlas_staging.with_mapped::<u8, _, _>(atlas_size as usize, |dst| {
			for y in 0..ah as usize {
				let src_row = &pixels[y * aw as usize .. (y + 1) * aw as usize];
				let dst_row = &mut dst[(y * aligned_row as usize)..][..aw as usize];
				dst_row.copy_from_slice(src_row);
			}
		})?;
		
		let image = glex.gpu_mut().allocate_image_2d(
			Format::R8_UNORM,
			Extent2D::new(aw, ah),
			ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
			fam,
		)?;
		
		let lane = glex.gpu_mut().graphics_lane_mut();
		let cmd  = lane.allocate()?.begin()?;
		
		let src       = atlas_staging.into_transfer_src(&cmd);
		let image_dst = image.into_transfer_dst(&cmd);
		
		cmd.copy_buffer_to_image(&src, 0, &image_dst);
		
		let image = image_dst.into_shader_read(&cmd);
		
		let cmd = cmd.end()?;
		let val = lane.submit(device, cmd, &[])?;
		trace!(signal_val = val, "Atlas image upload submitted");
		
		let mut src = src;
		src.finalize_lifetime(val);
		
		// ---------------------------------------------------------
		// VIEW + SAMPLER + DESCRIPTOR
		// ---------------------------------------------------------
		
		let view    = ImageView::color_2d(device, image.handle(), Format::R8_UNORM)?;
		let sampler = Sampler::new(device, Filter::LINEAR, SamplerAddressMode::CLAMP_TO_EDGE)?;
		
		let pool = DescriptorPool::new(
			device,
			1,
			&[DescriptorPoolSize {
				descriptor_type: DescriptorType::CombinedImageSampler,
				count: 1,
			}],
		)?;
		
		let set = DescriptorSet::image_sampler::<GlyphAtlas>(
			device,
			&pool,
			glex.sampler_layout(),
			&sampler,
			&view,
		)?;
		
		info!("CSD resources setup complete");
		
		Ok(Self {
			atlas: TextAtlas {
				_image: image,
				_view: view,
				_sampler: sampler,
				descriptor_set: set,
			},
			quad: QuadMesh { quad_buf },
			_descriptor_pool: pool,
		})
	}
	
	pub fn finalize_lifetime(&mut self, t: u64) {
		self.atlas.finalize_lifetime(t);
		self.quad.finalize_lifetime(t);
	}
}

// =============================================================================
// record_csd_layer
// =============================================================================

#[instrument(skip_all, level = "debug")]
pub fn record_csd_layer(
	graph:     &mut FrameGraph,
	pipelines: &CsdPipelines,
	layer:     &DecorationLayer,
	layout:    &DecorationLayout,
	atlas_set: DescriptorSetId,
	quad_res:  crate::domain::ResourceId,
) -> PassId {
	debug!(
		rect_count  = layer.rects().len(),
		glyph_count = layer.glyphs().len(),
		"Recording CSD layer"
	);
	
	let screen_size = Vec2::new(
		layout.size().width()  as f32,
		layout.size().height() as f32,
	);
	
	let mut builder = graph
		.add_graphics_pass(None)
		.reads(quad_res, UsageIntent::vertex_buffer_read())
		.bind_pipeline(pipelines.rect())
		.bind_vertex_buffer(quad_res, 0);
	
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
			_pad2:     [0.0, 0.0, 0.0, 0.0].into(),
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
// =============================================================================
// CsdPass
// =============================================================================

pub struct CsdPass<'dev> {
	resources:    CsdResources<'dev>,
	pipelines:    CsdPipelines,
	atlas_set_id: DescriptorSetId,
	base_layer:    Option<DecorationLayer>,
	button_layer:  Option<DecorationLayer>,
	is_fullscreen: bool,
	screen_size:   Vec2,
	layout:        Option<DecorationLayout>,
}

impl<'dev> CsdPass<'dev> {
	#[instrument(skip_all, name = "CsdPass::setup")]
	pub fn setup<'glex>(
		ctx:  &'dev VulkanContext,
		glex: &'glex mut Glex<'dev>,
	) -> Result<Self, Box<dyn std::error::Error>> {
		info!("Setting up CsdPass");
		let format       = glex.format();
		let resources    = CsdResources::setup(ctx, glex)?;
		let atlas_handle = resources.atlas.descriptor_set.handle();
		let pipelines    = CsdPipelines::load(glex.pipelines(), format)?;
		let atlas_set_id = glex.register_descriptor_set(atlas_handle);
		
		info!("CsdPass setup complete");
		
		Ok(Self {
			resources,
			pipelines,
			atlas_set_id,
			base_layer:  None,
			button_layer:      None,
			is_fullscreen:     false,
			screen_size:       Vec2::new(0.0, 0.0),
			layout:            None,
		})
	}
	
	pub(crate) fn begin_frame(
		&mut self,
		layout: &DecorationLayout,
		is_fullscreen: bool,
		title: &str,
		theme: &glex_platform::csd::CsdTheme,
		state: &DecorationState,
	) {
		self.is_fullscreen = is_fullscreen;
		
		if is_fullscreen {
			self.base_layer   = None;
			self.button_layer = None;
			self.layout       = None;
			return;
		}
		
		self.screen_size = Vec2::new(
			layout.size().width()  as f32,
			layout.size().height() as f32,
		);
		
		self.base_layer   = Some(StandardDecorations.build_base(layout, theme, title));
		self.button_layer = Some(StandardDecorations.build_buttons(layout, state, theme));
		self.layout       = Some(layout.clone());
	}
	
	// ── Background: window body fill + title bar + title text ────────────
	// Call BEFORE user passes so rounded rect body is behind user content.
	
	pub(crate) fn record_background(&self, graph: &mut FrameGraph, _info: &FrameInfo) {
		if self.is_fullscreen {
			trace!("CSD background skipped — fullscreen");
			return;
		}
		
		let layout = match &self.layout {
			Some(l) => l,
			None => return,
		};
		
		let quad_buf = &self.resources.quad.quad_buf;
		let quad_res = graph.add_buffer(
			VulkanBackend::buffer_handle(quad_buf.handle()),
			0,
			quad_buf.size(),
		);
		
		if let Some(base) = &self.base_layer {
			trace!(
				rects = base.rects().len(),
				glyphs = base.glyphs().len(),
				"Recording CSD background layer"
			);
			record_csd_layer(
				graph, &self.pipelines,
				base, layout, self.atlas_set_id, quad_res,
			);
		}
	}
	
	// ── Foreground: close / minimize / maximize buttons ──────────────────
	// Call AFTER user passes so buttons sit on top of everything.
	
	pub(crate) fn record_foreground(&self, graph: &mut FrameGraph, _info: &FrameInfo) {
		if self.is_fullscreen {
			trace!("CSD foreground skipped — fullscreen");
			return;
		}
		
		let layout = match &self.layout {
			Some(l) => l,
			None => return,
		};
		
		let quad_buf = &self.resources.quad.quad_buf;
		let quad_res = graph.add_buffer(
			VulkanBackend::buffer_handle(quad_buf.handle()),
			0,
			quad_buf.size(),
		);
		
		if let Some(buttons) = &self.button_layer {
			trace!(
				rects = buttons.rects().len(),
				glyphs = buttons.glyphs().len(),
				"Recording CSD foreground layer"
			);
			record_csd_layer(
				graph, &self.pipelines,
				buttons, layout, self.atlas_set_id, quad_res,
			);
		}
	}
}

impl<'dev> Pass<'dev> for CsdPass<'dev> {
	fn record(&self, graph: &mut FrameGraph, info: &FrameInfo) {
		self.record_background(graph, info);
		self.record_foreground(graph, info);
	}
	
	fn finalize(&mut self, t: u64) {
		self.resources.finalize_lifetime(t);
	}
}