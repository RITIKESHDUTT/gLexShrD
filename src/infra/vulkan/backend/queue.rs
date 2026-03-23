use super::{PhysicalDevice, VulkanInstance};
use crate::core::type_state_queue::{
	sealed::QueueHandle, Compute,
	Graphics, Present, Queue, Transfer
};
use crate::infra::platform::Surface;
use crate::infra::vulkan::backend::VulkanBackend;
use crate::infra::vulkan::backend::VulkanDevice;
use ash::vk;

pub struct QueueDiscovery {
	pub graphics: Option<u32>,
	pub compute:  Option<u32>,
	pub transfer: Vec<u32>,
	pub present:  Option<u32>,
}

impl QueueDiscovery {
	pub fn find_queue(
		instance: &VulkanInstance,
		physical: &PhysicalDevice,
		surface: Option<&Surface>,
	) -> Self {
		let families = unsafe {
			instance
				.instance()
				.get_physical_device_queue_family_properties(physical.handle())
		};
		
		let mut graphics = None;
		let mut compute = None;
		let mut transfer = None;
		let mut present = None;
		
		for (index, props) in families.iter().enumerate() {
			let i = index as u32;
			let flags = props.queue_flags;
			
			// --- GRAPHICS ---
			if flags.contains(vk::QueueFlags::GRAPHICS) && graphics.is_none() {
				graphics = Some(i);
			}
			
			// --- COMPUTE (prefer dedicated) ---
			let dedicated_compute =
				flags.contains(vk::QueueFlags::COMPUTE)
					&& !flags.contains(vk::QueueFlags::GRAPHICS);
			
			if flags.contains(vk::QueueFlags::COMPUTE) {
				if dedicated_compute || compute.is_none() {
					compute = Some(i);
				}
			}
			
			// --- TRANSFER (pick ONE dedicated only) ---
			let dedicated_transfer =
				flags.contains(vk::QueueFlags::TRANSFER)
					&& !flags.contains(vk::QueueFlags::GRAPHICS)
					&& !flags.contains(vk::QueueFlags::COMPUTE);
			
			if transfer.is_none() && dedicated_transfer {
				transfer = Some(i);
			}
			
			// --- PRESENT ---
			if let Some(s) = surface {
				if unsafe {
					s.supports_queue_family(physical.handle(), i).unwrap_or(false)
				} {
					if present.is_none() {
						present = Some(i);
					}
				}
			}
		}
		
		// --- FALLBACKS ---
		
		let graphics = graphics.expect("No graphics queue");
		
		let compute = compute.unwrap_or(graphics);
		
		let transfer = transfer.unwrap_or(graphics);
		
		// --- ALIAS FIX ---
		let transfer = if transfer == compute {
			graphics
		} else {
			transfer
		};
		
		Self {
			graphics: Some(graphics),
			compute: Some(compute),
			present,
			transfer: vec![transfer], // ALWAYS exactly one
		}
	}
}


pub struct QueueLane{
	transfer_q: Vec<Queue<Transfer, VulkanBackend>>,
	graphics_q: Queue<Graphics, VulkanBackend>,
	present_q:  Option<Queue<Present, VulkanBackend>>,
	compute_q: Option<Queue<Compute, VulkanBackend>>,
}

impl QueueLane {
	pub fn new(device: &VulkanDevice, discovery: &QueueDiscovery) -> Self {
		let get_raw = |idx: u32| unsafe {
			device.inner.get_device_queue(idx, 0)
		};
		Self {
			graphics_q: {
				let i = discovery.graphics.expect("No Graphics Index");
				Queue::new(get_raw(i), i)
			},
			present_q: discovery.present.map(|i| Queue::new(get_raw(i), i)),
			compute_q: discovery.compute.map(|i| Queue::new(get_raw(i), i)),
			transfer_q: discovery.transfer.iter().map(|i| Queue::new(get_raw(*i), *i)).collect(),
		}
	}
	pub fn graphics(&self) -> &Queue<Graphics, VulkanBackend> {
		&self.graphics_q
	}
	
	pub fn present(&self) -> Option<&Queue<Present, VulkanBackend>> {
		self.present_q.as_ref()
	}
	
	pub fn compute(&self) -> Option<&Queue<Compute, VulkanBackend>> {
		self.compute_q.as_ref()
	}
	/// Get the "Next" available transfer lane for load balancing
	pub fn next_transfer(&self, frame_count: usize) -> &Queue<Transfer, VulkanBackend> {
		&self.transfer_q[frame_count % self.transfer_q.len()]
	}
	/// Returns how many dedicated transfer lanes we found (1, 3, 4, 5 on my GPU).
	pub fn transfer_count(&self) -> usize {
		self.transfer_q.len()
	}
	/// Grabs a reference to a specific lane by index.
	pub fn transfer(&self, index: usize) -> Option<&Queue<Transfer, VulkanBackend>> {
		self.transfer_q.get(index)
	}
	pub fn transfer_family(&self) -> Option<u32> {
		// Return the family of the first lane if it exists
		self.transfer_q.first().map(|q| q.family())
	}
	pub fn transfer_queues(&self) -> &[Queue<Transfer, VulkanBackend>] {
		&self.transfer_q
	}
}