//! Wayland implementation of `core::platform::{Platform, Window}`.
//!
//! Owns all OS-level types. Translates Wayland events into core vocabulary.
//! Core never sees anything in this file.
use crate::core::Backend;
use crate::infra::vulkan::VulkanEntry;
use crate::infra::vulkan::VulkanInstance;
use crate::infra::platform::Surface;
use ash::vk;

use crate::infra::platform::surface::WaylandHandles;
use glex_platform::csd::hit_test::hit_test;
use {
	std::{
		sync::Arc,
	},
	crate::{
		infra::{
			VulkanBackend,
		},
	},
	glex_platform::platform::{
		ControlFlow,
		ElementState,
		Event,
		KeyCode,
		KeyEvent,
		MouseButton,
		Platform,
		Window,
		WindowConfig,
		WindowEvent,
		WindowId,
		Modifiers,
		Extent2D,
	},
	glex_platform::{
		csd::{
			hit_test::{HitZone},
			layout::DecorationLayout,
			state::{ButtonAction, DecorationState},
		},
		WlEvent, WaylandWindow, wayland_available}
};

// ─────────────────────────────────────────────────────────────────────────────
// Error
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WaylandError {
	#[error("wayland connection failed: {0}")]
	ConnectionFailed(&'static str),
	
	#[error("wayland window creation failed: {0}")]
	WindowCreationFailed(&'static str),
}


// ─────────────────────────────────────────────────────────────────────────────
// WaylandPlatform
// ─────────────────────────────────────────────────────────────────────────────

pub struct WaylandPlatform {
	next_generation: u32,
}

impl WaylandPlatform {
	pub fn new() -> Result<Self, WaylandError> {
		if !wayland_available() {
			return Err(WaylandError::ConnectionFailed(
				"WAYLAND_DISPLAY or XDG_RUNTIME_DIR not set / socket not found",
			));
		}
		Ok(Self { next_generation: 0 })
	}
}

impl Platform for WaylandPlatform {
	type Window = WaylandWindowImpl;
	type Error  = WaylandError;
	
	fn create_window(&mut self, config: WindowConfig) -> Result<Self::Window, Self::Error> {
		let inner = WaylandWindow::create(config.width, config.height)
			.map_err(WaylandError::WindowCreationFailed)?;
		
		let id = WindowId::new(0, self.next_generation);
		self.next_generation += 1;
		
		let dec_layout = DecorationLayout::new(config.width, config.height, false);
		let dec_state  = DecorationState::new();
		
		Ok(WaylandWindowImpl { inner, id, dec_layout, dec_state })
	}
	
	fn pump(&mut self, window: &mut Self::Window, callback: impl FnMut(Event)) -> ControlFlow {
		window.pump(callback)
	}
}

// ─────────────────────────────────────────────────────────────────────────────
// WaylandWindowImpl — impl Window + pump
// ─────────────────────────────────────────────────────────────────────────────
pub struct WaylandWindowImpl {
	
	pub inner: WaylandWindow,
	id:        WindowId,
	/// Decoration geometry — rebuilt on resize.
	dec_layout:     DecorationLayout,
	/// Decoration visual state — updated on pointer events.
	dec_state:      DecorationState,
}

impl WaylandWindowImpl {
	/// Call once per frame before rendering.
	///
	/// Flushes, dispatches, and translates all pending Wayland events into
	/// [`Event`]s, feeding each to `callback`.
	/// Returns [`ControlFlow::Exit`] if the window was closed.
	/// Call once per frame before rendering.
	///
	/// Flushes, dispatches Wayland events, handles CSD internally,
	/// and translates remaining events into core `Event`s for `callback`.
	///
	/// Returns `ControlFlow::Exit` if the window should close.
	pub fn pump<F>(&mut self, mut callback: F) -> ControlFlow
				   where F: FnMut(Event),
	{
		self.inner.display.flush();
		if self.inner.display.prepare_read() == 0 {
			self.inner.display.read_events();
		}
		self.inner.display.dispatch_pending();
		
		let mut cf = ControlFlow::Continue;
		
		while let Some(wl) = self.inner.poll_event() {
			match wl {
				// ── Close ────────────────────────────────────────────────────
				WlEvent::Close => {
					cf = ControlFlow::Exit;
					callback(Event::Window {
						id:    self.id,
						event: WindowEvent::CloseRequested,
					});
				}
				
				// ── Resize ───────────────────────────────────────────────────
				WlEvent::Resize { width, height } => {
					let wl_maximized = self.inner.maximized();
					
					self.dec_state.is_maximized = wl_maximized;
					
					self.dec_layout = DecorationLayout::new(
						width, height, wl_maximized,
					);
					callback(Event::Window {
						id:    self.id,
						event: WindowEvent::Resized(Extent2D::new(width, height)),
					});
				}
				
				// ── Configure ────────────────────────────────────────────────
				WlEvent::Configure => {}
				
				// ── Pointer enter / motion ───────────────────────────────────
				WlEvent::PointerEnter { x, y } | WlEvent::PointerMotion { x, y } => {
					let zone = hit_test(&self.dec_layout, x, y);
					self.dec_state.update_hover(zone);
					
					let serial = self.inner.pointer_serial();
					self.inner.set_cursor_shape(serial, zone.cursor_shape());
					
					
					if zone == HitZone::ClientArea {
						callback(Event::Window {
							id:    self.id,
							event: WindowEvent::CursorMoved { x: x as f32, y: y as f32 },
						});
					}
				}
				
				// ── Pointer leave ────────────────────────────────────────────
				WlEvent::PointerLeave => {
					self.dec_state.reset();
					callback(Event::Window {
						id:    self.id,
						event: WindowEvent::CursorLeft,
					});
				}
				
				// ── Pointer button ───────────────────────────────────────────
				WlEvent::PointerButton { button, pressed, serial } => {
					let (px, py) = self.inner.pointer_position();
					let zone = hit_test(&self.dec_layout, px, py);
					
					if button == 0x110 {
						if pressed {
							match zone {
								HitZone::TitleBar => {
									self.inner.interactive_move(serial);
								}
								z if z.is_resize() => {
									if let Some(edge) = z.resize_edge() {
										self.inner.interactive_resize(serial, edge);
									}
								}
								HitZone::ButtonClose
								| HitZone::ButtonMaximize
								| HitZone::ButtonMinimize => {
									self.dec_state.press(zone);
								}
								HitZone::ClientArea => {
									callback(Event::Window {
										id:    self.id,
										event: WindowEvent::MouseInput {
											button: MouseButton::Left,
											state:  ElementState::Pressed,
										},
									});
								}
								_ => {}
							}
						} else {
							if let Some(action) = self.dec_state.release(zone) {
								match action {
									ButtonAction::Close => {
										cf = ControlFlow::Exit;
										callback(Event::Window {
											id:    self.id,
											event: WindowEvent::CloseRequested,
										});
									}
									ButtonAction::Minimize => {
										self.inner.minimize();
									}
									ButtonAction::Maximize => {
										self.inner.toggle_maximize();
										self.dec_state.is_maximized =
											!self.dec_state.is_maximized;
										let (w, h) = self.inner.size();
										self.dec_layout = DecorationLayout::new(
											w, h, self.dec_state.is_maximized,
										);
									}
								}
							} else if zone == HitZone::ClientArea {
								callback(Event::Window {
									id:    self.id,
									event: WindowEvent::MouseInput {
										button: MouseButton::Left,
										state:  ElementState::Released,
									},
								});
							}
						}
					} else if zone == HitZone::ClientArea {
						let mb = match button {
							0x111 => MouseButton::Right,
							0x112 => MouseButton::Middle,
							other => MouseButton::Other(other as u16),
						};
						let state = if pressed {
							ElementState::Pressed
						} else {
							ElementState::Released
						};
						callback(Event::Window {
							id:    self.id,
							event: WindowEvent::MouseInput { button: mb, state },
						});
					}
				}
				
				// ── Keyboard ─────────────────────────────────────────────────
				WlEvent::Key { keycode, pressed, modifiers } => {
					let key = scancode_to_keycode(keycode);
					let state = if pressed {
						ElementState::Pressed
					} else {
						ElementState::Released
					};
					let modifiers = Modifiers {
						ctrl:  modifiers.ctrl,
						alt:   modifiers.alt,
						shift: modifiers.shift,
						logo:  modifiers.logo,
					};
					
					if pressed {
						match key {
							KeyCode::Escape => {
								cf = ControlFlow::Exit;
								callback(Event::Window {
									id:    self.id,
									event: WindowEvent::CloseRequested,
								});
								continue;
							}
							KeyCode::Q if modifiers.ctrl => {
								cf = ControlFlow::Exit;
								callback(Event::Window {
									id:    self.id,
									event: WindowEvent::CloseRequested,
								});
								continue;
							}
							KeyCode::F11 | KeyCode::F8 => {
								self.inner.toggle_maximize();
								self.dec_state.is_maximized =
									!self.dec_state.is_maximized;
								let (w, h) = self.inner.size();
								self.dec_layout = DecorationLayout::new(
									w, h, self.dec_state.is_maximized,
								);
								continue;
							}
							KeyCode::Enter if modifiers.alt => {
								self.inner.toggle_maximize();
								self.dec_state.is_maximized =
									!self.dec_state.is_maximized;
								let (w, h) = self.inner.size();
								self.dec_layout = DecorationLayout::new(
									w, h, self.dec_state.is_maximized,
								);
								continue;
							}
							_ => {}
						}
					}
					
					callback(Event::Window {
						id:    self.id,
						event: WindowEvent::KeyboardInput(KeyEvent {
							key,
							state,
							repeat: false,
							modifiers,
						}),
					});
				}
			}
		}
		
		callback(Event::MainEventsCleared);
		cf
	}
	
	
	
	/// Decoration layout — for the renderer to draw CSD geometry.
	pub fn decoration_layout(&self) -> &DecorationLayout {
		&self.dec_layout
	}
	
	/// Decoration state — for the renderer to draw button visuals.
	pub fn decoration_state(&self) -> &DecorationState {
		&self.dec_state
	}
	
	/// Raw `wl_surface` pointer — required by Vulkan for surface creation.
	///
	/// # Safety
	///
	/// The pointer is valid only while this [`WaylandWindowImpl`] is alive.
	/// The caller must not store it beyond that lifetime.
	#[inline]
	pub fn wl_surface(&self) -> *mut std::ffi::c_void {
		self.inner.surface.ptr as *mut std::ffi::c_void
	}
	
	/// Raw `wl_display` pointer — required by Vulkan for surface creation.
	///
	/// # Safety
	///
	/// Same constraints as [`wl_surface`](Self::wl_surface).
	#[inline]
	pub fn wl_display(&self) -> *mut std::ffi::c_void {
		self.inner.display.as_ptr() as *mut std::ffi::c_void
	}
}

impl Window for WaylandWindowImpl {
	#[inline]
	fn id(&self) -> WindowId {
		self.id
	}
	
	#[inline]
	fn extent(&self) -> Extent2D {
		let (w, h) = self.inner.size();
		Extent2D::new(w, h)
	}
	
	#[inline]
	fn request_redraw(&self) {
		// Engine renders every frame — no-op.
	}
}


fn scancode_to_keycode(sc: u32) -> KeyCode {
	match sc {
		1  => KeyCode::Escape,    14 => KeyCode::Backspace, 15 => KeyCode::Tab,
		28 => KeyCode::Enter,     57 => KeyCode::Space,     82 => KeyCode::Insert,
		83 => KeyCode::Delete,    71 => KeyCode::Home,      79 => KeyCode::End,
		73 => KeyCode::PageUp,    81 => KeyCode::PageDown,
		
		75 => KeyCode::Left,  77 => KeyCode::Right,
		72 => KeyCode::Up,    80 => KeyCode::Down,
		
		30 => KeyCode::A,  48 => KeyCode::B,  46 => KeyCode::C,  32 => KeyCode::D,
		18 => KeyCode::E,  33 => KeyCode::F,  34 => KeyCode::G,  35 => KeyCode::H,
		23 => KeyCode::I,  36 => KeyCode::J,  37 => KeyCode::K,  38 => KeyCode::L,
		50 => KeyCode::M,  49 => KeyCode::N,  24 => KeyCode::O,  25 => KeyCode::P,
		16 => KeyCode::Q,  19 => KeyCode::R,  31 => KeyCode::S,  20 => KeyCode::T,
		22 => KeyCode::U,  47 => KeyCode::V,  17 => KeyCode::W,  45 => KeyCode::X,
		21 => KeyCode::Y,  44 => KeyCode::Z,
		
		11 => KeyCode::Key0,   2 => KeyCode::Key1,  3 => KeyCode::Key2,
		4 => KeyCode::Key3,   5 => KeyCode::Key4,  6 => KeyCode::Key5,
		7 => KeyCode::Key6,   8 => KeyCode::Key7,  9 => KeyCode::Key8,
		10 => KeyCode::Key9,
		
		59 => KeyCode::F1,  60 => KeyCode::F2,  61 => KeyCode::F3,  62 => KeyCode::F4,
		63 => KeyCode::F5,  64 => KeyCode::F6,  65 => KeyCode::F7,  66 => KeyCode::F8,
		67 => KeyCode::F9,  68 => KeyCode::F10, 87 => KeyCode::F11, 88 => KeyCode::F12,
		
		42  => KeyCode::LShift,  54  => KeyCode::RShift,
		29  => KeyCode::LCtrl,   97  => KeyCode::RCtrl,
		56  => KeyCode::LAlt,    100 => KeyCode::RAlt,
		125 => KeyCode::LSuper,  126 => KeyCode::RSuper,
		
		_ => KeyCode::Unknown,
	}
}


impl WaylandHandles for WaylandWindowImpl {
	fn wl_display(&self) -> *mut std::ffi::c_void { self.wl_display() }
	fn wl_surface(&self) -> *mut std::ffi::c_void { self.wl_surface() }
}

impl VulkanWindow for WaylandWindowImpl {
	fn required_vulkan_extensions() -> &'static [*const i8] {
		Surface::REQUIRED_EXTENSIONS_WAYLAND
	}
	
	fn create_surface(
		&self,
		entry: &VulkanEntry,
		instance: Arc<VulkanInstance>,
	) -> Result<Surface, <VulkanBackend as Backend>::Error> {
		Surface::from_wayland_window(entry, instance, self)
	}
	
}

/// Extends `Window` with Vulkan surface creation capability.
/// Core never sees this. Infra uses it for `VulkanContext::new`.
pub trait VulkanWindow: Window {
	fn required_vulkan_extensions() -> &'static [*const i8];
	fn create_surface(
		&self,
		entry:    &VulkanEntry,
		instance: Arc<VulkanInstance>,
	) -> Result<Surface, vk::Result>;
}