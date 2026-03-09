use crate::core::backend::Backend;
use crate::core::backend::types::{Extent2D, Rect2D, Offset2D, AttachmentLoadOp, AttachmentStoreOp, ClearValue};
use crate::core::backend::{RenderingDesc, ColorAttachment, DepthAttachment};
use crate::core::ImageView;
use crate::domain::ImageLayout;

pub struct RenderingInfoBuilder<B: Backend> {
	color_attachments: Vec<ColorAttachment<B>>,
	depth_attachment: Option<DepthAttachment<B>>,
	extent: Extent2D,
}

impl<B: Backend> RenderingInfoBuilder<B> {
	pub fn new(extent: Extent2D) -> Self {
		Self {
			color_attachments: Vec::new(),
			depth_attachment: None,
			extent,
		}
	}
	
	pub fn color_clear(mut self, view: &ImageView<'_, B>, clear_color: [f32; 4]) -> Self {
		self.color_attachments.push(ColorAttachment {
			view: view.handle(),
			layout: ImageLayout::ColorAttachment,
			load_op: AttachmentLoadOp::CLEAR,
			store_op: AttachmentStoreOp::STORE,
			clear_value: ClearValue::Color(clear_color),
		});
		self
	}
	
	pub fn color_load(mut self, view: &ImageView<'_, B>) -> Self {
		self.color_attachments.push(ColorAttachment {
			view: view.handle(),
			layout: ImageLayout::ColorAttachment,
			load_op:  AttachmentLoadOp::LOAD,
			store_op: AttachmentStoreOp::STORE,
			clear_value: ClearValue::Color([0.0; 4]),
		});
		self
	}
	
	pub fn depth_clear(mut self, view: &ImageView<'_, B>, clear_depth: f32) -> Self {
		self.depth_attachment = Some(DepthAttachment {
			view: view.handle(),
			layout: ImageLayout::DepthAttachment,
			load_op:  AttachmentLoadOp::CLEAR,
			store_op: AttachmentStoreOp::STORE,
			clear_depth,
		});
		self
	}
	
	pub fn build(self) -> RenderingDesc<B> {
		RenderingDesc {
			area: Rect2D::new(
				Offset2D::new(0, 0),
				self.extent,
			),
			color_attachments: self.color_attachments,
			depth_attachment: self.depth_attachment,
		}
	}
}