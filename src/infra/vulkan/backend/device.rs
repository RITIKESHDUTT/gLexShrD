use super::QueueLane;
use super::{PhysicalDevice, VulkanInstance};
use crate::core::Backend;
use crate::infra::vulkan::backend::{VulkanBackend as vb, VulkanDevice};
use crate::infra::vulkan::QueueDiscovery;
use ash::vk;

pub struct LogicalDevice;

impl LogicalDevice {
	/// Presentation path — pass `Swapchain::required_device_extensions()`.
	pub fn with_extensions(
		instance: &VulkanInstance,
		physical: &PhysicalDevice,
		queues: &QueueDiscovery,
		extensions: &[*const i8],
	)-> Result<(VulkanDevice, QueueLane), <vb as Backend>::Error> {
		// Create the device handle
		let device = Self::create_device(instance, physical, queues, extensions)?;
		// This is where the conversion from Discovery (u32) to Lane (Queue<C>) happens
		let queue_lane = QueueLane::new(&device, queues);
		Ok((device, queue_lane))
	}

	/// Private: creates device + extracts raw queue. Callers wrap into typed Queue.
	fn create_device(
		instance: &VulkanInstance,
		physical: &PhysicalDevice,
		discovery: &QueueDiscovery,
		extensions: &[*const i8],
	)-> Result<(VulkanDevice), <vb as Backend>::Error> {
		let priorities = [1.0f32];
		let mut unique_indices = std::collections::HashSet::new();
		unique_indices.insert(discovery.graphics);
		unique_indices.insert(discovery.compute);
		// 2. Iterate through your transfer family indices and insert them
		for &transfer_family_idx in &discovery.transfer {
			unique_indices.insert(Some(transfer_family_idx));
		}
		if let Some(p) = discovery.present { unique_indices.insert(Some(p)); }
		

		let queue_infos: Vec<_> = unique_indices
			.iter()
			.map(|&index| {
				vk::DeviceQueueCreateInfo::default()
					.queue_family_index(index.expect("NOT FOUND"))
					.queue_priorities(&priorities)
			})
			.collect();
		
		let mut features_12 = vk::PhysicalDeviceVulkan12Features::default()
			.timeline_semaphore(true);
		
		let mut features_13 = vk::PhysicalDeviceVulkan13Features::default()
			.synchronization2(true)
			.dynamic_rendering(true);

		let create_info = vk::DeviceCreateInfo::default()
			.queue_create_infos(&queue_infos)
			.enabled_extension_names(extensions)
			.push_next(&mut features_13)
			.push_next(&mut features_12);

		let device_handle = unsafe {
			instance.instance()
				.create_device(physical.handle(), &create_info, None)?
		};
		

		Ok(VulkanDevice{inner:device_handle, })
	}
}