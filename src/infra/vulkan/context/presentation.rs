use crate::core::type_state_queue::sealed::QueueHandle;
use crate::core::types::Extent2D;
use crate::core::types::Format;
use crate::core::types::MemoryPropertyFlags;
use crate::core::types::MemoryRequirements;
use crate::core::Backend;
use crate::core::BinarySemaphore;
use crate::core::ImageView;
use crate::infra::platform::Surface;
use crate::infra::vulkan::backend::{PhysicalDevice, RetiredSwapchainResources, Swapchain, VulkanBackend,
									VulkanInstance};
use crate::infra::VulkanContext;
use glex_platform::platform::Window;

// ── AcquireResult ────────────────────────────────────────────────────────────

pub struct AcquireResult {
	pub image_index: u32,
	pub acquire_semaphore: ash::vk::Semaphore,
	pub render_semaphore: ash::vk::Semaphore,
	pub suboptimal: bool,
}

// ── CondemnedSemaphores ──────────────────────────────────────────────────────

struct CondemnedSemaphores<'dev> {
	semaphores: Vec<BinarySemaphore<'dev, VulkanBackend>>,
	frames_remaining: u32,
}

// ── RetiredEntry ─────────────────────────────────────────────────────────────

struct RetiredEntry<'dev> {
	resources: RetiredSwapchainResources<'dev>,
	retire_value: u64,
}

// ── Presentation ─────────────────────────────────────────────────────────────

pub struct Presentation<'dev> {
	swapchain: Swapchain<'dev>,
	surface: &'dev Surface,
	queue: ash::vk::Queue,
	present_mode: PresentMode,
	pending_resize: Option<(u32, u32)>,
	retired: Vec<RetiredEntry<'dev>>,
	acquire_semaphores: Vec<BinarySemaphore<'dev, VulkanBackend>>,
	acquire_index: usize,
	condemned_semaphores: Vec<CondemnedSemaphores<'dev>>,
}

impl<'dev> Presentation<'dev> {
	pub fn new(
		ctx: &'dev VulkanContext,
		surface: &'dev Surface,
		window: &impl Window,
		frames_in_flight: usize,
	) -> Result<Self, <VulkanBackend as Backend>::Error> {
		let extent = window.extent();
		let swapchain = Swapchain::new(
			&ctx.instance, &ctx.device, &ctx.physical,
			&surface, extent.width(), extent.height(),
		)?;
		
		let present_queue = ctx
			.queues()
			.present()
			.expect("device has no present queue")
			.raw();
		
		let acquire_semaphores: Vec<_> = (0..frames_in_flight)
			.map(|_| BinarySemaphore::new(ctx.device()))
			.collect::<Result<_, _>>()?;
		
		Ok(Self {
			swapchain,
			surface,
			queue: present_queue,
			present_mode: PresentMode::Vsync,
			pending_resize: None,
			retired: Vec::new(),
			acquire_semaphores,
			acquire_index: 0,
			condemned_semaphores: Vec::new(),
		})
	}
	
	// ── Acquire ──────────────────────────────────────────────────────────────
	
	pub fn acquire(&mut self) -> Result<Option<AcquireResult>, ash::vk::Result> {
		let sem = self.acquire_semaphores[self.acquire_index].handle();
		self.acquire_index = (self.acquire_index + 1) % self.acquire_semaphores.len();
		
		match self.swapchain.acquire_next(sem) {
			Ok((idx, suboptimal)) => {
				Ok(Some(AcquireResult {
					image_index: idx,
					acquire_semaphore: sem,
					render_semaphore: self.swapchain.render_semaphore(idx),
					suboptimal,
				}))
			}
			Err(ash::vk::Result::ERROR_OUT_OF_DATE_KHR) => {
				Ok(None)
			}
			Err(e) => Err(e),
		}
	}
	
	// ── Recreate ─────────────────────────────────────────────────────────────
	
	pub fn recreate(
		&mut self,
		physical: &PhysicalDevice,
		width: u32,
		height: u32,
		present_mode: PresentMode,
		retire_at: u64,
	) -> Result<(), ash::vk::Result> {
		let vk_mode = match present_mode {
			PresentMode::Vsync => ash::vk::PresentModeKHR::FIFO,
			PresentMode::Mailbox => ash::vk::PresentModeKHR::MAILBOX,
		};
		let (old_resources, old_sems) = self.swapchain.recreate_with_present_mode(
			physical, &self.surface, width, height, vk_mode,
		)?;
		
		self.retired.push(RetiredEntry {
			resources: old_resources,
			retire_value: retire_at,
		});
		
		self.condemned_semaphores.push(CondemnedSemaphores {
			semaphores: old_sems,
			frames_remaining: 4,
		});
		
		Ok(())
	}
	
	// ── Present ──────────────────────────────────────────────────────────────
	
	pub fn present(
		&self,
		image_index: u32,
		wait_semaphore: <VulkanBackend as Backend>::Semaphore,
	) -> Result<bool, <VulkanBackend as Backend>::Error> {
		self.swapchain.present_raw(self.queue, image_index, wait_semaphore)
	}
	
	// ── Resize scheduling ────────────────────────────────────────────────────
	
	pub fn schedule_resize(&mut self, w: u32, h: u32) {
		self.pending_resize = Some((w, h));
	}
	
	pub fn schedule_present_mode_change(&mut self) {
		if self.pending_resize.is_none() {
			self.pending_resize = Some((self.extent().width(), self.extent().height()));
		}
	}
	
	pub fn needs_recreate(&self) -> bool {
		self.pending_resize.is_some()
	}
	
	pub fn apply_pending_recreate(
		&mut self,
		physical: &PhysicalDevice,
		retire_at: u64,
	) -> Result<(), ash::vk::Result> {
		if let Some((w, h)) = self.pending_resize.take() {
			self.recreate(physical, w, h, self.present_mode, retire_at)?;
		}
		Ok(())
	}
	
	// ── GC ───────────────────────────────────────────────────────────────────
	
	pub fn gc_retired(&mut self, completed_timeline: u64) {
		self.retired.retain(|entry| entry.retire_value > completed_timeline);
	}
	
	pub fn tick_condemned(&mut self) {
		for entry in &mut self.condemned_semaphores {
			entry.frames_remaining = entry.frames_remaining.saturating_sub(1);
		}
		self.condemned_semaphores.retain(|e| e.frames_remaining > 0);
	}
	
	// ── Accessors ────────────────────────────────────────────────────────────
	
	pub fn set_present_mode(&mut self, mode: PresentMode) {
		self.present_mode = mode;
	}
	
	pub fn present_mode(&self) -> PresentMode {
		self.present_mode
	}
	
	pub fn extent(&self) -> Extent2D {
		let e = self.swapchain.extent();
		Extent2D::new(e.width, e.height)
	}
	
	pub fn image(&self, image_index: u32) -> ash::vk::Image {
		self.swapchain.image(image_index)
	}
	
	pub fn image_view(&self, image_index: u32) -> &ImageView<'_, VulkanBackend> {
		self.swapchain.image_view(image_index)
	}
	
	pub fn format(&self) -> Format {
		Format::from(Format(self.swapchain.format().as_raw()))
	}
}

// ── MemoryIndices ────────────────────────────────────────────────────────────

#[derive(Copy, Clone)]
pub struct MemoryIndices {
	pub staging: u32,
	pub device_local: u32,
	pub transcript: u32,
}

impl MemoryIndices {
	pub fn find(
		physical: &PhysicalDevice,
		instance: &VulkanInstance,
		req: MemoryRequirements,
	) -> Self {
		let staging = physical.find_memory_type(
			instance,
			req,
			MemoryPropertyFlags::HOST_VISIBLE | MemoryPropertyFlags::HOST_COHERENT,
		).expect("No HOST_VISIBLE|HOST_COHERENT memory");
		
		let device_local = physical.find_memory_type(
			instance,
			req,
			MemoryPropertyFlags::DEVICE_LOCAL,
		).expect("No DEVICE_LOCAL memory");
		
		let transcript = physical.find_memory_type(
			instance,
			req,
			MemoryPropertyFlags::HOST_VISIBLE,
		);
		
		Self {
			staging,
			device_local,
			transcript: transcript.expect("REASON"),
		}
	}
}

// ── PresentMode ──────────────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum PresentMode {
	Vsync,
	Mailbox,
}