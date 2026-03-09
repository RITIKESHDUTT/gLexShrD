use crate::infra::platform::Surface;
use crate::core::types::Format;
use crate::core::BinarySemaphore;
use crate::core::ImageView;
use ash::vk;
use crate::infra::vulkan::backend::VulkanDevice;
use super::instance::VulkanInstance;
use super::physical_device::PhysicalDevice;
use crate::infra::vulkan::backend::VulkanBackend;

pub struct Swapchain<'dev,> {
	device: &'dev VulkanDevice,
	loader: ash::khr::swapchain::Device,
	swapchain: vk::SwapchainKHR,
	images: Vec<vk::Image>,
	image_views: Vec<ImageView<'dev, VulkanBackend>>,
	render_semaphores: Vec<BinarySemaphore<'dev, VulkanBackend>>,
	format: vk::SurfaceFormatKHR,
	extent: vk::Extent2D,
}

impl<'dev> Swapchain<'dev, > {
	pub(crate) fn required_device_extensions() -> Vec<*const i8> {
		vec![ash::khr::swapchain::NAME.as_ptr()]
	}
	
	pub(crate) fn new(
		instance: &VulkanInstance,
		device: &'dev VulkanDevice,
		physical: &PhysicalDevice,
		surface: &Surface,
		width: u32,
		height: u32,
	) -> Result<Self, vk::Result> {
		let caps = unsafe {
			surface.capabilities(physical.handle())?
		};
		let composite_alpha = if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED) {
			vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED
		} else if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED) {
			vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED
		} else {
			vk::CompositeAlphaFlagsKHR::OPAQUE
		};
		let formats = unsafe {
			surface.formats(physical.handle())?
		};

		let format = formats
			.iter()
			.find(|f| {
				f.format == vk::Format::B8G8R8A8_SRGB
					&& f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
			})
			.or(formats.first())
			.copied()
			.ok_or(vk::Result::ERROR_FORMAT_NOT_SUPPORTED)?;

		let extent = vk::Extent2D {
			width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
			height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
		};

		let image_count = (caps.min_image_count + 1).min(
			if caps.max_image_count == 0 { u32::MAX } else { caps.max_image_count }
		);

		let create_info = vk::SwapchainCreateInfoKHR::default()
			.surface(surface.handle())
			.min_image_count(image_count)
			.image_format(format.format)
			.image_color_space(format.color_space)
			.image_extent(extent)
			.image_array_layers(1)
			.image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
			.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
			.pre_transform(caps.current_transform)
			.composite_alpha(composite_alpha)
			.present_mode(vk::PresentModeKHR::FIFO)
			.clipped(true);
		let device_handle = &device.inner;
		let loader = ash::khr::swapchain::Device::new(
			instance.instance(),
			device_handle,
		);

		let swapchain = unsafe {
			loader.create_swapchain(&create_info, None)?
		};

		let images = unsafe {
			loader.get_swapchain_images(swapchain)?
		};

		let image_views: Result<Vec<_>, _> = images
			.iter()
			.map(|&image| {
				ImageView::color_2d(device, image, Format(format.format.as_raw()))
			})
			.collect();

		let image_views = image_views?;
		let render_semaphores: Result<Vec<_>, _> = (0..images.len())
			.map(|_| BinarySemaphore::new(device))
			.collect();
		let render_semaphores = render_semaphores?;
		Ok(Self {
			device,
			loader,
			swapchain,
			images,
			image_views,
			render_semaphores,
			format,
			extent,
		})
	}
	
	pub(crate) fn acquire_next(
		&self,
		semaphore: vk::Semaphore,
	) -> Result<(u32, bool), vk::Result> {
		let (image_index, suboptimal) = unsafe {
			self.loader.acquire_next_image(
				self.swapchain,
				u64::MAX,
				semaphore,
				vk::Fence::null(),
			)?
		};
		Ok((image_index, suboptimal))
	}
	
	/// Recreate the swapchain after resize or suboptimal present.
	///
	/// Reuses the old swapchain handle for a smoother transition.
	/// Caller must ensure no frames are in flight (call device_wait_idle first).
	pub(crate) fn recreate(
		&mut self,
		physical: &PhysicalDevice,
		surface: &Surface,
		width: u32,
		height: u32,
	) -> Result<(), vk::Result> {
		self.recreate_with_present_mode(
			physical,
			surface,
			width,
			height,
			vk::PresentModeKHR::FIFO,
		)
	}
	
	pub fn recreate_with_present_mode(
		&mut self,
		physical: &PhysicalDevice,
		surface: &Surface,
		width: u32,
		height: u32,
		present_mode: vk::PresentModeKHR,
	)-> Result<(), vk::Result> {
		let supported = unsafe { surface.present_modes(physical.handle())? };
		let present_mode = if supported.contains(&present_mode) {
			present_mode
		} else if supported.contains(&vk::PresentModeKHR::MAILBOX) {
			vk::PresentModeKHR::MAILBOX
		} else {
			vk::PresentModeKHR::FIFO // guaranteed by spec
		};

		let caps = unsafe {
			surface.capabilities(physical.handle())?
		};
		let composite_alpha = if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED) {
			vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED
		} else if caps.supported_composite_alpha.contains(vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED) {
			vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED
		} else {
			vk::CompositeAlphaFlagsKHR::OPAQUE
		};
		let extent = vk::Extent2D {
			width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
			height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
		};
		
		let image_count = (caps.min_image_count + 1).min(
			if caps.max_image_count == 0 { u32::MAX } else { caps.max_image_count }
		);
		
		let old_swapchain = self.swapchain;
	
		
		let create_info = vk::SwapchainCreateInfoKHR::default()
			.surface(surface.handle())
			.min_image_count(image_count)
			.image_format(self.format.format)
			.image_color_space(self.format.color_space)
			.image_extent(extent)
			.image_array_layers(1)
			.image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
			.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
			.pre_transform(caps.current_transform)
			.composite_alpha(composite_alpha)
			.present_mode(present_mode)
			.clipped(true)
			.old_swapchain(old_swapchain);
		
		let new_swapchain = unsafe {
			self.loader.create_swapchain(&create_info, None)?
		};
		
		let images = unsafe {
			self.loader.get_swapchain_images(new_swapchain)?
		};
		
		let image_views: Result<Vec<_>, _> = images
			.iter()
			.map(|&image| {
				ImageView::color_2d(self.device, image, Format(self.format.format.as_raw()))
			})
			.collect();
		let image_views = image_views?;
		let new_render_semaphores: Result<Vec<_>, _> = (0..images.len())
			.map(|_| BinarySemaphore::new(self.device))
			.collect();
		let new_render_semaphores = new_render_semaphores?;
		  // Destroy old resources — order matters:
		// 1. render_semaphores drop (device_wait_idle already called by caller)
		// 2. image_views drop /
		//  3. old swapchain destroyed
		self.render_semaphores.clear();
  self.image_views.clear();
  unsafe {
      self.loader.destroy_swapchain(old_swapchain, None);
  }

	
		// Replace with new
		self.swapchain = new_swapchain;
		self.images = images;
		self.image_views = image_views;
		self.render_semaphores = new_render_semaphores;
		self.extent = extent;
		Ok(())
	}
	/// Render semaphore for the given swapchain image index.
	/// Indexed by image — not frame slot — because the presentation
	/// engine holds the semaphore until that image is re-acquired.
	pub(crate) fn render_semaphore(&self, image_index: u32) -> vk::Semaphore {
		self.render_semaphores[image_index as usize].handle()
	}
	
	pub(crate) fn format(&self) -> vk::Format {
		self.format.format
	}
	
	pub(crate) fn extent(&self) -> vk::Extent2D {
		self.extent
	}
	
	pub(crate) fn image(&self, index: u32) -> vk::Image {
		self.images[index as usize]
	}

	/// Returns a reference to the typed ImageView.
	pub(crate) fn image_view(&self, index: u32) -> &ImageView<'_, VulkanBackend> {
		&self.image_views[index as usize]
	}
	
	pub(crate) fn _image_count(&self) -> u32 {
		self.images.len() as u32
	}
	pub(crate) fn present_raw(
		&self,
		queue: vk::Queue,
		index: u32,
		wait_semaphore: vk::Semaphore,
	) -> Result<bool, vk::Result> {
		let swapchains = [self.swapchain];
		let indices = [index];
		let wait = [wait_semaphore];
		
		let present_info = vk::PresentInfoKHR::default()
			.wait_semaphores(&wait)
			.swapchains(&swapchains)
			.image_indices(&indices);
		
		unsafe {
			match self.loader.queue_present(queue, &present_info) {
				Ok(suboptimal) => Ok(suboptimal),
				Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(true), // Signal a recreate is needed
				Err(e) => Err(e),
			}
		}
	}
}

impl Drop for Swapchain<'_> {
	fn drop(&mut self) {
		// image_views are dropped automatically (Vec<ImageView> → each ImageView::drop)
		// Images are NOT destroyed — swapchain owns them
		unsafe {
			// Clear views first, before destroying the swapchain
			self.image_views.clear();
			self.loader.destroy_swapchain(self.swapchain, None);
		}
	}
}