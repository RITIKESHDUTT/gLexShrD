use crate::renderer::GfxPush;
use glex_shader_types::Vec4o0;
use glex_shader_types::Vec4i0;
use crate::renderer::prelude::*;
#[fragment_shader]
fn particle_frag(
	v_color: Vec4i0,
	#[push_constant] push: &GfxPush,
	#[builtin(point_coord)] point_coord: Vec2,
	mut out_color: Vec4o0,
) {
	let d = (point_coord - 0.5).length() * 2.0;
	let t = push.frame as f32 * 0.016;
	
	// ── three-layer soft glow ──
	let core = exp(-d * d * 10.0);          // bright center
	let glow = exp(-d * d * 2.5) * 0.4;     // mid halo
	let haze = exp(-d * 1.5) * 0.12;        // wide atmosphere
	
	// ── gentle breathing — ±10%, no spikes ──
	let breath = sin(t * 1.2) * 0.1 + 1.0;
	
	let intensity = (core + glow + haze) * breath;
	
	// ── warm the core subtly ──
	let warm = v_color.rgb() + core * vec3(0.1, 0.04, -0.02);
	
	let rgb = warm * intensity;
	let alpha = intensity * v_color.a();
	
	out_color = vec4_v3f(rgb, alpha);
}


pub const FRAG_SHADER: &str = PARTICLE_FRAG_GLSL;
pub static FRAG_SPV: &[u8] = PARTICLE_FRAG_SPV;