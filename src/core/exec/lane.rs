use crate::{
	core::{
		Backend, DeviceOps, SemaphoreSubmit,
		cmd::{CommandBuffer, CommandPool, Executable, Initial},
		type_state_queue::sealed::QueueHandle,
		sync::TimelineSemaphore,
	},
	domain::Stage,
};

pub struct WorkLane<'dev, Q: QueueHandle, B: Backend> {
	pool: CommandPool<'dev, B>,
	timeline: TimelineSemaphore<'dev, B>,
	queue: Q,
	last_signal: u64,
}


impl<'dev, Q, B: Backend> WorkLane<'dev, Q, B>
	where
		Q: QueueHandle<Handle = B::Queue>,
		B::Device: DeviceOps<B>,
{
	pub fn new(device: &'dev B::Device, queue: Q) -> Result<Self, B::Error> {
		let pool = CommandPool::new(device, &queue)?;
		let timeline = TimelineSemaphore::new(device, 0)?;
		Ok(Self {
			pool,
			timeline,
			queue,
			last_signal: 0,
		})
	}
	
	
	
	pub fn allocate(&self) -> Result<CommandBuffer<'dev, Initial, B>, B::Error> {
		self.pool.allocate()
	}
	
	pub fn last_signal_value(&self) -> u64 {
		self.last_signal
	}
	
	fn next_value(&mut self) -> u64 {
		self.last_signal += 1;
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
	
	pub fn submit(
		&mut self,
		device: &B::Device,
		cmd: CommandBuffer<'_, Executable, B>,
		waits: &[(B::Semaphore, u64)],
	) -> Result<u64, B::Error> {
		let val = self.next_value();
		let wait_submits: Vec<SemaphoreSubmit<B>>
			= waits.iter()
				   .map(|&(sem, value) |
					   SemaphoreSubmit { semaphore: sem, value, stage: Stage::All })
				   .collect();
		let signals = [SemaphoreSubmit {
			semaphore: self.timeline.handle(),
			value: val,
			stage: Stage::All,
		}];
		device.queue_submit2(self.queue.raw(), Some(cmd.handle()), &wait_submits, &signals)?;
		Ok(val)
	}
	
	#[warn(deprecated)]
	//superseded by submit_binary
	pub fn submit_present(
		&mut self,
		device: &B::Device,
		cmd: CommandBuffer<'_, Executable, B>,
		waits: &[(B::Semaphore, u64)],
		acquire_sem: B::Semaphore,
		present_sem: B::Semaphore,
	) -> Result<u64, B::Error> {
		let val = self.next_value();
		let mut wait_submits: Vec<SemaphoreSubmit<B>> = waits.iter()
															 .map(|&(sem, value)| SemaphoreSubmit { semaphore: sem, value, stage: Stage::All })
															 .collect();
		wait_submits.push(SemaphoreSubmit {
			semaphore: acquire_sem,
			value: 0,
			stage: Stage::ColorOutput,
		});
		let signals = [
			SemaphoreSubmit { semaphore: self.timeline.handle(), value: val, stage: Stage::All },
			SemaphoreSubmit { semaphore: present_sem, value: 0, stage: Stage::All },
		];
		device.queue_submit2(self.queue.raw(), Some(cmd.handle()), &wait_submits, &signals)?;
		Ok(val)
	}
	
	pub fn submit_with_binary(
		&mut self,
		device: &B::Device,
		cmd: CommandBuffer<'_, Executable, B>,
		waits: &[(B::Semaphore, u64)],
		binary_waits: &[SemaphoreSubmit<B>],
		binary_signals: &[SemaphoreSubmit<B>],
	) -> Result<u64, B::Error> {
		let val = self.next_value();
		let mut wait_submits: Vec<SemaphoreSubmit<B>>
			= waits.iter()
				   .map(|&(sem, value) |
					   SemaphoreSubmit { semaphore: sem, value, stage: Stage::All })
			.collect();
		for w in binary_waits {
			wait_submits.push(SemaphoreSubmit { semaphore: w.semaphore, value: w.value, stage: w.stage });
		}
		let mut signal_submits = vec![SemaphoreSubmit {
			semaphore: self.timeline.handle(),
			value: val,
			stage: Stage::All,
		}];
		for s in binary_signals {
			signal_submits.push(SemaphoreSubmit { semaphore: s.semaphore, value: s.value, stage: s.stage });
		}
		device.queue_submit2(self.queue.raw(), Some(cmd.handle()), &wait_submits, &signal_submits)?;
		Ok(val)
	}
	/// Empty submit: no command buffers, just bumps the timeline.
	/// Use after vkQueuePresentKHR on the same queue to extend
	/// timeline coverage past the present operation.
	pub fn bump_timeline(&mut self, device: &B::Device) -> Result<u64, B::Error> {
		let val = self.next_value();
		let signals = [SemaphoreSubmit {
			semaphore: self.timeline.handle(),
			value: val,
			stage: Stage::All,
		}];
		device.queue_submit2(self.queue.raw(), None, &[], &signals)?;
		Ok(val)
	}
}