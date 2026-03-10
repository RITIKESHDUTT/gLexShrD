use crate::core::exec::command::PassCommand;
use crate::core::render::cache::PipelineId;
use {
	crate::core::{
		type_state_queue::{Compute, Graphics, Transfer},
		types::PushConstantRange,
	},
	crate::domain::{
		DescriptorSetId,
		GraphError,
		PassDomain,
		PassId,
		ResourceDecl,
		ResourceHandle,
		ResourceId,
		ResourceKind,
		UsageIntent,
	},
	std::{
		collections::{HashMap, HashSet},
		marker::PhantomData,
	},
};

#[derive(Debug)]
pub struct BarrierEdge {
	pub resource: ResourceId,
	pub from_pass: PassId,
	pub to_pass: PassId,
	pub src_usage: UsageIntent,
	pub dst_usage: UsageIntent,
	pub src_domain: PassDomain,
	pub dst_domain: PassDomain,
}

impl BarrierEdge {
	pub fn is_cross_queue(&self) -> bool {
		self.src_domain != self.dst_domain
	}
}

/// The compiled execution order from a frame graph.
#[derive(Debug)]
pub struct ExecutionOrder {
	pub ordered_passes: Vec<PassId>,
	pub barriers: Vec<BarrierEdge>,
}


// ─── PassDecl ────────────────────────────────────────────────
// Replaced: record: Option<PassRecord<'f, B>> → commands: Vec<PassCommand>
// Removed:  'f lifetime (existed solely for closure borrows)
// Added:    descriptor_set: Option<DescriptorSetId>

pub struct PassDecl {
	pub id: PassId,
	pub pipeline: Option<PipelineId>,
	pub descriptor_set: Option<DescriptorSetId>,
	pub reads: Vec<(ResourceId, UsageIntent)>,
	pub writes: Vec<(ResourceId, UsageIntent)>,
	pub domain: PassDomain,
	pub commands: Vec<PassCommand>,
}


/// Builder for adding passes to a frame graph.
///
/// The pipeline field in PassBuilder is temporary storage that gets
/// transferred into PassDecl when .submit() is called.
///
/// Domain safety: PassBuilder<D> gates which commands are available.
/// PassBuilder<Graphics> has .draw(), PassBuilder<Compute> has .dispatch(),
/// PassBuilder<Transfer> has .copy_buffer(). Calling .dispatch() on a
/// PassBuilder<Graphics> is a compile error.
// Removed:  'f lifetime
// Added:    domain, descriptor_set, commands fields
pub struct PassBuilder<'a, D> {
	graph: &'a mut FrameGraph,
	id: PassId,
	domain: PassDomain,
	pipeline: Option<PipelineId>,
	descriptor_set: Option<DescriptorSetId>,
	reads: Vec<(ResourceId, UsageIntent)>,
	writes: Vec<(ResourceId, UsageIntent)>,
	commands: Vec<PassCommand>,
	_domain: PhantomData<D>,
}

// ─── Generic impl (all domains) ─────────────────────────────

impl<D> PassBuilder<'_, D> {
	pub fn reads(mut self, resource: ResourceId, usage: UsageIntent) -> Self {
		self.reads.push((resource, usage));
		self
	}
	
	pub fn writes(mut self, resource: ResourceId, usage: UsageIntent) -> Self {
		self.writes.push((resource, usage));
		self
	}
	
	pub fn descriptor_set(mut self, id: DescriptorSetId) -> Self {
		self.descriptor_set = Some(id);
		self
	}
	
	pub fn push_constants(mut self, range: PushConstantRange, data: &[u8]) -> Self {
		let mut buf = [0u8; 128];
		buf[..data.len()].copy_from_slice(data);
		self.commands.push(PassCommand::PushConstants { range, data: buf });
		self
	}
	
	pub fn submit(self) -> PassId {
		self.validate_resource_declarations();
		let id = self.id;
		self.graph.passes.push(PassDecl {
			id,
			pipeline: self.pipeline,
			descriptor_set: self.descriptor_set,
			reads: self.reads,
			writes: self.writes,
			domain: self.domain,
			commands: self.commands,
		});
		id
	}
	
	
	fn validate_resource_declarations(&self) {
		for cmd in &self.commands {
			match cmd {
				PassCommand::BindVertexBuffer(id) | PassCommand::BindIndexBuffer(id) => {
					debug_assert!(
						self.reads.iter().any(|(r, _)| r == id),
						"Resource {id:?} bound in command but not declared in reads()"
					);
				}
				PassCommand::CopyBuffer { src, dst, .. } => {
					debug_assert!(
						self.reads.iter().any(|(r, _)| r == src),
						"Resource {src:?} used as copy source but not declared in reads()"
					);
					debug_assert!(
						self.writes.iter().any(|(r, _)| r == dst),
						"Resource {dst:?} used as copy destination but not declared in writes()"
					);
				}
				_ => {}
			}
		}
	}
}
// ─── Graphics impl ──────────────────────────────────────────
// Compile-time enforced: only PassBuilder<Graphics> can call these.

impl PassBuilder<'_, Graphics> {
	pub fn bind_pipeline(mut self, id: PipelineId) -> Self {
		self.commands.push(PassCommand::BindPipeline(id));
		self
	}
	
	pub fn draw(mut self, vertex_count: u32) -> Self {
		self.commands.push(PassCommand::Draw { vertex_count });
		self
	}
	
	pub fn draw_indexed(mut self, index_count: u32, instance_count: u32, first_index: u32) -> Self {
		self.commands.push(PassCommand::DrawIndexed { index_count, instance_count, first_index });
		self
	}
	
	pub fn bind_vertex_buffer(mut self, resource: ResourceId) -> Self {
		self.commands.push(PassCommand::BindVertexBuffer(resource));
		self
	}
	
	pub fn bind_index_buffer(mut self, resource: ResourceId) -> Self {
		self.commands.push(PassCommand::BindIndexBuffer(resource));
		self
	}
	
	pub fn bind_descriptor_set(mut self, id: DescriptorSetId) -> Self {
		self.commands.push(PassCommand::BindDescriptorSet(id));
		self
	}
}

// ─── Compute impl ───────────────────────────────────────────
// Compile-time enforced: only PassBuilder<Compute> can call these.

impl PassBuilder<'_, Compute> {
	pub fn bind_pipeline(mut self, id: PipelineId) -> Self {
		self.commands.push(PassCommand::BindPipeline(id));
		self
	}
	
	pub fn dispatch(mut self, x: u32, y: u32, z: u32) -> Self {
		self.commands.push(PassCommand::Dispatch { x, y, z });
		self
	}
	
	pub fn bind_descriptor_set(mut self, id: DescriptorSetId) -> Self {
		self.commands.push(PassCommand::BindDescriptorSet(id));
		self
	}
}

// ─── Transfer impl ──────────────────────────────────────────
// Compile-time enforced: only PassBuilder<Transfer> can call these.

impl PassBuilder<'_, Transfer> {
	pub fn copy_buffer(mut self, src: ResourceId, dst: ResourceId, size: u64, dst_offset: u64) -> Self {
		self.commands.push(PassCommand::CopyBuffer { src, dst, size, dst_offset });
		self
	}
}

/// Declarative frame graph. Resources and passes are declared,
/// then compiled into an execution order with automatic barrier placement.
// Removed: 'f lifetime
// Added:   descriptor_sets registry
pub struct FrameGraph {
	resources: Vec<ResourceDecl>,
	passes: Vec<PassDecl>,
	next_resource_id: ResourceId,
	next_pass_id: PassId,
}

impl FrameGraph {
	pub fn new() -> Self {
		Self {
			resources: Vec::new(),
			passes: Vec::new(),
			next_resource_id: 0,
			next_pass_id: 0,
		}
	}
	
	pub fn add_resource(&mut self, kind: ResourceKind, handle: ResourceHandle) -> ResourceId {
		let id = self.next_resource_id;
		self.next_resource_id += 1;
		self.resources.push(ResourceDecl { id, kind, handle });
		id
	}
	pub fn add_buffer(&mut self, handle:u64) -> ResourceId {
		self.add_resource(ResourceKind::Buffer, ResourceHandle::Buffer(handle))
	}
	pub fn resource(&self, id: ResourceId) -> &ResourceDecl {
		self.resources.iter().find(|r| r.id == id).expect("resource not found")
	}
	
	// Changed: returns PassBuilder<'_, Graphics, B> (no 'f)
	// Added:   domain, descriptor_set, commands fields in builder
	pub fn add_graphics_pass(&mut self, pipeline: Option<PipelineId>) -> PassBuilder<'_, Graphics> {
		let id = self.next_pass_id;
		self.next_pass_id += 1;
		PassBuilder {
			graph: self,
			id,
			domain: PassDomain::Graphics,
			pipeline,
			descriptor_set: None,
			reads: Vec::new(),
			writes: Vec::new(),
			commands: Vec::new(),
			_domain: PhantomData,
		}
	}
	
	pub fn add_compute_pass(&mut self, pipeline: Option<PipelineId>) -> PassBuilder<'_, Compute> {
		let id = self.next_pass_id;
		self.next_pass_id += 1;
		PassBuilder {
			graph: self,
			id,
			domain: PassDomain::Compute,
			pipeline,
			descriptor_set: None,
			reads: Vec::new(),
			writes: Vec::new(),
			commands: Vec::new(),
			_domain: PhantomData,
		}
	}
	
	pub fn add_transfer_pass(&mut self) -> PassBuilder<'_, Transfer> {
		let id = self.next_pass_id;
		self.next_pass_id += 1;
		PassBuilder {
			graph: self,
			id,
			domain: PassDomain::Transfer,
			pipeline: None,
			descriptor_set: None,
			reads: Vec::new(),
			writes: Vec::new(),
			commands: Vec::new(),
			_domain: PhantomData,
		}
	}
	
	
	pub(crate) fn compile_dependencies(&self) -> Result<ExecutionOrder, GraphError> {
		struct ResourceState {
			last_write: Option<(PassId, UsageIntent, PassDomain)>,
			readers: Vec<(PassId, UsageIntent, PassDomain)>,
		}
		
		let mut state: HashMap<ResourceId, ResourceState> = HashMap::new();
		let mut edge_set: HashSet<(PassId, PassId)> = HashSet::new();
		let mut barriers: Vec<BarrierEdge> = Vec::new();
		
		for pass in &self.passes {
			// Reads: need barrier from last writer (RAW)
			for &(res, dst_usage) in &pass.reads {
				let rs = state.entry(res).or_insert_with(|| ResourceState {
					last_write: None,
					readers: Vec::new(),
				});
				
				if let Some((writer_id, writer_usage, writer_domain)) = rs.last_write {
					if writer_id != pass.id && edge_set.insert((writer_id, pass.id)) {
						barriers.push(BarrierEdge {
							resource: res,
							from_pass: writer_id,
							to_pass: pass.id,
							src_usage: writer_usage,
							dst_usage,
							src_domain: writer_domain,
							dst_domain: pass.domain,
						});
					}
				}
				
				rs.readers.push((pass.id, dst_usage, pass.domain));
			}
			
			// Writes: need barriers from last writer (WAW) + all readers (WAR)
			for &(res, usage) in &pass.writes {
				let rs = state.entry(res).or_insert_with(|| ResourceState {
					last_write: None,
					readers: Vec::new(),
				});
				
				// WAW: barrier from previous writer
				if let Some((writer_id, writer_usage, writer_domain)) = rs.last_write {
					if writer_id != pass.id && edge_set.insert((writer_id, pass.id)) {
						barriers.push(BarrierEdge {
							resource: res,
							from_pass: writer_id,
							to_pass: pass.id,
							src_usage: writer_usage,
							dst_usage: usage,
							src_domain: writer_domain,
							dst_domain: pass.domain,
						});
					}
				}
				
				// WAR: barrier from each reader
				for &(reader_id, reader_usage, reader_domain) in &rs.readers {
					if reader_id != pass.id && edge_set.insert((reader_id, pass.id)) {
						barriers.push(BarrierEdge {
							resource: res,
							from_pass: reader_id,
							to_pass: pass.id,
							src_usage: reader_usage,
							dst_usage: usage,
							src_domain: reader_domain,
							dst_domain: pass.domain,
						});
					}
				}
				
				// Reset: this write is now the last, clear readers
				rs.last_write = Some((pass.id, usage, pass.domain));
				rs.readers.clear();
			}
		}
		
		let pass_count = self.passes.len();
		let mut in_degree: HashMap<PassId, usize> = HashMap::new();
		let mut adj: HashMap<PassId, Vec<PassId>> = HashMap::new();
		
		for pass in &self.passes {
			in_degree.entry(pass.id).or_insert(0);
			adj.entry(pass.id).or_default();
		}
		
		for &(from, to) in &edge_set {
			*in_degree.entry(to).or_insert(0) += 1;
			adj.entry(from).or_default().push(to);
		}
		
		let mut queue: Vec<PassId> = in_degree.iter().filter(|&(_, &deg)| deg == 0).map(|(&id, _)| id).collect();
		
		queue.sort_by(|a, b| b.cmp(a));
		
		let mut ordered = Vec::with_capacity(pass_count);
		
		while let Some(pass_id) = queue.pop() {
			ordered.push(pass_id);
			
			if let Some(neighbors) = adj.get(&pass_id) {
				for &next in neighbors {
					let deg = in_degree.get_mut(&next).unwrap();
					*deg -= 1;
					if *deg == 0 {
						let pos = queue.partition_point(|&x| x > next);
						queue.insert(pos, next);
					}
				}
			}
		}
		
		if ordered.len() != pass_count {
			return Err(GraphError::CycleDetected);
		}
		
		Ok(ExecutionOrder {
			ordered_passes: ordered,
			barriers,
		})
	}
	
	#[inline]
	pub fn resources(&self) -> &[ResourceDecl] {
		&self.resources
	}
	
	#[inline]
	pub fn passes(&self) -> &[PassDecl] {
		&self.passes
	}
	
	pub fn passes_mut(&mut self) -> &mut [PassDecl] {
		&mut self.passes
	}
	
	#[inline]
	pub fn next_resource_id(&self) -> ResourceId {
		self.next_resource_id
	}
	
	#[inline]
	pub fn next_pass_id(&self) -> PassId {
		self.next_pass_id
	}
}

impl Default for FrameGraph{
	fn default() -> Self {
		Self::new()
	}
}

/*
NOTE:
Builder: 'a lifetime ties to mutable borrow of FrameGraph for construction.
PassDecl: commands are plain data — no closures, no lifetime constraints.
Executor: borrows CompiledGraph and replays PassCommands on command buffers.
 */

//---------------------new compiled Graph ----------------------

// Removed:  'f lifetime, record: PassRecord
// Replaced: commands: Vec<PassCommand>
// Changed:  descriptor_set: Option<DescriptorSetId> (opaque id, not raw handle)
pub struct CompiledPass {
	pub id: PassId,
	pub pipeline: Option<PipelineId>,
	pub descriptor_set: Option<DescriptorSetId>,
	pub domain: PassDomain,
	pub commands: Vec<PassCommand>,
}

// Removed: 'f lifetime
// Added:   descriptor_sets: Vec<B::DescriptorSet> (registry carried from FrameGraph)
pub struct CompiledGraph {
	pub order: ExecutionOrder,
	pub passes: Vec<CompiledPass>,
	pub resources: Vec<ResourceDecl>,
}

impl  FrameGraph {
	
	/// Full compilation step.
	///
	/// Consumes the FrameGraph (IR) and produces an executable graph.
	///
	/// Phase 1: dependency + barrier analysis
	/// Phase 2: lowering PassDecl → CompiledPass
	pub fn compile(self) -> Result<CompiledGraph, GraphError> {
		// ─────────────────────────────────────────────
		// Phase 1: dependency compilation
		// ─────────────────────────────────────────────
		let order = self.compile_dependencies()?;
		
		// ─────────────────────────────────────────────
		// Phase 2: lowering — just moves, no .take()
		// ─────────────────────────────────────────────
		let compiled_passes = self.passes.into_iter().map(|decl| {
			CompiledPass {
				id: decl.id,
				pipeline: decl.pipeline,
				descriptor_set: decl.descriptor_set,
				domain: decl.domain,
				commands: decl.commands,
			}
		}).collect();
		
		Ok(CompiledGraph {
			order,
			passes: compiled_passes,
			resources: self.resources,
		})
	}
}