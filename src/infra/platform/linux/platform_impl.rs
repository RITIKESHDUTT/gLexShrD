//! Wayland implementation of `core::platform::{Platform, Window}`.
//!
//! Owns all OS-level types. Translates Wayland events into core vocabulary.
//! Core never sees anything in this file.
use glex_platform::platform::ScrollDelta;
use glex_platform::platform::DeviceEvent;
use crate::core::Backend;
use crate::infra::platform::Surface;
use crate::infra::vulkan::VulkanEntry;
use crate::infra::vulkan::VulkanInstance;
use ash::vk;
use arrayvec::ArrayVec;
use glex_platform::csd::hit_test::hit_test;
use {
	crate::infra::VulkanBackend,
	glex_platform::platform::{
		ControlFlow,
		ElementState,
		Event,
		Extent2D,
		KeyCode,
		KeyEvent,
		Modifiers,
		MouseButton,
		Platform,
		Window,
		WindowConfig,
		WindowEvent,
		WindowId,
	},
	glex_platform::{
		csd::{
			hit_test::HitZone,
			layout::DecorationLayout,
			state::{ButtonAction, DecorationState},
		},
		wayland_available, WaylandWindow, WlEvent},
	std::sync::Arc
};
use glex_platform::csd::CsdTheme;
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
		let surface_w = config.width;
		let surface_h = config.height;
		let inner = WaylandWindow::create(surface_w, surface_h)
			.map_err(WaylandError::WindowCreationFailed)?;
		let id = WindowId::new(0, self.next_generation);
		
		self.next_generation += 1;
		
		let dec_layout = DecorationLayout::new(surface_w, surface_h, false, false);
		let dec_state  = DecorationState::new();
		Ok(WaylandWindowImpl { inner, id, config: config.clone(), dec_layout, dec_state, csd_theme: CsdTheme::default(), last_extent: Extent2D::new(surface_w, surface_h), pending_configure: false, is_maximized: false, is_fullscreen: false})
	}
	
	fn pump(&mut self, window: &mut Self::Window) -> (ControlFlow, ArrayVec<Event, 64>) {
		window.pump()
	}
}

// ─────────────────────────────────────────────────────────────────────────────
// WaylandWindowImpl — impl Window + pump
// ─────────────────────────────────────────────────────────────────────────────

pub struct WaylandWindowImpl {
	pub inner: WaylandWindow,
	id: WindowId,
	dec_layout: DecorationLayout,
	config: WindowConfig,
	dec_state: DecorationState,
	csd_theme: CsdTheme,
	last_extent: Extent2D,
	pending_configure: bool,
	is_maximized: bool,
	is_fullscreen: bool,
}

impl WaylandWindowImpl {
	/// Dispatch Wayland events, return control flow + translated events.
	pub fn pump(&mut self) -> (ControlFlow, ArrayVec<Event, 64>) {
		self.inner.display.dispatch_pending();
		self.inner.display.flush();
		
		if self.inner.display.prepare_read() == 0 {
			self.inner.display.read_events();
		} else {
			self.inner.display.cancel_read();
		}
		
		self.inner.display.dispatch_pending();
		
		let mut cf = ControlFlow::Continue;
		let mut events: ArrayVec<Event, 64> = ArrayVec::new();
		#[inline]
		fn emit(events: &mut ArrayVec<Event, 64>,  id: WindowId, event: WindowEvent) {
			events.push(Event::Window { id, event });
		}
		
		#[inline]
		fn map_button(button: u32) -> MouseButton {
			match button {
				0x110 => MouseButton::Left,
				0x111 => MouseButton::Right,
				0x112 => MouseButton::Middle,
				other => MouseButton::Other(other as u16),
			}
		}
		#[inline]
		fn request_close(cf: &mut ControlFlow, events: &mut ArrayVec<Event, 64>, id: WindowId) {
			*cf = ControlFlow::Exit;
			events.push(Event::Window {
				id,
				event: WindowEvent::CloseRequested,
			});
		}
		
		fn pointer_move(
			window: &mut WaylandWindowImpl,
			events: &mut ArrayVec<Event, 64>,
			x: f64,
			y: f64,
		) {
			if window.is_fullscreen {
				emit(events, window.id, WindowEvent::CursorMoved {
					x: x as f32,
					y: y as f32,
				});
				return;
			}
			
			let zone = hit_test(&window.dec_layout, x, y);
			window.dec_state.update_hover(zone);
			
			let serial = window.inner.pointer_serial();
			window.inner.set_cursor_shape(serial, zone.cursor_shape());
			
			if zone == HitZone::ClientArea {
				let (ox, oy) = window.dec_layout.content_offset();
				
				emit(events, window.id, WindowEvent::CursorMoved {
					x: (x - ox) as f32,
					y: (y - oy) as f32,
				});
			}
		}
		
		while let Some(wl) = self.inner.poll_event() {
			match wl {
				// ── Close ────────────────────────────────────────────────────
				WlEvent::Close => {
					request_close(&mut cf, &mut events, self.id);
				}
				
				// ── Resize ───────────────────────────────────────────────────
				WlEvent::Resize { width, height } => {
					if width == 0 || height == 0 { continue; }
					let width  = width.max(DecorationLayout::min_width());
					let height = height.max(DecorationLayout::min_height());
					let new_extent = Extent2D::new(width, height);
					if new_extent == self.last_extent { continue; }
					self.last_extent = new_extent;
					emit(&mut events, self.id, WindowEvent::Resized(new_extent));
				}
				
				WlEvent::Configure { serial } => {
					let wl_maximized  = self.inner.maximized();
					let wl_fullscreen = self.inner.fullscreen();
					self.is_maximized  = wl_maximized;
					self.is_fullscreen = wl_fullscreen;
					self.inner.ack_configure(serial);
					self.pending_configure = true;
				}
				
				// ── Pointer enter / motion ───────────────────────────────────
				
				WlEvent::PointerEnter { x, y } => {
					emit(&mut events, self.id, WindowEvent::CursorEntered);
					pointer_move(self, &mut events, x, y);
				}
				WlEvent::PointerMotion { x, y } => {
					pointer_move(self, &mut events, x, y);
				}
				
				// ── Pointer leave ────────────────────────────────────────────
				
				WlEvent::PointerLeave => {
					self.dec_state.reset();
					emit(&mut events, self.id, WindowEvent::CursorLeft);

				}
				WlEvent::FocusGained | WlEvent::FocusLost => {
					let focused = matches!(wl, WlEvent::FocusGained);
					
					emit(
						&mut events,
						self.id,
						WindowEvent::Focused(focused),
					);
				}
				
				WlEvent::Scroll { axis, value } => {
					let (x, y) = match axis {
						0 => (0.0, value as f32),
						_ => (value as f32, 0.0),
					};
					
					events.push(Event::Device {
						event: DeviceEvent::MouseWheel {
							delta: ScrollDelta::Pixels { x, y },
						},
					});
				}
				
				// ── Pointer button ───────────────────────────────────────────
				WlEvent::PointerButton { button, pressed, serial } => {
					let (px, py) = self.inner.pointer_position();
					if self.is_fullscreen {
						let mb = map_button(button);
						let state = if pressed { ElementState::Pressed } else { ElementState::Released };
						emit(
							&mut events,
							self.id,
							WindowEvent::MouseInput { button: mb, state },
						);
						continue;
					}
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
									let (ox, oy) = self.dec_layout.content_offset();
									events.push(Event::Window {
										id: self.id,
										event: WindowEvent::MouseInput {
											button: MouseButton::Left,
											state: ElementState::Pressed,
										},
									});
									events.push(Event::Window {
										id: self.id,
										event: WindowEvent::CursorMoved {
											x: (px - ox) as f32,
											y: (py - oy) as f32,
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
										events.push(Event::Window {
											id: self.id,
											event: WindowEvent::CloseRequested,
										});
									}
									ButtonAction::Minimize => {
										self.inner.minimize();
									}
									ButtonAction::Maximize => {
										self.inner.toggle_maximize();
									}
								}
							} else if zone == HitZone::ClientArea {
								events.push(Event::Window {
									id: self.id,
									event: WindowEvent::MouseInput {
										button: MouseButton::Left,
										state: ElementState::Released,
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
						events.push(Event::Window {
							id: self.id,
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
								events.push(Event::Window {
									id:    self.id,
									event: WindowEvent::CloseRequested,
								});
								continue;
							}
							KeyCode::Q if modifiers.ctrl => {
								
								cf = ControlFlow::Exit;
								events.push(Event::Window {
									id:    self.id,
									event: WindowEvent::CloseRequested,
								});
								continue;
							}
							KeyCode::F11 => {
								self.inner.toggle_fullscreen();
								continue;
							}
							
							KeyCode::Enter if modifiers.alt => {
								self.inner.toggle_maximize();
								continue;
							}
							_ => {}
						}
					}
					
					events.push(Event::Window {
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
		events.push(Event::MainEventsCleared);
		
		self.inner.display.flush();
		(cf, events)
	}
	pub fn take_pending_configure(&mut self) -> bool {
		std::mem::replace(&mut self.pending_configure, false)
	}
	pub fn set_pending_configure(&mut self) {
		self.pending_configure = true
	}
	
	
	/// Rebuilds layout from swapchain extent. Invariant: layout.size() == swap_extent.
	pub fn rebuild_decoration_layout(&mut self, width: u32, height: u32) {
		self.dec_layout = DecorationLayout::new(width, height, self.is_maximized, self.is_fullscreen);
		self.inner.set_window_geometry_full(width, height);
	}

	pub fn decoration_layout(&self) -> &DecorationLayout {
		&self.dec_layout
	}
	
	pub fn decoration_state(&self) -> &DecorationState {
		&self.dec_state
	}
	
	pub fn set_theme(&mut self, theme: CsdTheme) {
		self.csd_theme = theme;
	}
	
	pub fn theme(&self) -> &CsdTheme {
		&self.csd_theme
	}
	
	pub fn sync_extent(&mut self, extent: Extent2D) {
		self.last_extent = extent;
	}
	/// Valid only while self is alive.
	#[inline]
	pub fn wl_surface(&self) -> *mut std::ffi::c_void {
		self.inner.surface.ptr as *mut std::ffi::c_void
	}
	
	/// Valid only while self is alive.
	#[inline]
	pub fn wl_display(&self) -> *mut std::ffi::c_void {
		self.inner.display.as_ptr() as *mut std::ffi::c_void
	}
	
	pub fn is_maximized(&self) -> bool { self.is_maximized }
	pub fn is_fullscreen(&self) -> bool { self.is_fullscreen }
	pub fn min_surface_width(&self) -> u32 { DecorationLayout::min_width() }
	pub fn min_surface_height(&self) -> u32 { DecorationLayout::min_height() }
	
	
	#[inline]
	pub fn config(&self) -> &WindowConfig {
		&self.config
	}
	
	#[inline]
	pub fn title(&self) -> &str {
		&self.config.title
	}
	
	#[inline]
	pub fn width(&self) -> u32 {
		self.config.width
	}
	
	#[inline]
	pub fn height(&self) -> u32 {
		self.config.height
	}
	
	#[inline]
	pub fn resizable(&self) -> bool {
		self.config.resizable
	}
	
	#[inline]
	pub fn visible(&self) -> bool {
		self.config.visible
	}
}

impl Window for WaylandWindowImpl {
	#[inline]
	fn id(&self) -> WindowId {
		self.id
	}
	
	#[inline]
	fn extent(&self) -> Extent2D {
		self.last_extent
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

impl VulkanWindow for WaylandWindowImpl {
	fn required_vulkan_extensions() -> &'static [*const i8] {
		Surface::REQUIRED_EXTENSIONS_WAYLAND
	}
	
	fn create_surface(
		&self,
		entry: &VulkanEntry,
		instance: Arc<VulkanInstance>,
	) -> Result<Surface, <VulkanBackend as Backend>::Error> {
		Surface::from_wayland_window(entry, instance, self.inner.native())
	}
	
}

/// Vulkan surface creation. Infra-only — core never sees this.
pub trait VulkanWindow: Window {
	fn required_vulkan_extensions() -> &'static [*const i8];
	fn create_surface(
		&self,
		entry:    &VulkanEntry,
		instance: Arc<VulkanInstance>,
	) -> Result<Surface, vk::Result>;
}