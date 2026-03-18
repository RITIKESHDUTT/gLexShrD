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

/// The compiled execution order from a frame graph.
#[derive(Debug)]
pub struct ExecutionOrder {
	pub ordered_passes: Vec<PassId>,
	pub barriers: Vec<BarrierEdge>,
}


pub struct PassDecl {
	pub id: PassId,
	pub pipeline: Option<PipelineId>,
	pub descriptor_set: Option<DescriptorSetId>,
	pub reads: Vec<(ResourceId, UsageIntent)>,
	pub writes: Vec<(ResourceId, UsageIntent)>,
	pub domain: PassDomain,
	pub commands: Vec<PassCommand>,
	pub viewport: Option<(f32, f32, f32, f32)>,  // x, y, w, h
	pub scissor:  Option<(i32, i32, u32, u32)>,  // x, y, w, h
}


/// Domain-gated pass builder. `D` restricts available commands at compile time.
pub struct PassBuilder<'a, D> {
	graph: &'a mut FrameGraph,
	id: PassId,
	domain: PassDomain,
	pipeline: Option<PipelineId>,
	descriptor_set: Option<DescriptorSetId>,
	reads: Vec<(ResourceId, UsageIntent)>,
	writes: Vec<(ResourceId, UsageIntent)>,
	commands: Vec<PassCommand>,
	viewport: Option<(f32, f32, f32, f32)>,
	scissor:  Option<(i32, i32, u32, u32)>,
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
		self.commands.push(PassCommand::PushConstants {
			range,
			data: data.into(),
		});
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
			viewport: self.viewport,
			scissor: self.scissor,
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
	pub fn set_viewport(mut self, x: f32, y: f32, w: f32, h: f32) -> Self {
		self.viewport = Some((x, y, w, h));
		self
	}
	
	pub fn set_scissor(mut self, x: i32, y: i32, w: u32, h: u32) -> Self {
		self.scissor = Some((x, y, w, h));
		self
	}

}

// ─── Compute impl ───────────────────────────────────────────

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

impl PassBuilder<'_, Transfer> {
	pub fn copy_buffer(mut self, src: ResourceId, dst: ResourceId, size: u64, dst_offset: u64) -> Self {
		self.commands.push(PassCommand::CopyBuffer { src, dst, size, dst_offset });
		self
	}
}

/// Declarative frame graph. Compiles into execution order with automatic barriers.
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
			scissor:None,
			viewport:None,
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
			scissor:None,
			viewport:None,
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
			scissor:None,
			viewport:None,
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

// ─── Compiled Graph ─────────────────────────────────────────

pub struct CompiledPass {
	pub id: PassId,
	pub pipeline: Option<PipelineId>,
	pub descriptor_set: Option<DescriptorSetId>,
	pub domain: PassDomain,
	pub commands: Vec<PassCommand>,
	pub viewport:       Option<(f32, f32, f32, f32)>,
	pub scissor:        Option<(i32, i32, u32, u32)>,
}

pub struct CompiledGraph {
	pub order: ExecutionOrder,
	pub passes: Vec<CompiledPass>,
	pub resources: Vec<ResourceDecl>,
}

impl  FrameGraph {
	
	/// Consumes the graph IR: dependency analysis → barrier placement → lowering.
	pub fn compile(self) -> Result<CompiledGraph, GraphError> {
		let order = self.compile_dependencies()?;
		let compiled_passes = self.passes.into_iter().map(|decl| {
			CompiledPass {
				id: decl.id,
				pipeline: decl.pipeline,
				descriptor_set: decl.descriptor_set,
				domain: decl.domain,
				commands: decl.commands,
				viewport: decl.viewport,
				scissor: decl.scissor,
			}
		}).collect();
		
		Ok(CompiledGraph {
			order,
			passes: compiled_passes,
			resources: self.resources,
		})
	}
}