use crate::infra::platform::Surface;
use crate::infra::platform::VulkanWindow;

use crate::core::buf_state::Undefined;
use crate::core::types::BufferUsage;
use crate::core::types::MemoryPropertyFlags;
use crate::core::types::MemoryRequirements;
use crate::core::Backend;
use crate::core::DeviceOps;
use crate::core::Buffer;
use crate::infra::vulkan::backend::{
	LogicalDevice, PhysicalDevice, QueueDiscovery, QueueLane,
	Swapchain, VulkanBackend, VulkanDevice, VulkanEntry, VulkanInstance
};
use crate::infra::vulkan::context::presentation::MemoryIndices;
use std::sync::Arc;

pub struct VulkanContext {
	pub device:   VulkanDevice,
	pub lanes:    QueueLane,
	pub physical: PhysicalDevice,
	pub instance: Arc<VulkanInstance>,
	pub entry:    Arc<VulkanEntry>,
	pub indices: MemoryIndices
}

impl VulkanContext {
	pub fn new<W: VulkanWindow>(
		window: &W,
	) -> Result<(Self, Surface), <VulkanBackend as Backend>::Error> {
		let entry    = Arc::new(VulkanEntry::new()
			.expect("Failed to load Vulkan"));
		let instance = Arc::new(
			VulkanInstance::with_extensions(
				Arc::clone(&entry),
				&W::required_vulkan_extensions(),
			)?
		);
		
		let surface = window.create_surface(&entry, Arc::clone(&instance))?;
		let physical  = PhysicalDevice::pick(&instance);
		let discovery = QueueDiscovery::find_queue(&instance, &physical, Some(&surface));
		
		let (device, lanes) = LogicalDevice::with_extensions(
			&instance,
			&physical,
			&discovery,
			&Swapchain::required_device_extensions(),
		)?;
		// ── compute memory indices ─────────────────────────────
		let tmp = device.create_buffer(4, BufferUsage::TRANSFER_SRC)?;
		let req = device.get_buffer_memory_requirements(tmp);
		device.destroy_buffer(tmp);
		let indices = MemoryIndices::find(
			&physical,
			&instance,
			req,
		);
		Ok(( Self { entry, instance, physical, device, lanes, indices, }, surface,))
	}
	
	pub fn headless() -> Result<Self,  <VulkanBackend as Backend>::Error> {
		let entry    = Arc::new(VulkanEntry::new()
			.expect("Failed to load Vulkan"));
		let instance = Arc::new(VulkanInstance::new(Arc::clone(&entry))?);
		let physical  = PhysicalDevice::pick(&instance);
		let discovery = QueueDiscovery::find_queue(&instance, &physical, None);
		let (device, lanes) = LogicalDevice::with_extensions(
			&instance,
			&physical,
			&discovery,
			&[],
		)?;
		let tmp = device.create_buffer(4, BufferUsage::TRANSFER_SRC)?;
		
		let req = device.get_buffer_memory_requirements(tmp);
		
		device.destroy_buffer(tmp);
		let indices = MemoryIndices::find(
			&physical,
			&instance,
			req,
		);
		
		Ok(Self { entry, instance, physical, device, lanes, indices })
	}
	
	/// Convenience — exposes the device typed to VulkanBackend.
	/// Needed when passing to APIs that are generic over B: Backend.
	pub fn device(&self) -> &<VulkanBackend as Backend>::Device {
		&self.device
	}
	pub fn physical(&self) -> &PhysicalDevice {
		&self.physical
	}
	/// Host-visible staging buffer (Undefined state).
	pub fn staging_upload<'dev>(
		&'dev self,
		size: u64,
		family: u32,
	) -> Result<Buffer<'dev, Undefined, VulkanBackend>, <VulkanBackend as Backend>::Error> {
		let staging_memory_index = self.indices.staging;
			Buffer::allocate(
			self.device(),
			size,
			BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
			staging_memory_index,
			family,
		)
	}
	
	/// Device-local vertex buffer (Undefined state).
	pub fn vertex_buffer<'dev>(
		&'dev self,
		size: u64,
		family: u32,
	) -> Result<Buffer<'dev, Undefined, VulkanBackend>, <VulkanBackend as Backend>::Error> {
		let device_local_memory_index = self.indices.device_local;
		Buffer::allocate(
			self.device(),
			size,
			BufferUsage::VERTEX | BufferUsage::TRANSFER_DST,
			device_local_memory_index,
			family,
		)
	}
	
	/// Device-local storage buffer (Undefined state).
	pub fn storage_buffer<'dev>(
		&'dev self,
		size: u64,
		family: u32,
	) -> Result<Buffer<'dev, Undefined, VulkanBackend>, <VulkanBackend as Backend>::Error> {
		let device_local_memory_index = self.indices.device_local;
		Buffer::allocate(
			self.device(),
			size,
			BufferUsage::STORAGE | BufferUsage::TRANSFER_DST,
			device_local_memory_index,
			family,
		)
	}
	
	/// Upload typed CPU data into a staging buffer.
	pub fn staging_from_slice<'dev, T: Copy>(
		&'dev self,
		data: &[T],
		family: u32,
	) -> Result<Buffer<'dev, Undefined, VulkanBackend>, <VulkanBackend as Backend>::Error> {
		let size = (data.len() * std::mem::size_of::<T>()) as u64;
		let buf = self.staging_upload(size, family)?;
		
		buf.with_mapped::<T, _, _>(data.len(), |dst| {
			dst.copy_from_slice(data);
		})?;
		
		Ok(buf)
	}
	#[inline]
	pub fn staging_memory_index( &self, req: MemoryRequirements,) -> u32 {
		self.physical().find_memory_type(
			&self.instance,
			req,
			MemoryPropertyFlags::HOST_VISIBLE
				| MemoryPropertyFlags::HOST_COHERENT,
		).expect("REASON")
	}
	#[inline]
	pub fn create_buffer_with_requirements(
		&self,
		size: u64,
		usage: BufferUsage,
	) -> Result<( <VulkanBackend as Backend>::Buffer, MemoryRequirements ),
		<VulkanBackend as Backend>::Error>
	{
		let device = self.device();
		let handle = device.create_buffer(size, usage)?;
		let req = device.get_buffer_memory_requirements(handle);
		Ok((handle, req))
	}
	
	pub(crate) fn entry(&self) -> &Arc<VulkanEntry> { &self.entry }
	pub(crate) fn instance(&self)  -> &VulkanInstance {&self.instance }
	pub(crate) fn instance_arc(&self) -> Arc<VulkanInstance> { Arc::clone(&self.instance) }
	pub(crate) fn queues(&self) -> &QueueLane { &self.lanes }
}