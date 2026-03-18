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
	let core  = exp(-d * d * 3.0);
	let glow  = exp(-d * d * 0.8) * 0.4;
	let total = core - glow;
	
	// time from frame
	let t = push.frame as f32 * 0.02;
	// animated color shift
	let shift = vec3(
		(t).sin() * 0.5 + 0.5,
		(t * 1.3).sin() * 0.5 + 0.5,
		(t * 0.7).sin() * 0.5 + 0.5,
	);
	
	let base = v_color.rgb() * shift;
	let color = base * (1.0 + core * 0.8);  // just the glow boost, no color shift
	let alpha = total * v_color.a();
	
	out_color = vec4_v3f(color, alpha);
}

pub const FRAG_SHADER: &str = PARTICLE_FRAG_GLSL;
pub static FRAG_SPV: &[u8] = PARTICLE_FRAG_SPV;