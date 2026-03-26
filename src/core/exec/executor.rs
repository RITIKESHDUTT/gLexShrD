use crate::core::types::{Offset2D, Extent3D};
use crate::domain::DescriptorSetId;
use crate::core::types::IndexType;
use {
	crate::{
		core::{
			backend::{
				types::{Extent2D, ImageAspect, PipelineBindPoint},
				Backend, BufferBarrierInfo2, CommandOps,
				ImageBarrierInfo, SemaphoreSubmit,
			},
			barrier::{resolve_barrier, BarrierDesc},
			exec::{
				command::PassCommand,
				frame::{BarrierEdge, CompiledPass},
				lane::WorkLane,
				ExecutionOrder,
				FrameGraph,
				PipelineManager,
			},
			render::RenderingInfoBuilder,
			resource::{img_state, ImageView, SwapchainImage},
			type_state_queue::Queue,
			type_state_queue::{Compute, Graphics, Transfer},
		},
		domain::{
			Access, GraphError, PassDomain, PassId,
			ResourceDecl, ResourceHandle, Stage,
		},
	},
	std::collections::HashMap,
};

pub struct Executor<'dev, B: Backend> {
	device: &'dev B::Device,
	graphics: Option<WorkLane<'dev, Queue<Graphics, B>, B>>,
	compute: Option<WorkLane<'dev, Queue<Compute, B>, B>>,
	transfer: Option<WorkLane<'dev, Queue<Transfer, B>, B>>,
	descriptor_sets: Vec<B::DescriptorSet>,
}

pub struct RenderTarget<'a, B: Backend> {
	pub color_view: &'a ImageView<'a, B>,
	pub extent: Extent2D,
	pub clear_color: [f32; 4],
}

#[derive(Debug)]
pub struct PresentSync<B: Backend> {
	pub wait_acquire: B::Semaphore,
	pub signal_render_finished: B::Semaphore,
}
impl<B: Backend> Copy for PresentSync<B> {}
impl<B: Backend> Clone for PresentSync<B> {
	fn clone(&self) -> Self { *self }
}
fn build_image_barrier<B: Backend>(desc: &BarrierDesc, image: B::Image) -> ImageBarrierInfo<B> {
	ImageBarrierInfo {
		image,
		old_layout: desc.src_usage.layout(),
		new_layout: desc.dst_usage.layout(),
		src_stage: desc.src_usage.stage(),
		src_access: desc.src_usage.access(),
		dst_stage: desc.dst_usage.stage(),
		dst_access: desc.dst_usage.access(),
		aspect: ImageAspect::Color,
		src_queue_family: desc.src_queue_family,
		dst_queue_family: desc.dst_queue_family,
	}
}

fn build_buffer_barrier<B: Backend>(desc: &BarrierDesc, buffer: B::Buffer) -> BufferBarrierInfo2<B> {
	BufferBarrierInfo2 {
		buffer,
		src_stage: desc.src_usage.stage().into(),
		src_access: desc.src_usage.access().into(),
		dst_stage: desc.dst_usage.stage().into(),
		dst_access: desc.dst_usage.access().into(),
		src_queue_family: desc.src_queue_family.into(),
		dst_queue_family: desc.dst_queue_family.into(),
	}
}

#[derive(PartialEq)]
enum BarrierSide { Acquire, Release }

fn collect_barriers<B: Backend>(
	side: BarrierSide,
	barriers: &[BarrierEdge],
	pass_id: PassId,
	resources: &[ResourceDecl],
	family_for: impl Fn(PassDomain) -> u32,
) -> (Vec<ImageBarrierInfo<B>>, Vec<BufferBarrierInfo2<B>>) {
	let mut imgs = Vec::new();
	let mut bufs = Vec::new();
	
	for b in barriers {
		match side {
			BarrierSide::Acquire if b.to_pass != pass_id => continue,
			BarrierSide::Release if b.from_pass != pass_id => continue,
			_ => {}
		}
		
		let resolved = resolve_barrier(b, &family_for);
		let is_cross = resolved.src_queue_family != resolved.dst_queue_family;
		
		if side == BarrierSide::Release && !is_cross {
			continue;
		}
		
		let res = resources.iter().find(|r| r.id == resolved.resource).expect("resource missing");
		
		match res.handle {
			ResourceHandle::Image { raw, .. } => {
				let mut bar = build_image_barrier::<B>(
					&resolved,
					B::image_from_raw(raw),
				);
				
				match side {
					BarrierSide::Acquire if is_cross => {
						bar.src_stage = Stage::None;
						bar.src_access = Access::None;
					}
					BarrierSide::Release => {
						bar.dst_stage = Stage::None;
						bar.dst_access = Access::None;
					}
					_ => {}
				}
				
				imgs.push(bar);
			}
			
			ResourceHandle::Buffer { raw, .. } => {
				let mut bar = build_buffer_barrier::<B>(
					&resolved,
					B::buffer_from_raw(raw),
				);
				
				match side {
					BarrierSide::Acquire if is_cross => {
						bar.src_stage = Stage::None.into();
						bar.src_access = Access::None.into();
					}
					BarrierSide::Release => {
						bar.dst_stage = Stage::None.into();
						bar.dst_access = Access::None.into();
					}
					_ => {}
				}
				
				bufs.push(bar);
			}
		}
	}
	
	(imgs, bufs)
}

fn present_sync_submits<B: Backend>(
	sync: &Option<PresentSync<B>>,
	is_first: bool,
	is_last: bool,
) -> (Vec<SemaphoreSubmit<B>>, Vec<SemaphoreSubmit<B>>) {
	let mut waits = Vec::new();
	let mut signals = Vec::new();
	
	if let Some(ps) = sync {
		if is_first {
			waits.push(SemaphoreSubmit {
				semaphore: ps.wait_acquire,
				value: 0,
				stage: Stage::ColorOutput,
			});
		}
		if is_last {
			signals.push(SemaphoreSubmit {
				semaphore: ps.signal_render_finished,
				value: 0,
				stage: Stage::All,
			});
		}
	}
	
	(waits, signals)
}
fn merge_stage(a: Stage, b: Stage) -> Stage {
	if a == b { a } else { b }
}
fn resolve_buffer<B: Backend>(
	resources: &[ResourceDecl],
	id: u32,
) -> (B::Buffer, u64) {
	let res = resources.iter()
					   .find(|r| r.id == id)
					   .expect("resource not found");
	
	match res.handle {
		ResourceHandle::Buffer { raw, offset, .. } => {
			(B::buffer_from_raw(raw), offset)
		}
		_ => panic!("expected buffer"),
	}
}
fn resolve_image<B: Backend>(
	resources: &[ResourceDecl],
	id: u32,
) -> (B::Image, Extent3D) {
	let res = resources.iter()
					   .find(|r| r.id == id)
					   .expect("resource not found");
	
	match res.handle {
		ResourceHandle::Image { raw, extent } => {
			(B::image_from_raw(raw), extent)
		}
		_ => panic!("expected image"),
	}
}
impl<'dev, B: Backend> Executor<'dev, B> {
	pub fn new(device: &'dev B::Device) -> Self {
		Self {
			device,
			graphics: None,
			compute: None,
			transfer: None,
			descriptor_sets: Vec::new(),
		}
	}
	
	pub fn attach_graphics(&mut self, queue: Queue<Graphics, B>) -> Result<(), B::Error> {
		self.graphics = Some(WorkLane::new(self.device, queue)?);
		Ok(())
	}
	
	pub fn attach_compute(&mut self, queue: Queue<Compute, B>) -> Result<(), B::Error> {
		self.compute = Some(WorkLane::new(self.device, queue)?);
		Ok(())
	}
	
	pub fn attach_transfer(&mut self, queue: Queue<Transfer, B>) -> Result<(), B::Error> {
		self.transfer = Some(WorkLane::new(self.device, queue)?);
		Ok(())
	}
	
	pub fn device(&self) -> &'dev B::Device { self.device }
	
	pub fn graphics_lane(&self) -> &WorkLane<'dev, Queue<Graphics, B>, B> {
		self.graphics.as_ref().expect("no graphics lane")
	}
	pub fn graphics_lane_mut(&mut self) -> &mut WorkLane<'dev, Queue<Graphics, B>, B> {
		self.graphics.as_mut().expect("no graphics lane")
	}
	pub fn compute_lane(&self) -> &WorkLane<'dev, Queue<Compute, B>, B> {
		self.compute.as_ref().expect("no compute lane")
	}
	pub fn compute_lane_mut(&mut self) -> &mut WorkLane<'dev, Queue<Compute, B>, B> {
		self.compute.as_mut().expect("no compute lane")
	}
	pub fn transfer_lane(&self) -> &WorkLane<'dev, Queue<Transfer, B>, B> {
		self.transfer.as_ref().expect("no transfer lane")
	}
	pub fn transfer_lane_mut(&mut self) -> &mut WorkLane<'dev, Queue<Transfer, B>, B> {
		self.transfer.as_mut().expect("no transfer lane")
	}
	pub fn has_transfer(&self) -> bool { self.transfer.is_some() }
	pub fn has_compute(&self) -> bool { self.compute.is_some() }
	
	fn timeline_handle_for(&self, domain: PassDomain) -> B::Semaphore {
		match domain {
			PassDomain::Graphics => self.graphics_lane().timeline_handle(),
			PassDomain::Compute => self.compute_lane().timeline_handle(),
			PassDomain::Transfer => self.transfer_lane().timeline_handle(),
		}
	}
	
	fn family_for(&self, domain: PassDomain) -> u32 {
		match domain {
			PassDomain::Graphics => self.graphics_lane().family(),
			PassDomain::Compute => self.compute_lane().family(),
			PassDomain::Transfer => self.transfer_lane().family(),
		}
	}
	
	pub fn graphics_timeline_handle(&self) -> B::Semaphore {
		self.graphics_lane().timeline_handle()
	}
	
	pub fn register_descriptor_set(&mut self, handle: B::DescriptorSet) -> DescriptorSetId {
		let id = DescriptorSetId(self.descriptor_sets.len() as u32);
		self.descriptor_sets.push(handle);
		id
	}
	
	pub fn clear_descriptor_sets(&mut self) {
		self.descriptor_sets.clear();
	}
	
	pub fn execute(
		&mut self,
		graph: FrameGraph,
		swap_img: SwapchainImage<'dev, img_state::Undefined, B>,
		target: RenderTarget<'_, B>,
		pipelines: &PipelineManager<'dev, B>,
		present_sync: Option<PresentSync<B>>,
	) -> Result<u64, GraphError> {
		let compiled = graph.compile()?;
		for pass in &compiled.passes {
			match pass.domain {
				PassDomain::Compute if self.compute.is_none() => {
					return Err(GraphError::MissingLane(pass.domain));
				}
				PassDomain::Transfer if self.transfer.is_none() => {
					return Err(GraphError::MissingLane(pass.domain));
				}
				_ => {}
			}
		}
		let ExecutionOrder { ordered_passes, barriers } = compiled.order;
		let mut passes: HashMap<PassId, CompiledPass> = compiled.passes.into_iter().map(|p| (p.id, p)).collect();
		let resources = compiled.resources;
		let descriptor_sets = &self.descriptor_sets;
		let mut final_graphics_val: u64 = 0;
		
		let mut swap_img: Option<SwapchainImage<'dev, img_state::Undefined, B>> = Some(swap_img);
		let mut color_img: Option<SwapchainImage<'dev, img_state::ColorAttachment, B>> = None;
		let mut pass_signals: HashMap<PassId, (u64, PassDomain)> = HashMap::new();
		
		let first_gfx = ordered_passes.iter().find(|&&pid| passes.get(&pid).map(|p| p.domain == PassDomain::Graphics).unwrap_or(false)).copied();
		let last_gfx = ordered_passes.iter().rev().find(|&&pid| passes.get(&pid).map(|p| p.domain == PassDomain::Graphics).unwrap_or(false)).copied();
		
		for &pass_id in &ordered_passes {
			let pass = passes.remove(&pass_id).expect("compiled pass missing");
			
			let domain = pass.domain;
			let pass_pipeline = pass.pipeline;
			let pass_viewport = pass.viewport;
			let pass_scissor = pass.scissor;
			let stage = pass
				.commands
				.first()
				.map(|_| match domain {
					PassDomain::Graphics => Stage::ColorOutput,
					PassDomain::Compute  => Stage::Compute,
					PassDomain::Transfer => Stage::Transfer,
				})
				.unwrap_or(Stage::Bottom);
			let commands = pass.commands;
			
			let mut waits: Vec<(B::Semaphore, u64, Stage)> = Vec::new();
			
			let (lane_sem, lane_prev) = match domain {
				PassDomain::Graphics => {
					let l = self.graphics.as_ref().expect("no graphics lane");
					(l.timeline_handle(), l.last_signal_value())
				}
				PassDomain::Compute => {
					let l = self.compute.as_ref().expect("no compute lane");
					(l.timeline_handle(), l.last_signal_value())
				}
				PassDomain::Transfer => {
					let l = self.transfer.as_ref().expect("no transfer lane");
					(l.timeline_handle(), l.last_signal_value())
				}
			};
		
			if lane_prev > 0 {
				waits.push((lane_sem, lane_prev, stage));
			}
			
			for barrier in &barriers {
				if barrier.to_pass == pass_id {
					if let Some(&(src_val, src_domain)) = pass_signals.get(&barrier.from_pass) {
						if src_domain != domain {
							let src_sem = self.timeline_handle_for(src_domain);
							
							let stage = barrier.dst_usage.stage();
							
							if let Some(existing) = waits.iter_mut().find(|(s, _, _)| *s == src_sem) {
								existing.1 = existing.1.max(src_val);
								existing.2 = merge_stage(existing.2, stage);
							} else {
								waits.push((src_sem, src_val, stage));
							}
						}
					}
				}
			}
			
			let (img_bars, buf_bars) = collect_barriers::<B>(
				BarrierSide::Acquire, &barriers, pass_id, &resources, |d| self.family_for(d),
			);
			let (rel_img, rel_buf) = collect_barriers::<B>(
				BarrierSide::Release, &barriers, pass_id, &resources, |d| self.family_for(d),
			);
			
			let is_first = first_gfx == Some(pass_id);
			let is_last = last_gfx == Some(pass_id);
			
			let (bin_waits, bin_signals) = if domain == PassDomain::Graphics {
				present_sync_submits(&present_sync, is_first, is_last)
			} else {
				(Vec::new(), Vec::new())
			};
			
			let signal_val = match domain {
				PassDomain::Graphics => {
					let lane = self.graphics.as_mut().expect("no graphics lane");
					let cmd = lane.allocate().map_err(|e| GraphError::backend(e))?.begin().map_err(|e|
						GraphError::backend(e))?;
					
					if !img_bars.is_empty() || !buf_bars.is_empty() {
						cmd.image_barrier(&img_bars);
						cmd.buffer_barrier(&buf_bars);
					}
					
					if is_first {
						let si = swap_img.take().expect("first gfx pass but swapchain already taken");
						color_img = Some(si.into_color_attachment(&cmd));
					}
					
					let mut builder = RenderingInfoBuilder::new(
						Offset2D::new(0, 0),
						target.extent,
					);
					
					builder = if is_first {
						builder.color_clear(target.color_view, target.clear_color)
					} else {
						builder.color_load(target.color_view)
					};
					
					let rendering_info = builder.build();
					let inside = cmd.begin_rendering(&rendering_info);
					
					match pass_viewport {
						Some((x, y, w, h)) => inside.set_viewport_rect(x, y, w, h),
						None => inside.set_viewport_rect(0.0, 0.0, target.extent.width() as f32, target.extent.height() as f32),
					}
					
					match pass_scissor {
						Some((x, y, w, h)) => inside.set_scissor_rect(x, y, w, h),
						None => inside.set_scissor_rect(0, 0, target.extent.width(), target.extent.height()),
					}
					
					let mut active_layout: Option<B::PipelineLayout> = None;
					
					if let Some(id) = pass_pipeline {
						inside.bind_graphics_pipeline(pipelines.handle(id));
						active_layout = Some(pipelines.layout(id));
					}
					
					for command in &commands {
						match command {
							PassCommand::BindPipeline(id) => {
								inside.bind_graphics_pipeline(pipelines.handle(*id));
								active_layout = Some(pipelines.layout(*id));
							}
							PassCommand::Draw { vertex_count } => {
								inside.draw(*vertex_count);
							}
							PassCommand::DrawIndexed { index_count, instance_count, first_index } => {
								inside.draw_indexed(*index_count, *instance_count, *first_index);
							}
							PassCommand::BindVertexBuffer(res_id, offset) => {
								let (raw, base_offset) = resolve_buffer::<B>(&resources, *res_id);
								
								inside.device().cmd_bind_vertex_buffers(
									inside.handle(),
									0,
									&[raw],
									&[base_offset + *offset],
								);
							}
							
							PassCommand::BindIndexBuffer(res_id, offset) => {
								let (raw, base_offset) = resolve_buffer::<B>(&resources, *res_id);
								
								inside.device().cmd_bind_index_buffer(
									inside.handle(),
									raw,
									base_offset + *offset,
									IndexType::U32,
								);
							}
							PassCommand::BindDescriptorSet(set_id) => {
								let raw = descriptor_sets[set_id.0 as usize];
								let layout = active_layout.expect("no pipeline bound before descriptor set");
								inside.device().cmd_bind_descriptor_sets(inside.handle(), PipelineBindPoint::GRAPHICS, layout, 0, &[raw], &[]);
							}
							PassCommand::PushConstants { range, data } => {
								let layout = active_layout.expect("no pipeline bound before push constants");
								inside.device().cmd_push_constants(
									inside.handle(), layout,
									range.stages, range.offset,
									&data[..range.size as usize],
								);
							}
							PassCommand::CopyBuffer { src, dst, size, dst_offset } => {
								let (src_buf, src_base) = resolve_buffer::<B>(&resources, *src);
								let (dst_buf, dst_base) = resolve_buffer::<B>(&resources, *dst);
								
								inside.device().cmd_copy_buffer(
									inside.handle(),
									src_buf,
									dst_buf,
									src_base,
									dst_base + *dst_offset,
									*size,
								);
							}
							
							// REQUIRED
							PassCommand::CopyBufferToImage { src, dst } => {
								let (src_buf, src_base) = resolve_buffer::<B>(&resources, *src);
								let (dst_img, extent)   = resolve_image::<B>(&resources, *dst);
								
								inside.device().cmd_copy_buffer_to_image(
									inside.handle(),
									src_buf,
									src_base,   // FIX: real offset
									dst_img,
									extent.into(),     // FIX: real extent
								);
							}
							PassCommand::Dispatch { .. }  => {
								unreachable!("non-graphics command in graphics pass")
							}
						}
					}
					
					let outside = inside.end_rendering();
					
					if is_last {
						let ci = color_img.take().expect("last gfx pass but no color attachment");
						let _present = ci.into_present_src(&outside);
					}
					
					if !rel_img.is_empty() || !rel_buf.is_empty() {
						outside.image_barrier(&rel_img);
						outside.buffer_barrier(&rel_buf);
					}
					let executable = outside.end().map_err(|e| GraphError::backend(e))?;
					
					if bin_waits.is_empty() && bin_signals.is_empty() {
						lane.submit(self.device, executable, &waits).map_err(|e| GraphError::backend(e))?
					} else {
						lane.submit_with_binary(
							self.device, executable, &waits, &bin_waits, &bin_signals, ).map_err(|e| GraphError::backend(e))?
					}
				}
				
				PassDomain::Compute => {
					let lane = self.compute.as_mut().expect("no compute lane");
					let cmd = lane.allocate().map_err(|e| GraphError::backend(e))?.begin().map_err(|e|
						GraphError::backend(e))?;
					
					if !img_bars.is_empty() || !buf_bars.is_empty() {
						cmd.image_barrier(&img_bars);
						cmd.buffer_barrier(&buf_bars);
					}
					
					let mut active_layout: Option<B::PipelineLayout> = None;
					if let Some(id) = pass_pipeline {
						cmd.bind_compute_pipeline(pipelines.handle(id));
						active_layout = Some(pipelines.layout(id));
					}
					
					for command in &commands {
						match command {
							PassCommand::BindPipeline(id) => {
								cmd.bind_compute_pipeline(pipelines.handle(*id));
								active_layout = Some(pipelines.layout(*id));
							}
							PassCommand::Dispatch { x, y, z } => {
								cmd.dispatch(*x, *y, *z);
							}
							PassCommand::BindDescriptorSet(set_id) => {
								let raw = descriptor_sets[set_id.0 as usize];
								let layout = active_layout.expect("no pipeline bound before descriptor set");
								cmd.device().cmd_bind_descriptor_sets(
									cmd.handle(), PipelineBindPoint::COMPUTE,
									layout, 0, &[raw], &[],
								);
							}
							PassCommand::PushConstants { range, data } => {
								let layout = active_layout.expect("no pipeline bound before push constants");
								cmd.device().cmd_push_constants(
									cmd.handle(), layout,
									range.stages, range.offset,
									&data[..range.size as usize],
								);
							}
							_ => unreachable!("non-compute command in compute pass"),
						}
					}
					
					if !rel_img.is_empty() || !rel_buf.is_empty() {
						cmd.image_barrier(&rel_img);
						cmd.buffer_barrier(&rel_buf);
					}
					
					let executable = cmd.end().map_err(|e| GraphError::backend(e))?;
					lane.submit(self.device, executable, &waits).map_err(|e| GraphError::backend(e))?
				}
				
				PassDomain::Transfer => {
					let lane = self.transfer.as_mut().expect("no transfer lane");
					let cmd = lane.allocate().map_err(|e| GraphError::backend(e))?.begin().map_err(|e|
						GraphError::backend(e))?;
					
					if !img_bars.is_empty() || !buf_bars.is_empty() {
						cmd.image_barrier(&img_bars);
						cmd.buffer_barrier(&buf_bars);
					}
					
					for command in &commands {
						match command {
							PassCommand::CopyBuffer { src, dst, size, dst_offset } => {
								let (src_buf, src_base) = resolve_buffer::<B>(&resources, *src);
								let (dst_buf, dst_base) = resolve_buffer::<B>(&resources, *dst);
								
								cmd.device().cmd_copy_buffer(
									cmd.handle(),
									src_buf,
									dst_buf,
									src_base,
									dst_base + *dst_offset,
									*size,
								);
							}
							_ => unreachable!("non-transfer command in transfer pass"),
						}
					}
					
					if !rel_img.is_empty() || !rel_buf.is_empty() {
						cmd.image_barrier(&rel_img);
						cmd.buffer_barrier(&rel_buf);
					}
					let executable = cmd.end().map_err(|e| GraphError::backend(e))?;
					lane.submit(self.device, executable, &waits).map_err(|e| GraphError::backend(e))?
				}
			};
			
			pass_signals.insert(pass_id, (signal_val, domain));
			if domain == PassDomain::Graphics {
				final_graphics_val = signal_val;
			}
		}
		
		if first_gfx.is_none() {
			if let Some(ps) = &present_sync {
				let lane = self.graphics.as_mut().expect("no graphics lane");
				let cmd = lane.allocate().map_err(|e| GraphError::backend(e))?.begin().map_err(|e| GraphError::backend(e))?;
				
				let si = swap_img.take().expect("swapchain image not consumed");
				let ci = si.into_color_attachment(&cmd);
				
				let mut builder = RenderingInfoBuilder::new(
					Offset2D::new(0, 0),
					target.extent,
				);
				builder = builder.color_clear(target.color_view, target.clear_color);
				let rendering_info = builder.build();
				let inside = cmd.begin_rendering(&rendering_info);
				let cmd = inside.end_rendering();
				
				let _pi = ci.into_present_src(&cmd);
				
				let executable = cmd.end().map_err(|e| GraphError::backend(e))?;
				
				let waits = [SemaphoreSubmit { semaphore: ps.wait_acquire, value: 0, stage: Stage::ColorOutput }];
				let signals = [SemaphoreSubmit { semaphore: ps.signal_render_finished, value: 0, stage: Stage::Bottom }];
				final_graphics_val = lane.submit_with_binary(
					self.device, executable, &[], &waits, &signals,
				).map_err(|e| GraphError::backend(e))?;
			}
		}
		Ok(final_graphics_val)
	}
}