use crate::renderer::prelude::*;
#[fragment_shader]
fn particle_frag(
	#[location(0, in)] v_color:    Vec4,
	#[location(0, out)] out_color:  Vec4,
	#[builtin(point_coord)] point_coord: Vec2,
) {
	// Normalised distance from point center (0 = center, 1 = edge)
	let d = (point_coord - 0.5).length() * 2.0;
	// Bright core + wide glow halo
	let core  = exp(-d * d * 3.0);
	let glow  = exp(-d * d * 0.8) * 0.4;
	let total = core + glow;
	// Colour: tint the particle colour by core brightness
	let color = v_color.rgb() * (1.0 + core * 0.8);
	let alpha = total * v_color.a();
	out_color = vec4_v3f(color, alpha);
}
pub const FRAG_SHADER:    &str = PARTICLE_FRAG_GLSL;

pub static FRAG_SPV:    &[u8] = PARTICLE_FRAG_SPV;