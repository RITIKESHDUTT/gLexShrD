use crate::core::types::MemoryPropertyFlags;
use crate::core::types::MemoryRequirements;
use super::VulkanInstance;
use ash::vk;

pub struct PhysicalDevice {
	handle: vk::PhysicalDevice,
	
}
impl PhysicalDevice  {
	pub fn pick(instance: &VulkanInstance) -> Self {
		let devices = unsafe {
			instance.instance()
				.enumerate_physical_devices()
				.expect("No Vulkan devices found")
		};
		
		let mut fallback: Option<vk::PhysicalDevice> = None;
		
		for device in devices {
			let props = unsafe {
				instance.instance()
					.get_physical_device_properties(device)
			};
			
			match props.device_type {
				vk::PhysicalDeviceType::DISCRETE_GPU => {
					return Self {     handle: device,
					};
				}
				vk::PhysicalDeviceType::INTEGRATED_GPU => {
					if fallback.is_none() {
						fallback = Some(device);
					}
				}
				_ => {}
			}
		}
		
		Self {
			handle: fallback.expect("No suitable GPU found"),
		}
	}
	fn properties(&self, instance: &VulkanInstance) -> vk::PhysicalDeviceProperties {
		unsafe {
			instance.instance()
				.get_physical_device_properties(self.handle)
		}
	}
	
	pub fn memory_properties(
		&self,
		instance: &VulkanInstance,
	) -> vk::PhysicalDeviceMemoryProperties {
		unsafe {
			instance
				.instance()
				.get_physical_device_memory_properties(self.handle)
		}
	}
	
	pub fn vendor_id(&self, instance: &VulkanInstance) -> u32 {
		self.properties(instance).vendor_id
	}
	
	pub fn name(&self, instance: &VulkanInstance) -> String {
		let props = self.properties(instance);
		// SAFE: Validates null terminator
		let bytes: &[u8] = unsafe {
			std::slice::from_raw_parts(
				props.device_name.as_ptr() as *const u8,
				props.device_name.len(),
			)
		};
		
		std::ffi::CStr::from_bytes_until_nul(bytes)
			.ok()
			.and_then(|s| s.to_str().ok())
			.unwrap_or("Unknown GPU")
			.to_string()
	}
	
	pub fn handle(&self) -> vk::PhysicalDevice {
		self.handle
	}
	pub fn find_memory_type(
		&self,
		instance: &VulkanInstance,
		requirements: MemoryRequirements,
		properties: MemoryPropertyFlags
	) -> Option<u32> {
		let mem_props = self.memory_properties(instance);
		(0..mem_props.memory_type_count).find(|&i| {
			let supported =
				(requirements.memory_type_bits & (1 << i)) != 0;
			
			let flags =
				mem_props.memory_types[i as usize].property_flags;
			
			supported && flags.contains(properties.into())
		})
	}
}