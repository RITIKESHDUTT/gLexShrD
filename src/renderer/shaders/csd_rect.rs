use crate::renderer::prelude::*;
use crate::renderer::pipelines::RectPush;

#[vertex_shader]
fn csd_vert(
	#[location(0, in)] in_pos: Vec2,
	#[location(1, in)] in_uv: Vec2,
	#[push_constant(screen_size: Vec2, rect_pos: Vec2, rect_size: Vec2, radius: f32, _pad: f32,color: Vec4)] params: &RectPush,
	#[location(0, out)] mut frag_color: Vec4,
	#[location(1, out)]	mut v_uv: Vec2,
	#[location(2, out, flat)]	mut v_size: Vec2,
	#[location(3, out, flat)]	mut v_radius: f32,
	#[builtin(position)]	mut position: Vec4,
) {
	let pixel: Vec2 = params.rect_pos + in_pos * params.rect_size;
	let ndc:   Vec2 = (pixel / params.screen_size) * 2.0 - 1.0;
	
	position   = vec4(ndc, 0.0, 1.0);
	frag_color = params.color;
	v_uv       = in_uv;
	v_size     = params.rect_size;
	v_radius   = params.radius;
}

#[shader_fn]
fn sd_rounded_box(p: Vec2, half_size: Vec2, r: f32) -> f32 {
	let q: Vec2 = p.abs() - (half_size - vec2(r, r));
	q.max_f(0.0).length()
		+ min(max(q.x, q.y), 0.0)
		- r
}
#[fragment_shader]
fn csd_frag(
	#[location(0, in)] frag_color: Vec4,
	#[location(1, in)] v_uv: Vec2,
	#[location(2, in, flat)] v_size: Vec2,
	#[location(3, in, flat)] v_radius: f32,
	#[location(0, out)] mut out_color: Vec4,
) {
	let half_size: Vec2 = v_size * 0.5;
	let p: Vec2 = (v_uv - vec2(0.5, 0.5)) * v_size;
	
	let r: f32 = clamp(v_radius, 0.0, min(half_size.x, half_size.y));
	
	// ---- INLINE sdRoundedBox ----
	let q: Vec2 = p.abs() - (half_size - vec2(r, r));
	let d: f32 =
		q.max_f(0.0).length()
			+ min(max(q.x, q.y), 0.0)
			- r;
	// ----------------------------
	
	let w: f32 = fwidth(d);
	let alpha: f32 = 1.0 - smoothstep(-w, w, d);
	
	if alpha <= 0.0 {
		out_color = vec4(0.0, 0.0, 0.0, 0.0);
		return;
	}
	
	let mut color: Vec4 = frag_color;
	color.a *= alpha;
	color = vec4_v3f(color.rgb() * color.a, color.a);
	
	out_color = color;
}

pub const  VERTEX_SHADER: &str = CSD_VERT_GLSL;
pub const FRAGMENT_SHADER:&str = CSD_FRAG_GLSL;

pub static VERT_SPV:    &[u8] = CSD_VERT_SPV;
pub static FRAG_SPV:    &[u8] = CSD_FRAG_SPV;