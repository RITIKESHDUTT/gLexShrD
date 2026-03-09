use {
	crate::domain::{
		GraphError,
		PassDomain,
		PassId,
		ResourceDecl,
		ResourceHandle,
		ResourceId,
		ResourceKind,
		UsageIntent,
	},
	crate::core::{
		backend::Backend,
		exec::{
			recorder::PassRecord,
			ComputeRecorder,
			RenderRecorder2D,
			TransferRecorder,
		},
		type_state_queue::{Compute, Graphics, Transfer},
	},
	std::{
		collections::{HashMap, HashSet},
		marker::PhantomData},
};
use crate::core::render::cache::PipelineId;

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



pub struct PassDecl<'f, B: Backend> {
	pub id: PassId,
	pub pipeline: Option<PipelineId>,
	pub reads: Vec<(ResourceId, UsageIntent)>,
	pub writes: Vec<(ResourceId, UsageIntent)>,
	pub domain: PassDomain,
	pub record: Option<PassRecord<'f, B>>,
}

/// Builder for adding passes to a frame graph.
//The pipeline field in PassBuilder is temporary storage that gets transferred into PassDecl when .build() is called,
//   Option<PipelineId>
pub struct PassBuilder<'a, 'f, D, B: Backend> {
	graph: &'a mut FrameGraph<'f, B>,
	id: PassId,
	pipeline: Option<PipelineId>,
	reads: Vec<(ResourceId, UsageIntent)>,
	writes: Vec<(ResourceId, UsageIntent)>,
	_domain: PhantomData<D>,
}

impl<'f, D, B: Backend> PassBuilder<'_, 'f, D, B> {
	pub fn reads(mut self, resource: ResourceId, usage: UsageIntent) -> Self {
		self.reads.push((resource, usage));
		self
	}
	pub fn writes(mut self, resource: ResourceId, usage: UsageIntent) -> Self {
		self.writes.push((resource, usage));
		self
	}
	
}

impl<'f, B: Backend> PassBuilder<'_, 'f, Graphics, B> {
	pub fn build<F>(self, record: F) -> PassId
					where F: for<'a, 'dev> FnOnce(&'a mut RenderRecorder2D<'a, 'dev, B>) + 'f,
	{
		let id = self.id;
		self.graph.passes.push(PassDecl {
			id,
			pipeline: self.pipeline,
			reads: self.reads,
			writes: self.writes,
			domain: PassDomain::Graphics,
			record: Some(PassRecord::Graphics(Box::new(record))),
		});
		id
	}
}
impl<'f, B: Backend> PassBuilder<'_, 'f, Compute, B> {
	pub fn build<F>(self, record: F) -> PassId
					where F: for<'a, 'dev> FnOnce(&'a mut ComputeRecorder<'a, 'dev, B>) + 'f,

	{
		let id = self.id;
		self.graph.passes.push(PassDecl {
			id,
			pipeline:self.pipeline,
			reads: self.reads,
			writes: self.writes,
			domain: PassDomain::Compute,
			record: Some(PassRecord::Compute(Box::new(record))),
		});
		id
	}
}

impl<'f, B: Backend> PassBuilder<'_, 'f, Transfer, B> {
	pub fn build<F>(self, record: F) -> PassId
					where F: for<'a, 'dev> FnOnce(&'a mut TransferRecorder<'a, 'dev, B>) + 'f,

	{
		let id = self.id;
		self.graph.passes.push(PassDecl {
			id,
			pipeline: self.pipeline,
			reads: self.reads,
			writes: self.writes,
			domain: PassDomain::Transfer,
			record: Some(PassRecord::Transfer(Box::new(record))),
		});
		id
	}
}

/// Declarative frame graph. Resources and passes are declared,
/// then compiled into an execution order with automatic barrier placement.
pub struct FrameGraph<'f, B: Backend> {
	resources: Vec<ResourceDecl>,
	passes: Vec<PassDecl<'f, B>>,
	next_resource_id: ResourceId,
	next_pass_id: PassId,
}

impl<'f, B: Backend> FrameGraph<'f, B> {
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
	
	pub fn resource(&self, id: ResourceId) -> &ResourceDecl {
		self.resources.iter().find(|r| r.id == id).expect("resource not found")
	}
	
	pub fn add_graphics_pass(&mut self, pipeline: Option<PipelineId>) -> PassBuilder<'_, 'f, Graphics, B> {
		let id = self.next_pass_id;
		self.next_pass_id += 1;
		PassBuilder {
			graph: self,
			id,
			pipeline,
			reads: Vec::new(),
			writes: Vec::new(),
			_domain: PhantomData
		}
	}
	
	pub fn add_compute_pass(&mut self, pipeline: Option<PipelineId>) -> PassBuilder<'_, 'f, Compute, B> {
		let id = self.next_pass_id;
		self.next_pass_id += 1;
		PassBuilder {
			graph: self,
			id,
			pipeline,
			reads: Vec::new(),
			writes: Vec::new(),
			_domain: PhantomData
		}
	}
	
	
	pub fn add_transfer_pass(&mut self) -> PassBuilder<'_, 'f, Transfer, B> {
		let id = self.next_pass_id;
		self.next_pass_id += 1;
		PassBuilder {
			graph: self,
			id,
			pipeline: None,
			reads: Vec::new(),
			writes: Vec::new(),
			_domain: PhantomData
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
	pub fn passes(&self) -> &[PassDecl<'f, B>] {
		&self.passes
	}
	
	pub fn passes_mut(&mut self) -> &mut [PassDecl<'f, B>] {
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

impl<B: Backend> Default for FrameGraph<'_, B> {
	fn default() -> Self {
		Self::new()
	}
}


/*

NOTE:
Builder: 'a lifetime ties to mutable borrow of FrameGraph for construction.
PassDecl: closure must be 'static so it can outlive the builder, but it still works with CommandBuffer lifetimes (for<'cmd>).
Executor: borrows FrameGraph immutably and runs closures on command buffers.
 */

//---------------------new compiled Graph ----------------------

pub struct CompiledPass<'f, B: Backend> {
	pub id: PassId,
	pub pipeline: Option<PipelineId>,
	pub descriptor_set: Option<B::DescriptorSet>,
	pub domain: PassDomain,
	pub record: PassRecord<'f, B>,
}

pub struct CompiledGraph<'f, B: Backend> {
	pub order: ExecutionOrder,
	pub passes: Vec<CompiledPass<'f, B>>,
	pub resources: Vec<ResourceDecl>,
}

impl<'f, B: Backend> FrameGraph<'f, B> {
	
	/// Full compilation step.
	///
	/// Consumes the FrameGraph (IR) and produces an executable graph.
	///
	/// Phase 1: dependency + barrier analysis
	/// Phase 2: lowering PassDecl → CompiledPass
	pub fn compile<'dev>(
		self,
	) -> Result<CompiledGraph<'f, B>, GraphError>
		where
			B: 'dev,
	{
		// ─────────────────────────────────────────────
		// Phase 1: dependency compilation
		// ─────────────────────────────────────────────
		let order = self.compile_dependencies()?;
		
		// ─────────────────────────────────────────────
		// Phase 2: lowering
		// ─────────────────────────────────────────────
		let mut compiled_passes = Vec::with_capacity(self.passes.len());
		
		for mut decl in self.passes {
			let record = decl
				.record
				.take()
				.expect("PassDecl record closure already consumed");
			
			compiled_passes.push(CompiledPass {
				id: decl.id,
				pipeline: decl.pipeline,
				descriptor_set: None, // ← descriptor lowering stage can fill this later
				domain: decl.domain,
				record,
			});
		}
		
		Ok(CompiledGraph {
			order,
			passes: compiled_passes,
			resources: self.resources,
		})
	}
}