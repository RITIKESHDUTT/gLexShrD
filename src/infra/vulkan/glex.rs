use crate::infra::VulkanBackend;
use glex_platform::csd::layout::DecorationLayout;
use glex_platform::platform::ControlFlow;
use crate::infra::vulkan::context::PresentMode;
use crate::renderer::record_csd_layer;
use glex_platform::csd::{Color, DecorationLayer};
use glex_platform::platform::Extent2D;
use crate::domain::DescriptorSetId;
use crate::core::FrameGraph;
use crate::core::RenderTarget;
use crate::core::SwapchainImage;
use crate::infra::platform::Surface;
use crate::infra::platform::VulkanWindow;
use crate::infra::platform::WaylandWindowImpl;
use glex_platform::csd::build::DecorationBuilder;
use glex_platform::csd::build::StandardDecorations;
use glex_platform::platform::Window;
use crate::core::PipelineManager;
use crate::renderer::CsdPipelines;
use crate::renderer::CsdResources;
use crate::renderer::TextSet;
use super::context::{GpuContext, Presentation, Rendering, VulkanContext};
use super::VulkanDevice;

pub trait Pass<'dev> {
	fn update(&mut self, _frame_index: u32) {}
	fn record(&self, graph: &mut FrameGraph, info: &FrameInfo);
}

//read-only input to passes.
pub struct FrameInfo {
	pub extent: Extent2D, // swapchain
	pub frame_index: u32,
	pub viewport_offset: (f32, f32),
	pub viewport_extent: (f32, f32),
}
impl FrameInfo {
	pub fn from_layout(
		swap_extent: Extent2D,
		frame_index: u32,
		is_fullscreen: bool,
		layout: &DecorationLayout
	) -> Self {
		let (content_offset, content_size) = if is_fullscreen {
			(
				(0.0, 0.0),
				(swap_extent.width() as f32, swap_extent.height() as f32),
			)
		} else {
			let c = layout.client_area();
			(
				(c.x() as f32, c.y() as f32),
				(c.width() as f32, c.height() as f32),
			)
		};
		
		Self {
			extent: swap_extent,
			frame_index,
			viewport_offset: content_offset,
			viewport_extent: content_size,
		}
	}
}
pub struct Glex<'dev> {
	vsync: PresentMode,
	cached_base: Option<DecorationLayer>,
	cached_base_size: (u32, u32),
	cached_base_title: String,
	csd_resources: CsdResources<'dev>,
	csd_pipelines: CsdPipelines,
	rendering: Rendering<'dev, TextSet>,
	gpu: GpuContext<'dev, VulkanBackend>,
	presentation: Presentation<'dev>,
	atlas_set_id: DescriptorSetId,
	pending_image_index: Option<u32>,
	passes: Vec<Box<dyn Pass<'dev> + 'dev>>,
}

impl<'dev> Glex<'dev> {
	pub fn new(
		ctx: &'dev VulkanContext,
		surface: &'dev Surface,
		window: &impl VulkanWindow,
	) -> Result<Self, Box<dyn std::error::Error>> {
		let presentation = Presentation::new(ctx, surface, window)?;
		let mut gpu = GpuContext::new(ctx)?;
		
		let mut rendering = Rendering::upload(&mut gpu, &presentation)?;
		let format = rendering.format();
		let csd_pipelines = CsdPipelines::load(&mut rendering.pipelines, format)?;
		let csd_resources = CsdResources::upload(ctx, &mut gpu, &rendering.sampler_layout)?;
		let atlas_set_id = gpu.executor_mut().register_descriptor_set(csd_resources.atlas.descriptor_set.handle());
		let vsync = presentation.present_mode();
		Ok(Self {
			cached_base: None,
			cached_base_size: (0, 0),
			cached_base_title: String::new(),
			presentation,
			gpu,
			rendering,
			csd_pipelines,
			csd_resources,
			atlas_set_id,
			vsync,
			pending_image_index: None,
			passes: Vec::new(),
		})
	}
	
	pub fn app(
		ctx: &'dev VulkanContext,
		surface: &'dev Surface,
		window: &impl VulkanWindow,
	) -> Result<Self, Box<dyn std::error::Error>> {
		Self::new(ctx, surface, window)
	}
	
	// ── Pass registration ────────────────────────────────────────────────────
	
	pub fn add(&mut self, pass: impl Pass<'dev> + 'dev) {
		self.passes.push(Box::new(pass));
	}
	
	// ── Frame lifecycle ──────────────────────────────────────────────────────
	
	pub fn begin_frame(
		&mut self,
		ctx: &'dev VulkanContext,
		window: &mut WaylandWindowImpl,
	) -> Result<Option<(FrameGraph, FrameInfo)>, Box<dyn std::error::Error>> {
		if !self.gpu.begin_frame()? {
			return Ok(None);
		}
		
		let completed = self.gpu.timeline_completed();
		self.presentation.gc_retired(completed);
		
		let is_configure = window.take_pending_configure();
		let win_extent  = window.extent();
		
		let swap_extent = self.presentation.extent();
		
		let resized =
			win_extent.width()  != swap_extent.width()
				|| win_extent.height() != swap_extent.height()
				|| is_configure
				|| self.presentation.needs_recreate();
		
		if resized {
			let retire_at = self.gpu.last_graphics_signal();
			self.presentation.apply_pending_recreate(ctx.physical(), retire_at)?;
			
			if is_configure || win_extent != swap_extent.into() {
				self.presentation.recreate(
					ctx.physical(),
					win_extent.width(),
					win_extent.height(),
					self.vsync,
					retire_at,
				)?;
				let new_extent = self.presentation.extent();
				window.rebuild_decoration_layout(
					new_extent.width(),
					new_extent.height(),
				);
			}
			
			self.cached_base = None;
			self.pending_image_index = None;
			return Ok(None);
		}
		let layout = window.decoration_layout();
		
		debug_assert_eq!(
			layout.size().width() as u32,
			swap_extent.width()
		);
		
		debug_assert_eq!(
			layout.size().height() as u32,
			swap_extent.height()
		);
		
		let is_fullscreen = window.is_fullscreen();
		let theme = window.theme();
		
		let acquire = self.gpu.acquire_semaphore();
		
		let image_index = match self.presentation.acquire(acquire)? {
			Some(idx) => idx,
			None => {
				return Ok(None);
			}
		};
		self.pending_image_index = Some(image_index);
		
		let frame_ex = Extent2D::new(swap_extent.width(), swap_extent.height());
		let frame_in = self.gpu.frame() as u32;
		
		let info = FrameInfo::from_layout(
			frame_ex,
			frame_in,
			is_fullscreen,
			layout,
		);
		
		let mut graph = FrameGraph::new();
		
		if !is_fullscreen {
			let current_size  = (layout.size().width() as u32, layout.size().height() as u32);
			let size_changed  = current_size != self.cached_base_size;
			let title_changed = window.title() != self.cached_base_title;
			
			if self.cached_base.is_none() || size_changed || title_changed {
				self.cached_base       = Some(StandardDecorations.build_base(layout, theme, window.title()));
				self.cached_base_size  = current_size;
				self.cached_base_title = window.title().to_owned();
			}
			
			record_csd_layer(
				&mut graph,
				&self.csd_resources,
				&self.csd_pipelines,
				self.cached_base.as_ref().unwrap(),
				layout,
				self.atlas_set_id,
			);
		}
		
		Ok(Some((graph, info)))
	}
	
	pub fn end_frame(
		&mut self,
		ctx: &'dev VulkanContext,
		window: &mut WaylandWindowImpl,
		mut graph: FrameGraph,
	) -> Result<(), Box<dyn std::error::Error>> {
		let image_index = self.pending_image_index.take().expect("end_frame called without begin_frame");
		let layout= window.decoration_layout();
		let state = window.decoration_state();
		let is_fullscreen = window.is_fullscreen();
		let theme= window.theme();
		let extent= self.presentation.extent();
		let render = self.presentation.render_semaphore(image_index);
		let image  = self.presentation.image(image_index);
		let ps = self.gpu.present_sync(render);
		let swap_img = SwapchainImage::from_raw(ctx.device(), image, extent.into());
		
		let clear = if is_fullscreen { theme.window_bg } else { Color::TRANSPARENT };
		let target   = RenderTarget {
			color_view:  self.presentation.image_view(image_index),
			extent: extent.into(),
			clear_color: clear.to_array(),
		};
		
		if !is_fullscreen {
			let button_layer = StandardDecorations.build_buttons(layout, state, theme);
			record_csd_layer(
				&mut graph,
				&self.csd_resources,
				&self.csd_pipelines,
				&button_layer,
				layout,
				self.atlas_set_id,
			);
		}
		
		let signal_val = self.gpu.executor_mut().execute(graph, swap_img, target, &self.rendering.pipelines, Some(ps))?;
		self.gpu.record_signal(signal_val);
		let suboptimal = self.presentation.present(image_index, ps.signal_render_finished)?;
		
		if suboptimal {
			self.presentation.schedule_resize(
				extent.width(),
				extent.height(),
			);
		}
		let bump_val = self.gpu.bump_after_present()?;
		self.gpu.record_signal(bump_val);
		self.gpu.end_frame();
		Ok(())
	}
	
	// ── Event loop ───────────────────────────────────────────────────────────
	
	pub fn run(
		&mut self,
		ctx: &'dev VulkanContext,
		window: &mut WaylandWindowImpl,
	) -> Result<(), Box<dyn std::error::Error>> {
		loop {
			let (cf, _events) = window.pump();
			if matches!(cf, ControlFlow::Exit) { break; }
			
			let Some((mut graph, info)) = self.begin_frame(ctx, window)? else {
				continue;
			};
			
			for pass in &mut self.passes {
				pass.update(info.frame_index);
			}
			
			for pass in &mut self.passes {
				pass.record(&mut graph, &info);
			}
			
			self.end_frame(ctx, window, graph)?;
		}
		ctx.device().wait_idle()?;
		Ok(())
	}
	
	// ── Accessors ────────────────────────────────────────────────────────────
	
	pub fn pipelines(&mut self) -> &mut PipelineManager<'dev, VulkanBackend> {
		&mut self.rendering.pipelines
	}
	
	pub fn format(&self) -> <VulkanBackend as crate::core::Backend>::Format {
		self.rendering.format()
	}

	pub fn device(&self) -> &VulkanDevice {
		self.gpu.device()
	}

	pub fn gpu_mut(&mut self) -> &mut GpuContext<'dev, VulkanBackend> {
		&mut self.gpu
	}

	pub fn register_descriptor_set(&mut self, handle: <VulkanBackend as crate::core::Backend>::DescriptorSet) -> DescriptorSetId {
		self.gpu.executor_mut().register_descriptor_set(handle)
	}
}