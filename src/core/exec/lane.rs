use tracing::{debug, info, instrument, trace, warn};

use crate::{
	core::{
		cmd::{CommandBuffer, CommandPool, Executable, Initial}, sync::TimelineSemaphore, type_state_queue::sealed::QueueHandle,
		Backend,
		DeviceOps,
		SemaphoreSubmit,
	},
	domain::Stage,
};
use crate::domain::UsageIntent;

pub struct WorkLane<'dev, Q: QueueHandle, B: Backend> {
	pool: CommandPool<'dev, B>,
	timeline: TimelineSemaphore<'dev, B>,
	queue: Q,
	last_signal: u64,
}


impl<'dev, Q, B: Backend> WorkLane<'dev, Q, B>
where
	Q: QueueHandle<Handle=B::Queue>,
	B::Device: DeviceOps<B>,
{
	#[instrument(skip_all, name = "WorkLane::new")]
	pub fn new(device: &'dev B::Device, queue: Q) -> Result<Self, B::Error> {
		let family = queue.family();
		debug!(family, "Creating WorkLane");
		let pool = CommandPool::new(device, &queue)?;
		trace!(family, "CommandPool created");
		let timeline = TimelineSemaphore::new(device, 0)?;
		trace!(family, initial_value = 0, "TimelineSemaphore created");
		info!(family, "WorkLane ready");
		Ok(Self {
			pool,
			timeline,
			queue,
			last_signal: 0,
		})
	}
	
	pub fn allocate(&self) -> Result<CommandBuffer<'dev, Initial, B>, B::Error> {
		trace!(
                        family = self.queue.family(),
                        last_signal = self.last_signal,
                        "Allocating command buffer from pool"
                );
		self.pool.allocate()
	}
	
	pub fn last_signal_value(&self) -> u64 {
		self.last_signal
	}
	
	fn next_value(&mut self) -> u64 {
		let prev = self.last_signal;
		self.last_signal += 1;
		trace!(
                        prev,
                        next = self.last_signal,
                        family = self.queue.family(),
                        "Timeline advanced"
                );
		self.last_signal
	}
	
	pub fn timeline_handle(&self) -> B::Semaphore {
		self.timeline.handle()
	}
	
	pub fn timeline(&self) -> &TimelineSemaphore<'dev, B> {
		&self.timeline
	}
	
	pub fn queue_handle(&self) -> B::Queue {
		self.queue.raw()
	}
	
	pub fn family(&self) -> u32 {
		self.queue.family()
	}
	
	#[instrument(skip_all, name = "WorkLane::submit")]
	pub fn submit(
		&mut self,
		device: &B::Device,
		cmd: CommandBuffer<'_, Executable, B>,
		waits: &[(B::Semaphore, u64, Stage)],
	) -> Result<u64, B::Error> {
		let val = self.next_value();
		let wait_values: Vec<u64> = waits.iter().map(|&(_, v, _)| v).collect();
		trace!(
                        signal_value = val,
                        wait_count = waits.len(),
                        ?wait_values,
                        family = self.queue.family(),
                        "Submitting command buffer — timeline waits only"
                );
		let wait_submits: Vec<SemaphoreSubmit<B>> = waits.iter().map(|&(sem, value, stage)| SemaphoreSubmit {
			semaphore: sem,
			value,
			stage,
		}).collect();
		let signals = [SemaphoreSubmit {
			semaphore: self.timeline.handle(),
			value: val,
			stage: Stage::All,
		}];
		device.queue_submit2(self.queue.raw(), Some(cmd.handle()), &wait_submits, &signals)?;
		debug!(
                        signal_value = val,
                        wait_count = waits.len(),
                        family = self.queue.family(),
                        "Submit complete"
                );
		Ok(val)
	}
	
	#[instrument(skip_all, name = "WorkLane::submit_with_binary")]
	pub fn submit_with_binary(
		&mut self,
		device: &B::Device,
		cmd: CommandBuffer<'_, Executable, B>,
		waits: &[(B::Semaphore, u64, Stage)],
		binary_waits: &[SemaphoreSubmit<B>],
		binary_signals: &[SemaphoreSubmit<B>],
	) -> Result<u64, B::Error> {
		let val = self.next_value();
		let wait_values: Vec<u64> = waits.iter().map(|&(_, v, _)| v).collect();
		trace!(
				signal_value = val,
				timeline_wait_count = waits.len(),
				?wait_values,
				binary_wait_count = binary_waits.len(),
				binary_signal_count = binary_signals.len(),
				family = self.queue.family(),
				"Submitting command buffer — timeline + binary semaphores"
                );
		let mut wait_submits: Vec<SemaphoreSubmit<B>> = waits.iter().map(|&(sem, value, stage)| SemaphoreSubmit {
			semaphore: sem,
			value,
			stage,
		}).collect();
		for w in binary_waits {
			wait_submits.push(SemaphoreSubmit { semaphore: w.semaphore, value: w.value, stage: w.stage });
		}
		trace!(
				total_wait_count = wait_submits.len(),
				"Merged timeline + binary waits"
                );
		let mut signal_submits = vec![SemaphoreSubmit {
			semaphore: self.timeline.handle(),
			value: val,
			stage: Stage::All,
		}];
		for s in binary_signals {
			signal_submits.push(SemaphoreSubmit { semaphore: s.semaphore, value: s.value, stage: s.stage });
		}
		trace!(
				total_signal_count = signal_submits.len(),
				timeline_signal = val,
				"Merged timeline + binary signals"
                );
		device.queue_submit2(self.queue.raw(), Some(cmd.handle()), &wait_submits, &signal_submits)?;
		debug!(
				signal_value = val,
				total_waits = wait_submits.len(),
				total_signals = signal_submits.len(),
				family = self.queue.family(),
				"submit_with_binary complete"
                );
		Ok(val)
	}
	
	#[instrument(skip_all, name = "WorkLane::submit_semaphore_only")]
	pub fn submit_semaphore_only(
		&mut self,
		device: &B::Device,
		binary_waits: &[SemaphoreSubmit<B>],
		binary_signals: &[SemaphoreSubmit<B>],
	) -> Result<u64, B::Error> {
		let val = self.next_value();
		trace!(
				signal_value = val,
				binary_wait_count = binary_waits.len(),
				binary_signal_count = binary_signals.len(),
				family = self.queue.family(),
				"Submitting semaphore-only (no command buffer)"
                );
		let mut signals: Vec<SemaphoreSubmit<B>> = vec![SemaphoreSubmit {
			semaphore: self.timeline.handle(),
			value: val,
			stage: Stage::All,
		}];
		signals.extend_from_slice(binary_signals);
		trace!(
				total_signal_count = signals.len(),
				timeline_signal = val,
				"Merged timeline + binary signals"
                );
		device.queue_submit2(self.queue.raw(), None, &binary_waits, &signals)?;
		debug!(
				signal_value = val,
				total_waits = binary_waits.len(),
				total_signals = signals.len(),
				family = self.queue.family(),
				"submit_semaphore_only complete"
                );
		Ok(val)
	}
}