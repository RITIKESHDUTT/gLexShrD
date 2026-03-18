use crate::renderer::ComputeStorage;
use crate::renderer::VertexStorage;
use crate::renderer::GfxStorage;
use crate::core::backend::{
	types::{
		DescriptorBinding,
		DescriptorType,
		PipelineBindPoint,
		ShaderStages,
	}, Backend, CommandOps,
	DeviceOps,
};
use crate::core::cmd::{CommandBuffer, Inside, Recording};

use crate::core::resource::{DescriptorPool, Sampler};
use crate::domain::ImageLayout;
use std::marker::PhantomData;
//
// ─────────────────────────────────────────────────────────────
// Descriptor State Machine
// ─────────────────────────────────────────────────────────────
//

pub mod desc_state {
	pub struct Unallocated;
	pub struct Allocated; // building phase
	pub struct Updated;   // immutable, ready for bind
	pub struct Bound;
}

//
// ─────────────────────────────────────────────────────────────
// Static Binding Contract
// ─────────────────────────────────────────────────────────────
//

/// One descriptor slot.
/// Implement this once per binding.
pub trait Binding {
	const INDEX: u32;
	const TYPE: DescriptorType;
	const STAGES: ShaderStages;
	const COUNT: u32 = 1;
}

//
// ─────────────────────────────────────────────────────────────
// Descriptor Set Interface
// ─────────────────────────────────────────────────────────────
//

/// Represents a full descriptor set layout contract.
///
/// This replaces the earlier `single/pair/triple` layout creation
/// and makes the layout a **compile-time type identity**.
pub trait DescriptorSetInterface {
	const BINDINGS: &'static [DescriptorBinding];
}

//
// ─────────────────────────────────────────────────────────────
// Descriptor Layout
// ─────────────────────────────────────────────────────────────
//

/// A GPU descriptor set layout derived from a descriptor interface.
///
/// The interface type ensures the layout matches shader expectations.
pub struct DescriptorLayout<'dev, B: Backend, Iface> {
	device: &'dev B::Device,
	handle: B::DescriptorSetLayout,
	_iface: PhantomData<Iface>,
}

impl<'dev, B: Backend, Iface: DescriptorSetInterface> DescriptorLayout<'dev, B, Iface>
	where
		B::Device: DeviceOps<B>,
{
	/// Create descriptor layout from interface definition.
	pub fn new(device: &'dev B::Device) -> Result<Self, B::Error> {
		let handle =
			device.create_descriptor_set_layout(Iface::BINDINGS)?;
		Ok(Self {
			device,
			handle,
			_iface: PhantomData,
		})
	}
	
	#[inline]
	pub fn handle(&self) -> B::DescriptorSetLayout {
		self.handle
	}
}

//
// ─────────────────────────────────────────────────────────────
// Descriptor Set
// ─────────────────────────────────────────────────────────────
//

pub struct DescriptorSet<'dev, S, B: Backend, Iface> {
	pub(crate) device: &'dev B::Device,
	pub(crate) handle: B::DescriptorSet,
	pub(crate) _state: PhantomData<S>,
	pub(crate) _iface: PhantomData<Iface>,
}

impl<S, B: Backend, Iface> DescriptorSet<'_, S, B, Iface> {
	#[inline]
	pub fn handle(&self) -> B::DescriptorSet {
		self.handle
	}
}


//
// ─────────────────────────────────────────────────────────────
// Allocation
// ─────────────────────────────────────────────────────────────
//

impl<B: Backend, Iface: DescriptorSetInterface>
DescriptorSet<'_, desc_state::Unallocated, B, Iface>
	where
		B::Device: DeviceOps<B>,
{
	/// Allocate descriptor set from pool using a typed layout.
	pub fn allocate<'dev>(
		device: &'dev B::Device,
		pool: &DescriptorPool<'_, B>,
		layout: &DescriptorLayout<'dev, B, Iface>,
	) -> Result<DescriptorSet<'dev, desc_state::Allocated, B, Iface>, B::Error> {
		let handle = device.allocate_descriptor_set(pool.handle(), layout.handle())?;
		
		Ok(DescriptorSet {
			device,
			handle,
			_state: PhantomData,
			_iface: PhantomData,
		})
	}
}


//
// ─────────────────────────────────────────────────────────────
// Building Phase (Multi-Write Safe)
// ─────────────────────────────────────────────────────────────
//

impl<'dev, B: Backend, Iface> DescriptorSet<'dev, desc_state::Allocated, B, Iface>
	where
		B::Device: DeviceOps<B>,
{
	/// Write a buffer descriptor using a binding type.
	pub fn write_buffer<BIND: Binding>(
		self,
		buffer: B::Buffer,
		size: u64,
	) -> Self {
		debug_assert!(
			BIND::TYPE == DescriptorType::UniformBuffer
				|| BIND::TYPE == DescriptorType::StorageBuffer
		);
		
		self.device.write_descriptor_buffer(
			self.handle,
			BIND::INDEX,
			BIND::TYPE,
			buffer,
			0,
			size,
		);
		
		self
	}
	
	/// Write a combined image sampler descriptor.
	pub fn write_image_sampler<BIND: Binding>(
		self,
		sampler: &Sampler<'_, B>,
		image_view: B::ImageView,
	) -> Self {
		debug_assert!(BIND::TYPE == DescriptorType::CombinedImageSampler);
		
		self.device.write_descriptor_image(
			self.handle,
			BIND::INDEX,
			BIND::TYPE,
			sampler.handle(),
			image_view,
			ImageLayout::ShaderReadOnly,
		);
		
		self
	}
	
	/// Finalize descriptor writing phase.
	pub fn finish(self) -> DescriptorSet<'dev, desc_state::Updated, B, Iface> {
		DescriptorSet {
			device: self.device,
			handle: self.handle,
			_state: PhantomData,
			_iface: PhantomData,
		}
	}
}

//
// ─────────────────────────────────────────────────────────────
// Binding Phase
// ─────────────────────────────────────────────────────────────
//

impl<'dev, B: Backend, Iface> DescriptorSet<'dev, desc_state::Updated, B, Iface>
	where
		B::Device: DeviceOps<B>,
{
	pub fn bind(
		self,
		cmd: &CommandBuffer<'_, Recording, B, Inside>,
		layout: B::PipelineLayout,
		set_index: u32,
	) -> DescriptorSet<'dev, desc_state::Bound, B, Iface> {
		cmd.device.cmd_bind_descriptor_sets(
			cmd.handle(),
			PipelineBindPoint::GRAPHICS,
			layout,
			set_index,
			&[self.handle],
			&[],
		);
		
		DescriptorSet {
			device: self.device,
			handle: self.handle,
			_state: PhantomData,
			_iface: PhantomData,
		}
	}
	
	/// Borrow bind — for persistent descriptor sets reused across frames.
	pub fn bind_ref(
		&self,
		cmd: &CommandBuffer<'_, Recording, B, Inside>,
		layout: B::PipelineLayout,
		set_index: u32,
	) {
		cmd.device.cmd_bind_descriptor_sets(
			cmd.handle(),
			PipelineBindPoint::GRAPHICS,
			layout,
			set_index,
			&[self.handle],
			&[],
		);
	}
	
}

//
// ─────────────────────────────────────────────────────────────
// Example Static Bindings
// ─────────────────────────────────────────────────────────────
//

pub struct GlobalUBO;

impl Binding for GlobalUBO {
	const INDEX: u32 = 0;
	const TYPE: DescriptorType = DescriptorType::UniformBuffer;
	const STAGES: ShaderStages = ShaderStages::VERTEX;
}

/// Texture sampler binding at slot 1
pub struct MainTexture;

impl Binding for MainTexture {
	const INDEX: u32 = 1;
	const TYPE: DescriptorType =
		DescriptorType::CombinedImageSampler;
	const STAGES: ShaderStages = ShaderStages::FRAGMENT;
}
//
// ─────────────────────────────────────────────────────────────
// Example Descriptor Set Interface
// ─────────────────────────────────────────────────────────────
//

pub struct SceneSet;

impl DescriptorSetInterface for SceneSet {
	const BINDINGS: &'static [DescriptorBinding] = &[
		DescriptorBinding {
			binding: GlobalUBO::INDEX,
			descriptor_type: GlobalUBO::TYPE,
			count: GlobalUBO::COUNT,
			stages: GlobalUBO::STAGES,
		},
		DescriptorBinding {
			binding: MainTexture::INDEX,
			descriptor_type: MainTexture::TYPE,
			count: MainTexture::COUNT,
			stages: MainTexture::STAGES,
		},
	];
}




/// Compute storage read
pub struct StorageRead;

impl Binding for StorageRead {
	const INDEX: u32 = 0;
	const TYPE: DescriptorType = DescriptorType::StorageBuffer;
	const STAGES: ShaderStages = ShaderStages::COMPUTE;
}

/// Compute storage write
pub struct StorageWrite;

impl Binding for StorageWrite {
	const INDEX: u32 = 1;
	const TYPE: DescriptorType =DescriptorType::StorageBuffer;
	const STAGES: ShaderStages = ShaderStages::COMPUTE;
}


impl<'dev, B: Backend, Iface> Drop for DescriptorLayout<'dev, B, Iface>
	where
		B::Device: DeviceOps<B>,
{
	fn drop(&mut self) {
		self.device.destroy_descriptor_set_layout(self.handle);
	}
}

//================================

impl<'dev, B: Backend, Iface: DescriptorSetInterface>
DescriptorSet<'dev, desc_state::Updated, B, Iface>
	where
		B::Device: DeviceOps<B>,
{
	pub fn build(
		device: &'dev B::Device,
		pool: &DescriptorPool<'_, B>,
		layout: &DescriptorLayout<'dev, B, Iface>,
		f: impl FnOnce(DescriptorSet<'dev, desc_state::Allocated, B, Iface>)
			-> DescriptorSet<'dev, desc_state::Allocated, B, Iface>,
	) -> Result<Self, B::Error> {
		let set = DescriptorSet::allocate(device, pool, layout)?;
		let set = f(set);
		Ok(set.finish())
	}
}

impl<'dev, B: Backend>
DescriptorSet<'dev, desc_state::Updated, B, GfxStorage>
	where
		B::Device: DeviceOps<B>,
{
	pub fn vertex_storage(
		device: &'dev B::Device,
		pool: &DescriptorPool<'_, B>,
		layout: &DescriptorLayout<'dev, B, GfxStorage>,
		buffer: B::Buffer,
		size: u64,
	) -> Result<Self, B::Error> {
		DescriptorSet::build(device, pool, layout, |set| {
			set.write_buffer::<VertexStorage>(buffer, size)
		})
	}
}

impl<'dev, B: Backend>
DescriptorSet<'dev, desc_state::Updated, B, ComputeStorage>
	where
		B::Device: DeviceOps<B>,
{
	pub fn storage_pair(
		device: &'dev B::Device,
		pool: &DescriptorPool<'_, B>,
		layout: &DescriptorLayout<'dev, B, ComputeStorage>,
		read: B::Buffer,
		write: B::Buffer,
		size: u64,
	) -> Result<Self, B::Error> {
		DescriptorSet::build(device, pool, layout, |set| {
			set.write_buffer::<StorageRead>(read, size)
			   .write_buffer::<StorageWrite>(write, size)
		})
	}
}