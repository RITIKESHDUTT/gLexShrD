use crate::domain::DescriptorSetId;
use crate::core::FrameGraph;
use crate::core::PresentSync;
use crate::core::RenderTarget;
use crate::core::SwapchainImage;
use crate::infra::platform::Surface;
use crate::infra::platform::VulkanWindow;
use crate::infra::platform::WaylandPlatform;
use crate::infra::platform::WaylandWindowImpl;
use glex_platform::csd::build::DecorationBuilder;
use glex_platform::csd::build::StandardDecorations;
use glex_platform::csd::{Color, CsdTheme};
use glex_platform::platform::Window;
use glex_platform::platform::{ControlFlow, WindowConfig};
use crate::renderer::build_csd_commands;

mod backend;
use crate::core::DescriptorLayout;
use crate::renderer::TextSet;
pub use backend::*;
mod context;
use crate::core::PipelineManager;
use crate::lin_al::Vec2;
use crate::renderer::CsdPipelines;
use crate::renderer::CsdResources;
pub use context::{GpuContext, Presentation, Rendering, VulkanContext};
use glex_platform::platform::Platform;

// Dropped first → last (declaration order = drop order)
pub struct Glex<'dev> {
	theme: CsdTheme,
	csd_resources: CsdResources<'dev>,
	csd_pipelines: CsdPipelines,
	pipelines: PipelineManager<'dev, VulkanBackend>,
	gpu: GpuContext<'dev, VulkanBackend>,
	presentation: Presentation<'dev>,
	atlas_set_id: DescriptorSetId,
}


impl<'dev> Glex<'dev> {
	pub fn new(
		ctx: &'dev VulkanContext,
		surface: Surface,
		window: &impl VulkanWindow
	) -> Result<Self, Box<dyn std::error::Error>> {
		let presentation = Presentation::new(ctx, surface, window)?;
		let mut gpu = GpuContext::new(ctx)?;
		let mut pipelines = PipelineManager::new(ctx.device());
		
		let format = presentation.format();
		let csd_pipelines = CsdPipelines::load(&mut pipelines, format)?;
		
		let sampler_layout =
			DescriptorLayout::<VulkanBackend, TextSet>::new(ctx.device())?;
		
		let csd_resources = CsdResources::upload(ctx, &mut gpu, sampler_layout)?;
		let atlas_set_id = gpu.executor_mut()
							  .register_descriptor_set(csd_resources.atlas.descriptor_set.handle());
		
		
		Ok(Self {
			presentation,
			gpu,
			pipelines,
			csd_pipelines,
			csd_resources,
			theme: CsdTheme::default(),
			atlas_set_id,
		})
	}
	
	pub fn frame(&mut self, ctx: &'dev VulkanContext, window: &WaylandWindowImpl, title:&str) -> Result<(), Box<dyn std::error::Error>> {
		if !self.gpu.begin_frame()? {
			return Ok(());
		}
		
		// Check if swapchain needs recreation
		let extent = window.extent();
		let current = self.presentation.extent();
		if extent.width() != current.width() || extent.height() != current.height() {
			self.gpu.drain()?;
			self.presentation.recreate(ctx.physical(), extent.width(), extent.height())?;
		}
		
		
		let acquire = self.gpu.acquire_semaphore();
		let render = self.gpu.render_semaphore();
		self.gpu.set_present_sync(PresentSync {
			wait_acquire: acquire,
			signal_render_finished: render,
		});
		
		
		let (image_index, _) = self.presentation.acquire(acquire)?;
		
		let swap_img = SwapchainImage::from_raw(
			ctx.device(),
			self.presentation.swapchain.image(image_index),
			self.presentation.extent(),
		);
		
		let target = RenderTarget {
			color_view: self.presentation.swapchain.image_view(image_index),
			extent: self.presentation.extent(),
			clear_color: Color::transparent().to_array(),
		};
		
		let layout = window.decoration_layout();
		let state = window.decoration_state();
		let draw = StandardDecorations.build_decorations(layout, state, &self.theme, title);
		let screen_size = Vec2::new(extent.width() as f32, extent.height() as f32);
		let mut graph = FrameGraph::new();
		let resources = &self.csd_resources;
		let csd = &self.csd_pipelines;
		
		build_csd_commands(&mut graph, resources, csd, &draw, screen_size, self.atlas_set_id);
		
		let signal_val = self.gpu.executor_mut()
							 .execute(graph, swap_img, target, &self.pipelines)?;
		self.gpu.record_signal(signal_val);
		
		self.presentation.present(image_index, render)?;
		self.gpu.bump_after_present()?;
		self.gpu.end_frame();
		
		Ok(())
	}
	pub fn set_theme(&mut self, theme: CsdTheme) {
		self.theme = theme;
	}
	pub fn app(title: &str, width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
		let mut platform = WaylandPlatform::new()
			.map_err(|e| format!("Failed to init Wayland: {e}"))?;
		let mut window = platform.create_window(WindowConfig::new(title, width, height))?;
		let (ctx, surface) = VulkanContext::new::<WaylandWindowImpl>(&window)?;
		let mut glex = Glex::new(&ctx, surface, &window)?;
		
		glex.set_theme(CsdTheme {
			window_bg: Color::ABSOLUTE_BLACK,
			title_bar_bg: Color::PITCH_BLACK,
			..CsdTheme::default()
		});
		
		loop {
			match window.pump(|_| {}) {
				ControlFlow::Exit => break,
				_ => glex.frame(&ctx, &window, title)?,
			}
		}
		ctx.device.wait_idle().unwrap(); //explicit drop impl
		Ok(())
	}
}