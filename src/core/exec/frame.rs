use crate::core::exec::command::PassCommand;
use crate::core::render::cache::PipelineId;
use {
	crate::core::{
		type_state_queue::{Compute, Graphics, Transfer},
		types::{PushConstantRange, Extent3D},
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

use tracing::{debug, info, instrument, trace, warn};

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
		trace!(
                        pass_id = self.id,
                        resource,
                        stage = ?usage.stage(),
                        access = ?usage.access(),
                        "PassBuilder::reads"
                );
		self.reads.push((resource, usage));
		self
	}
	
	pub fn writes(mut self, resource: ResourceId, usage: UsageIntent) -> Self {
		trace!(
                        pass_id = self.id,
                        resource,
                        stage = ?usage.stage(),
                        access = ?usage.access(),
                        "PassBuilder::writes"
                );
		self.writes.push((resource, usage));
		self
	}
	
	pub fn descriptor_set(mut self, id: DescriptorSetId) -> Self {
		trace!(pass_id = self.id, ?id, "PassBuilder::descriptor_set");
		self.descriptor_set = Some(id);
		self
	}
	
	pub fn push_constants(mut self, range: PushConstantRange, data: &[u8]) -> Self {
		trace!(
                        pass_id = self.id,
                        offset = range.offset,
                        size = range.size,
                        data_len = data.len(),
                        "PassBuilder::push_constants"
                );
		self.commands.push(PassCommand::PushConstants {
			range,
			data: data.into(),
		});
		self
	}
	
	pub fn submit(self) -> PassId {
		self.validate_resource_declarations();
		let id = self.id;
		debug!(
                        pass_id = id,
                        domain = ?self.domain,
                        pipeline = ?self.pipeline,
                        read_count = self.reads.len(),
                        write_count = self.writes.len(),
                        command_count = self.commands.len(),
                        has_viewport = self.viewport.is_some(),
                        has_scissor = self.scissor.is_some(),
                        "PassBuilder::submit — finalizing pass"
                );
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
				PassCommand::BindVertexBuffer(id, _) | PassCommand::BindIndexBuffer(id, _) => {
					if !self.reads.iter().any(|(r, _)| r == id) {
						warn!(
                                                        pass_id = self.id,
                                                        resource = ?id,
                                                        "Resource bound in command but not declared in reads()"
                                                );
					}
					debug_assert!(
						self.reads.iter().any(|(r, _)| r == id),
						"Resource {id:?} bound in command but not declared in reads()"
					);
				}
				PassCommand::CopyBuffer { src, dst, .. } => {
					if !self.reads.iter().any(|(r, _)| r == src) {
						warn!(
                                                        pass_id = self.id,
                                                        resource = ?src,
                                                        "Copy source not declared in reads()"
                                                );
					}
					if !self.writes.iter().any(|(r, _)| r == dst) {
						warn!(
                                                        pass_id = self.id,
                                                        resource = ?dst,
                                                        "Copy destination not declared in writes()"
                                                );
					}
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
		trace!(pass_id = self.id, ?id, "Graphics::bind_pipeline");
		self.commands.push(PassCommand::BindPipeline(id));
		self
	}
	
	pub fn draw(mut self, vertex_count: u32) -> Self {
		trace!(pass_id = self.id, vertex_count, "Graphics::draw");
		self.commands.push(PassCommand::Draw { vertex_count });
		self
	}
	
	pub fn draw_indexed(mut self, index_count: u32, instance_count: u32, first_index: u32) -> Self {
		trace!(
                        pass_id = self.id,
                        index_count,
                        instance_count,
                        first_index,
                        "Graphics::draw_indexed"
                );
		self.commands.push(PassCommand::DrawIndexed { index_count, instance_count, first_index });
		self
	}
	
	pub fn bind_vertex_buffer(mut self, resource: ResourceId, offset: u64) -> Self {
		trace!(pass_id = self.id, resource, offset, "Graphics::bind_vertex_buffer");
		self.commands.push(PassCommand::BindVertexBuffer(resource, offset));
		self
	}
	
	pub fn bind_index_buffer(mut self, resource: ResourceId, offset: u64) -> Self {
		trace!(pass_id = self.id, resource, offset, "Graphics::bind_index_buffer");
		self.commands.push(PassCommand::BindIndexBuffer(resource, offset));
		self
	}
	
	pub fn bind_descriptor_set(mut self, id: DescriptorSetId) -> Self {
		trace!(pass_id = self.id, ?id, "Graphics::bind_descriptor_set");
		self.commands.push(PassCommand::BindDescriptorSet(id));
		self
	}
	pub fn set_viewport(mut self, x: f32, y: f32, w: f32, h: f32) -> Self {
		trace!(pass_id = self.id, x, y, w, h, "Graphics::set_viewport");
		self.viewport = Some((x, y, w, h));
		self
	}
	
	pub fn set_scissor(mut self, x: i32, y: i32, w: u32, h: u32) -> Self {
		trace!(pass_id = self.id, x, y, w, h, "Graphics::set_scissor");
		self.scissor = Some((x, y, w, h));
		self
	}
	
}

// ─── Compute impl ───────────────────────────────────────────

impl PassBuilder<'_, Compute> {
	pub fn bind_pipeline(mut self, id: PipelineId) -> Self {
		trace!(pass_id = self.id, ?id, "Compute::bind_pipeline");
		self.commands.push(PassCommand::BindPipeline(id));
		self
	}
	
	pub fn dispatch(mut self, x: u32, y: u32, z: u32) -> Self {
		trace!(pass_id = self.id, x, y, z, "Compute::dispatch");
		self.commands.push(PassCommand::Dispatch { x, y, z });
		self
	}
	
	pub fn bind_descriptor_set(mut self, id: DescriptorSetId) -> Self {
		trace!(pass_id = self.id, ?id, "Compute::bind_descriptor_set");
		self.commands.push(PassCommand::BindDescriptorSet(id));
		self
	}
}

// ─── Transfer impl ──────────────────────────────────────────

impl PassBuilder<'_, Transfer> {
	pub fn copy_buffer(mut self, src: ResourceId, dst: ResourceId, size: u64, dst_offset: u64) -> Self {
		trace!(
                        pass_id = self.id,
                        src,
                        dst,
                        size,
                        dst_offset,
                        "Transfer::copy_buffer"
                );
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
		trace!("FrameGraph::new");
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
		trace!(resource_id = id, ?kind, ?handle, "FrameGraph::add_resource");
		self.resources.push(ResourceDecl { id, kind, handle });
		id
	}
	pub fn add_image(
		&mut self,
		handle: u64,
		extent: Extent3D,
	) -> ResourceId {
		trace!(handle, ?extent, "FrameGraph::add_image");
		self.add_resource(ResourceKind::Image, ResourceHandle::Image { raw: handle, extent})
	}
	pub fn add_buffer(&mut self, handle: u64, offset: u64, size: u64 ) -> ResourceId {
		trace!(handle, offset, size, "FrameGraph::add_buffer");
		self.add_resource(ResourceKind::Buffer, ResourceHandle::Buffer { raw: handle, offset, size }, )
	}
	pub fn resource(&self, id: ResourceId) -> &ResourceDecl {
		self.resources.iter().find(|r| r.id == id).expect("resource not found")
	}
	
	pub fn add_graphics_pass(&mut self, pipeline: Option<PipelineId>) -> PassBuilder<'_, Graphics> {
		let id = self.next_pass_id;
		self.next_pass_id += 1;
		debug!(pass_id = id, ?pipeline, "FrameGraph::add_graphics_pass");
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
		debug!(pass_id = id, ?pipeline, "FrameGraph::add_compute_pass");
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
		debug!(pass_id = id, "FrameGraph::add_transfer_pass");
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
	
	
	#[instrument(skip_all, name = "FrameGraph::compile_dependencies")]
	pub(crate) fn compile_dependencies(&self) -> Result<ExecutionOrder, GraphError> {
		struct ResourceState {
			last_write: Option<(PassId, UsageIntent, PassDomain)>,
			readers: Vec<(PassId, UsageIntent, PassDomain)>,
		}
		
		debug!(
                        pass_count = self.passes.len(),
                        resource_count = self.resources.len(),
                        "Compiling dependencies"
                );
		
		let mut state: HashMap<ResourceId, ResourceState> = HashMap::new();
		let mut edge_set: HashSet<(PassId, PassId)> = HashSet::new();
		let mut barriers: Vec<BarrierEdge> = Vec::new();
		
		for pass in &self.passes {
			trace!(
                                pass_id = pass.id,
                                domain = ?pass.domain,
                                read_count = pass.reads.len(),
                                write_count = pass.writes.len(),
                                "Processing pass"
                        );
			
			// Reads: need barrier from last writer (RAW)
			for &(res, dst_usage) in &pass.reads {
				let rs = state.entry(res).or_insert_with(|| ResourceState {
					last_write: None,
					readers: Vec::new(),
				});
				
				if let Some((writer_id, writer_usage, writer_domain)) = rs.last_write {
					if writer_id != pass.id && edge_set.insert((writer_id, pass.id)) {
						trace!(
                                                        resource = res,
                                                        from_pass = writer_id,
                                                        to_pass = pass.id,
                                                        src_domain = ?writer_domain,
                                                        dst_domain = ?pass.domain,
                                                        src_stage = ?writer_usage.stage(),
                                                        src_access = ?writer_usage.access(),
                                                        dst_stage = ?dst_usage.stage(),
                                                        dst_access = ?dst_usage.access(),
                                                        "RAW barrier"
                                                );
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
				} else {
					trace!(
                                                resource = res,
                                                pass_id = pass.id,
                                                "Read with no prior writer — no barrier needed"
                                        );
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
						trace!(
                                                        resource = res,
                                                        from_pass = writer_id,
                                                        to_pass = pass.id,
                                                        src_domain = ?writer_domain,
                                                        dst_domain = ?pass.domain,
                                                        "WAW barrier"
                                                );
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
						trace!(
								resource = res,
								from_pass = reader_id,
								to_pass = pass.id,
								src_domain = ?reader_domain,
								dst_domain = ?pass.domain,
								"WAR barrier"
						);
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
		
		trace!(
				roots = queue.len(),
				edge_count = edge_set.len(),
				barrier_count = barriers.len(),
				"Topological sort starting"
                );
		
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
			warn!(
                                ordered = ordered.len(),
                                expected = pass_count,
                                "Cycle detected in frame graph"
                        );
			return Err(GraphError::CycleDetected);
		}
		
		debug!(
				ordered_passes = ?ordered,
				barrier_count = barriers.len(),
				"Dependency compilation complete"
                );
		
		for (i, &pid) in ordered.iter().enumerate() {
			let domain = self.passes.iter().find(|p| p.id == pid).map(|p| &p.domain);
			trace!(order = i, pass_id = pid, domain = ?domain, "Execution order");
		}
		
		for (i, b) in barriers.iter().enumerate() {
			trace!(
					idx = i,
					resource = b.resource,
					from_pass = b.from_pass,
					to_pass = b.to_pass,
					src_domain = ?b.src_domain,
					dst_domain = ?b.dst_domain,
					src_stage = ?b.src_usage.stage(),
					dst_stage = ?b.dst_usage.stage(),
					src_access = ?b.src_usage.access(),
					dst_access = ?b.dst_usage.access(),
					cross_queue = (b.src_domain != b.dst_domain),
					"Barrier summary"
			);
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
	#[instrument(skip_all, name = "FrameGraph::compile")]
	pub fn compile(self) -> Result<CompiledGraph, GraphError> {
		debug!(
                        pass_count = self.passes.len(),
                        resource_count = self.resources.len(),
                        "Compiling frame graph"
                );
		let order = self.compile_dependencies()?;
		
		let compiled_passes: Vec<CompiledPass> = self.passes.into_iter().map(|decl| {
			trace!(
					pass_id = decl.id,
					domain = ?decl.domain,
					pipeline = ?decl.pipeline,
					command_count = decl.commands.len(),
					"Lowering pass"
                        );
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
		
		info!(
				pass_count = compiled_passes.len(),
				barrier_count = order.barriers.len(),
				ordered_passes = ?order.ordered_passes,
				"Frame graph compiled"
                );
		
		Ok(CompiledGraph {
			order,
			passes: compiled_passes,
			resources: self.resources,
		})
	}
}