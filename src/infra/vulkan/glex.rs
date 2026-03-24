use crate::core::PresentSync;
use super::VulkanDevice;
use super::context::{GpuContext, Presentation, Rendering, VulkanContext};
use crate::core::FrameGraph;
use crate::core::PipelineManager;
use crate::core::RenderTarget;
use crate::core::SwapchainImage;
use crate::domain::DescriptorSetId;
use crate::infra::VulkanBackend;
use crate::infra::platform::Surface;
use crate::infra::platform::VulkanWindow;
use crate::infra::platform::WaylandWindowImpl;
use crate::infra::vulkan::context::PresentMode;
use crate::renderer::{CsdPass, TextSet};
use glex_platform::platform::{ControlFlow, Extent2D, Window};
use tracing::{debug, error, info, instrument, trace, warn};
use glex_platform::csd::Color;
// =============================================================================
// Pass trait
// =============================================================================

pub trait Pass<'dev> {
    fn update(&mut self, _frame_index: u32) {}
    fn record(&self, graph: &mut FrameGraph, info: &FrameInfo);
    fn finalize(&mut self, _t: u64) {}
}

// =============================================================================
// FrameInfo
// =============================================================================

pub struct FrameInfo {
    pub extent:          Extent2D,
    pub frame_index:     u32,
    pub viewport_offset: (f32, f32),
    pub viewport_extent: (f32, f32),
}

impl FrameInfo {
    pub fn from_layout(
        swap_extent:   Extent2D,
        frame_index:   u32,
        is_fullscreen: bool,
        layout:        &glex_platform::csd::layout::DecorationLayout,
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
            extent:          swap_extent,
            frame_index,
            viewport_offset: content_offset,
            viewport_extent: content_size,
        }
    }
}


// =============================================================================
// Glex
// =============================================================================

pub struct Glex<'dev> {
    vsync:        PresentMode,
    rendering:    Rendering<'dev, TextSet>,
    gpu:          GpuContext<'dev, VulkanBackend>,
    presentation: Presentation<'dev>,
    csd:          Option<CsdPass<'dev>>,
    passes:       Vec<Box<dyn Pass<'dev> + 'dev>>,
}

impl<'dev> Glex<'dev> {
    fn new(
        ctx:     &'dev VulkanContext,
        surface: &'dev Surface,
        window:  &impl VulkanWindow,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Initializing Glex context");
        debug!("Creating Presentation");
        let presentation = Presentation::new(ctx, surface, window, 3)?;
        debug!("Creating GpuContext");
        let mut gpu = GpuContext::new(ctx)?;
        debug!("Uploading Rendering resources");
        let rendering = Rendering::upload(&mut gpu, &presentation)?;
        debug!("Glex::new complete");
        Ok(Self {
            vsync: presentation.present_mode(),
            presentation,
            gpu,
            rendering,
            csd:    None,
            passes: Vec::new(),
        })
    }
    
    pub fn app(
        ctx:     &'dev VulkanContext,
        surface: &'dev Surface,
        window:  &impl VulkanWindow,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        debug!("Entering Glex::app initialization");
        let mut glex = Self::new(ctx, surface, window)?;
        debug!("Setting up CsdPass");
        let csd = CsdPass::setup(ctx, &mut glex)?;
        glex.csd = Some(csd);
        debug!("Glex::app initialization finished");
        Ok(glex)
    }
    
    // -------------------------------------------------------------------------
    // Pass registration
    // -------------------------------------------------------------------------
    
    pub fn add(&mut self, pass: impl Pass<'dev> + 'dev) {
        debug!("Registering new Pass, total passes: {}", self.passes.len() + 1);
        self.passes.push(Box::new(pass));
    }
    
    // -------------------------------------------------------------------------
    // CSD accessors
    // -------------------------------------------------------------------------
    
    #[inline]
    fn csd(&self) -> &CsdPass<'dev> {
        self.csd.as_ref().expect("CsdPass not initialized")
    }
    
    #[inline]
    fn csd_mut(&mut self) -> &mut CsdPass<'dev> {
        self.csd.as_mut().expect("CsdPass not initialized")
    }
    
    // -------------------------------------------------------------------------
    // Run loop
    // -------------------------------------------------------------------------
    #[instrument(skip_all, name = "Glex::run")]
    pub fn run(
        &mut self,
        ctx:    &'dev VulkanContext,
        window: &mut WaylandWindowImpl,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting main rendering loop");
        
        loop {
            let (cf, _) = window.pump();
            if matches!(cf, ControlFlow::Exit) {
                info!("Glex control flow exit, shutting down");
                break;
            }
            
            let frame_id = self.gpu.frame();
            trace!(frame_id, "FRAME_BEGIN");
            
            // ── configure / resize ──────────────────────────────
            let is_configure = window.take_pending_configure();
            let win_ext  = window.extent();
            let swap_ext = self.presentation.extent();
            let mismatch = win_ext.width()  != swap_ext.width()
                || win_ext.height() != swap_ext.height();
            
            if mismatch {
                self.presentation.schedule_resize(win_ext.width(), win_ext.height());
            }
            
            if is_configure || self.presentation.needs_recreate() {
                let retire_at = self.gpu.last_graphics_signal();
                let completed = self.gpu.timeline_completed();
                
                debug!(
                      frame_id,
                      is_configure,
                      mismatch,
                      retire_at,
                      completed,
                      win_w  = win_ext.width(),
                      win_h  = win_ext.height(),
                      swap_w = swap_ext.width(),
                      swap_h = swap_ext.height(),
                      "Resize path entered"
                  );
                
                self.presentation.gc_retired(completed);
                
                let t0 = std::time::Instant::now();
                self.presentation.apply_pending_recreate(ctx.physical(), retire_at)?;
                let ms = t0.elapsed().as_millis();
                if ms > 2 {
                    warn!(frame_id, ms, "SLOW apply_pending_recreate");
                } else {
                    trace!(frame_id, ms, retire_at, "apply_pending_recreate done");
                }
                
                if mismatch || is_configure {
                    let cur = self.presentation.extent();
                    debug!(
                          frame_id,
                          width  = cur.width(),
                          height = cur.height(),
                          mismatch,
                          is_configure,
                          "Rebuilding decoration layout"
                      );
                    window.rebuild_decoration_layout(cur.width(), cur.height());
                }
                continue;
            }
            
            // ── timeline slot check ─────────────────────────────
            let t0 = std::time::Instant::now();
            if !self.gpu.begin_frame()? {
                trace!(frame_id, "Slot not ready, skipping");
                continue;
            }
            let ms = t0.elapsed().as_millis();
            if ms > 2 {
                warn!(frame_id, ms, "SLOW begin_frame (timeline semaphore wait)");
            }
            
            trace!(
                  frame_id,
                  gpu_frame          = self.gpu.frame(),
                  timeline_completed = self.gpu.timeline_completed(),
                  last_signal        = self.gpu.last_graphics_signal(),
                  "GPU state snapshot"
              );
            
            self.presentation.gc_retired(self.gpu.timeline_completed());
           
            // ── acquire (last step before submit) ───────────────
            let acq = match self.presentation.acquire()? {
                Some(r) => {
                    if r.suboptimal {
                        trace!(frame_id, "Acquire suboptimal — scheduling recreate");
                        self.presentation.schedule_resize(win_ext.width(), win_ext.height());
                    }
                    trace!(frame_id, image_index = r.image_index, "Acquire success");
                    r
                }
                None => {
                    warn!(frame_id, "Acquire failed (out of date) — scheduling recreate");
                    self.presentation.schedule_resize(win_ext.width(), win_ext.height());
                    continue;
                }
            };
            
            // ── layout / decoration state ───────────────────────
            let swap_ext = self.presentation.extent();
            let layout   = window.decoration_layout();
            let is_fs    = window.is_fullscreen()
                || layout.title().bar().height() == 0.0;
            let theme    = window.theme();
            let title    = window.title();
            let state    = window.decoration_state();
            
            trace!(
                  frame_id,
                  layout_size   = ?layout.size(),
                  swap_extent   = ?swap_ext,
                  is_fullscreen = is_fs,
                  "Layout vs swapchain"
              );
            
            self.csd_mut().begin_frame(layout, is_fs, title, theme, state);
            
            let info = FrameInfo::from_layout(
                Extent2D::new(swap_ext.width(), swap_ext.height()),
                self.gpu.frame() as u32,
                is_fs,
                layout,
            );
            // ── update + record ─────────────────────────────────────
            for pass in self.passes.iter_mut() {
                pass.update(self.gpu.frame() as u32);
            }
            
            let mut graph = FrameGraph::new();
            // CSD background FIRST — rounded window body behind user content
            self.csd().record_background(&mut graph, &info);
            
            
            // user passes draw inside the content area
            for pass in self.passes.iter() {
                pass.record(&mut graph, &info);
            }
            // CSD foreground LAST — buttons on top of everything
            self.csd().record_foreground(&mut graph, &info);
            
            
            // ── submit ──────────────────────────────────────────
            let extent  = self.presentation.extent();
            let image   = self.presentation.image(acq.image_index);
            let ps = PresentSync {
                wait_acquire:           acq.acquire_semaphore,
                signal_render_finished: acq.render_semaphore,
            };
            let swap_img = SwapchainImage::from_raw(
                ctx.device(), image, extent.into(),
            );
            let target = RenderTarget {
                color_view:  self.presentation.image_view(acq.image_index),
                extent:      extent.into(),
                clear_color: theme.window_bg.screen(Color::TRANSPARENT).to_array()
            };
            
            trace!(frame_id, "Executing frame graph");
            
            let signal_val = self.gpu.executor_mut().execute(
                graph, swap_img, target,
                &self.rendering.pipelines, Some(ps),
            ).map_err(|e| {
                error!(frame_id, error = ?e, "Graph execution failed");
                e
            })?;
            
            // ── record signal ───────────────────────────────────
            self.gpu.record_signal(signal_val);
            debug!(frame_id, signal_val, "Graph submitted");
            
            debug_assert!(
                self.gpu.timeline_completed() <= signal_val,
                "Timeline invariant violated: completed={} signal_val={}",
                self.gpu.timeline_completed(), signal_val
            );
            // ── present ─────────────────────────────────────────
            trace!(frame_id, image_index = acq.image_index, "Present submission");
            
            let present_stale = self.presentation.present(
                acq.image_index, acq.render_semaphore,
            )?;
            if present_stale {
                trace!(frame_id, "Present reports stale surface — scheduling recreate");
                self.presentation.schedule_resize(win_ext.width(), win_ext.height());
            }
          
            // ── advance ─────────────────────────────────────────
            trace!(
                  frame_id,
                  cpu_frame     = self.gpu.frame(),
                  gpu_completed = self.gpu.timeline_completed(),
                  in_flight     = self.gpu.frame() as i64 - self.gpu.timeline_completed() as i64,
                  "Frame overlap"
              );
            
            self.gpu.tick_allocator();
            self.gpu.end_frame();
            self.presentation.tick_condemned();
            
            trace!(frame_id, "END_FRAME_DONE");
        }
        
        debug!("Waiting for device idle before exit");
        ctx.device().wait_idle()?;
        info!("Glex finished cleanly");
        Ok(())
    }
    
    // -------------------------------------------------------------------------
    // Accessors
    // -------------------------------------------------------------------------
    
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
    
    pub fn sampler_layout(
        &self,
    ) -> &crate::core::DescriptorLayout<'dev, VulkanBackend, TextSet> {
        &self.rendering.sampler_layout
    }
    
    pub fn register_descriptor_set(
        &mut self,
        handle: <VulkanBackend as crate::core::Backend>::DescriptorSet,
    ) -> DescriptorSetId {
        self.gpu.executor_mut().register_descriptor_set(handle)
    }
}

// =============================================================================
// Drop
// =============================================================================

impl<'dev> Drop for Glex<'dev> {
    fn drop(&mut self) {
        let completed = self.gpu.timeline_completed();
        debug!(completed_timeline = completed, "Finalizing passes");
        
        for pass in &mut self.passes {
            pass.finalize(completed);
        }
        if let Some(csd) = &mut self.csd {
            csd.finalize(completed);
        }
        
        self.passes.clear();
        self.csd = None;
        
        self.gpu.tick_allocator();
        debug!("Glex Drop complete");
    }
}