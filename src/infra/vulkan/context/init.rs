// src/infra/vulkan/context/init.rs
//
// VulkanContext: backend initialisation only.
//
// Responsible for:
//   - Vulkan entry / instance / surface creation
//   - Physical device selection
//   - Logical device + queue lane construction
//   - MemoryIndices (needed by swapchain and non-arena paths)
//
// NOT responsible for:
//   - Buffer / image allocation helpers  →  GpuContext
//   - Arena allocator lifecycle           →  GpuContext
//   - Frame synchronisation              →  GpuContext

use crate::infra::platform::{Surface, VulkanWindow};
use crate::infra::vulkan::backend::{
	LogicalDevice, PhysicalDevice, QueueDiscovery, QueueLane,
	Swapchain, VulkanBackend, VulkanDevice, VulkanEntry, VulkanInstance,
};
use crate::infra::vulkan::context::presentation::MemoryIndices;
use crate::core::{Backend, DeviceOps};
use crate::core::types::BufferUsage;
use std::sync::Arc;

pub struct VulkanContext {
	/// Arc so GpuContext and GpuAllocator can share ownership without borrowing.
	pub device:   Arc<VulkanDevice>,
	pub lanes:    QueueLane,
	pub physical: PhysicalDevice,
	pub instance: Arc<VulkanInstance>,
	pub entry:    Arc<VulkanEntry>,
	/// Memory type indices for swapchain and any non-arena allocations.
	pub indices:  MemoryIndices,
}

impl VulkanContext {
	pub fn new<W: VulkanWindow>(
		window: &W,
	) -> Result<(Self, Surface), <VulkanBackend as Backend>::Error> {
		let entry    = Arc::new(VulkanEntry::new().expect("Failed to load Vulkan"));
		let instance = Arc::new(VulkanInstance::with_extensions(
			Arc::clone(&entry),
			&W::required_vulkan_extensions(),
		)?);
		
		let surface   = window.create_surface(&entry, Arc::clone(&instance))?;
		let physical  = PhysicalDevice::pick(&instance);
		let discovery = QueueDiscovery::find_queue(&instance, &physical, Some(&surface));
		
		let (device_raw, lanes) = LogicalDevice::with_extensions(
			&instance, &physical, &discovery,
			&Swapchain::required_device_extensions(),
		)?;
		let device = Arc::new(device_raw);
		
		// Probe memory indices via a throwaway buffer handle.
		let tmp     = device.create_buffer(4, BufferUsage::TRANSFER_SRC)?;
		let req     = device.get_buffer_memory_requirements(tmp);
		device.destroy_buffer(tmp);
		let indices = MemoryIndices::find(&physical, &instance, req);
		
		Ok((Self { entry, instance, physical, device, lanes, indices }, surface))
	}
	
	pub fn headless() -> Result<Self, <VulkanBackend as Backend>::Error> {
		let entry    = Arc::new(VulkanEntry::new().expect("Failed to load Vulkan"));
		let instance = Arc::new(VulkanInstance::new(Arc::clone(&entry))?);
		let physical  = PhysicalDevice::pick(&instance);
		let discovery = QueueDiscovery::find_queue(&instance, &physical, None);
		
		let (device_raw, lanes) = LogicalDevice::with_extensions(
			&instance, &physical, &discovery, &[],
		)?;
		let device = Arc::new(device_raw);
		
		let tmp     = device.create_buffer(4, BufferUsage::TRANSFER_SRC)?;
		let req     = device.get_buffer_memory_requirements(tmp);
		device.destroy_buffer(tmp);
		let indices = MemoryIndices::find(&physical, &instance, req);
		
		Ok(Self { entry, instance, physical, device, lanes, indices })
	}
	
	// ── Accessors ─────────────────────────────────────────────────────────────
	
	pub fn device(&self) -> &<VulkanBackend as Backend>::Device { &self.device }
	pub fn device_arc(&self)   -> Arc<VulkanDevice>    { Arc::clone(&self.device)   }
	pub fn physical(&self)     -> &PhysicalDevice       { &self.physical             }
	pub fn queues(&self)       -> &QueueLane             { &self.lanes                }
	pub fn instance(&self)     -> &VulkanInstance        { &self.instance             }
	pub fn instance_arc(&self) -> Arc<VulkanInstance>   { Arc::clone(&self.instance) }
	pub fn entry(&self)        -> &Arc<VulkanEntry>      { &self.entry                }
}