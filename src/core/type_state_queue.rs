use crate::core::backend::Backend;
use std::marker::PhantomData;


// ── Capability markers (zero-sized, zero-cost) ──────────────
pub struct Graphics;

pub struct Present;

pub struct Compute;
pub struct Transfer;
// ── Sealed handle access ────────────────────────────────────
// pub(crate) module: visible inside crate, invisible outside.
// External code can USE the traits as bounds but can NEVER
// implement them (sealed pattern — no escape hatch).
pub(crate) mod sealed {
	pub trait QueueHandle {
		type Handle: Copy;
		fn raw(&self) -> Self::Handle;
		fn family(&self) -> u32;
	}
}

// ── Public capability traits ────────────────────────────────
/// Compile-time proof: this queue can submit graphics/compute commands.
pub trait SupportsGraphics: sealed::QueueHandle {}
/// Compile-time proof: this queue can present to a surface.
pub trait SupportsPresent: sealed::QueueHandle {}

pub trait SupportsCompute: sealed::QueueHandle {}
pub trait SupportsTransfer: sealed::QueueHandle {}

// ── The queue wrapper ───────────────────────────────────────
/// A Vulkan queue with compile-time capability enforcement.
///
/// `C` is a capability marker:
/// - `Graphics` — graphics-only queue
/// - `Present` — present-only queue
/// - `(Graphics, Present)` — unified queue (common on desktop GPUs)
#[derive(Debug)]
pub struct Queue<C, B: Backend> {
	queue: B::Queue,
	family: u32,
	_cap: PhantomData<(C, B)>,
}

impl<C, B: Backend> Copy for Queue<C, B> {}

impl<C, B: Backend> Clone for Queue<C, B> {
	fn clone(&self) -> Self { *self }
}

impl<C, B: Backend> Queue<C, B> {
	/// Construct a typed queue. Only infra layer calls this.
	pub(crate) fn new(queue: B::Queue, family: u32) -> Self {
		Self { queue, family, _cap: PhantomData }
	}
}

// ── Sealed impl for all variants (crate-only access) ────────
impl<C, B: Backend> sealed::QueueHandle for Queue<C, B> {
	type Handle = B::Queue;
	fn raw(&self) -> B::Queue { self.queue }
	fn family(&self) -> u32 { self.family }
}


// ── Capability wiring ───────────────────────────────────────

// ─────────────────────────────────────────────────────────────
// Capability Wiring — Single Capability
// ─────────────────────────────────────────────────────────────
impl<B: Backend> SupportsGraphics for Queue<Graphics, B> {}
impl<B: Backend> SupportsPresent for Queue<Present, B> {}
impl<B: Backend> SupportsCompute for Queue<Compute, B> {}
impl<B: Backend> SupportsTransfer for Queue<Transfer, B> {}

// ─────────────────────────────────────────────────────────────
// Dual Capability Wiring
// ─────────────────────────────────────────────────────────────
// Unified queue — both capabilities, zero cost


impl<B: Backend> SupportsGraphics for Queue<(Graphics, Present), B> {}
impl<B: Backend> SupportsPresent for Queue<(Graphics, Present), B> {}
impl<B: Backend> SupportsGraphics for Queue<(Graphics, Compute), B> {}
impl<B: Backend> SupportsCompute for Queue<(Graphics, Compute), B> {}
impl<B: Backend> SupportsCompute for Queue<(Compute, Transfer), B> {}
impl<B: Backend> SupportsTransfer for Queue<(Compute, Transfer), B> {}
impl<B: Backend> SupportsTransfer for Queue<(Transfer, Present), B> {}
impl<B: Backend> SupportsPresent for Queue<(Transfer, Present), B> {}


// ─────────────────────────────────────────────────────────────
// Triple Capability Wiring
// ─────────────────────────────────────────────────────────────
impl<B: Backend> SupportsGraphics for Queue<(Graphics, Compute, Present), B> {}
impl<B: Backend> SupportsCompute for Queue<(Graphics, Compute, Present), B> {}
impl<B: Backend> SupportsPresent for Queue<(Graphics, Compute, Present), B> {}
impl<B: Backend> SupportsGraphics for Queue<(Graphics, Compute, Transfer), B> {}
impl<B: Backend> SupportsCompute for Queue<(Graphics, Compute, Transfer), B> {}
impl<B: Backend> SupportsTransfer for Queue<(Graphics, Compute, Transfer), B> {}

// ─────────────────────────────────────────────────────────────
// Fully Unified Queue
// ─────────────────────────────────────────────────────────────

impl<B: Backend> SupportsGraphics for Queue<(Graphics, Compute, Transfer, Present), B> {}
impl<B: Backend> SupportsCompute for Queue<(Graphics, Compute, Transfer, Present), B> {}
impl<B: Backend> SupportsTransfer for Queue<(Graphics, Compute, Transfer, Present), B> {}
impl<B: Backend> SupportsPresent for Queue<(Graphics, Compute, Transfer, Present), B> {}