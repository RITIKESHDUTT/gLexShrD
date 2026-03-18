
use crate::lin_al::Vec3;
use crate::renderer::prelude::*;
use crate::renderer::shaders::ComputePush;

// ── Helper Functions ──────────────────────────────────────────────────────────
// NOTE: #[shader_fn] wiring into the macro is the next step.
// For now these are declared here so Rust type-checks the whole file.
#[shader_fn]
fn mod289_v3(x: Vec3) -> Vec3 {
	x - (x * (1.0 / 289.0)).floor() * 289.0
}
#[shader_fn]
fn mod289_v2(x: Vec2) -> Vec2 {
	x - (x * (1.0 / 289.0)).floor() * 289.0
}
#[shader_fn]
fn permute(x: Vec3) -> Vec3 {
	mod289_v3((x * 34.0 + 1.0) * x)
}
#[shader_fn]
fn snoise(v: Vec2) -> f32 {
	let c = vec4(
		0.211324865405187_f32,
		0.366025403784439_f32,
		-0.577350269189626_f32,
		0.024390243902439_f32,
	);
	
	let i: Vec2  = (v + dot_v2(v, c.yy())).floor();
	let x0: Vec2 = v - i + dot_v2(i, c.xx());
	
	let i1: Vec2 = if x0.x > x0.y { vec2(1.0, 0.0) } else { vec2(0.0, 1.0) };
	
	let x12_raw: Vec4 = x0.xyxy() + c.xxzz();
	let x12: Vec4     = vec4_2v(x12_raw.xy() - i1, x12_raw.zw());
	
	let i: Vec2 = mod289_v2(i);
	
	let p: Vec3 = permute(
		permute(i.y + vec3(0.0, i1.y, 1.0)) + i.x + vec3(0.0, i1.x, 1.0)
	);
	
	let m: Vec3 = vec3(
		0.5 - dot_v2(x0, x0),
		0.5 - dot_v2(x12.xy(), x12.xy()),
		0.5 - dot_v2(x12.zw(), x12.zw()),
	).max_f(0.0);
	
	let m = m * m;
	let m = m * m;
	
	let x: Vec3  = fract_v3(p * c.www()) * 2.0 - 1.0;
	let h: Vec3  = x.abs() - 0.5;
	let ox: Vec3 = (x + 0.5).floor();
	let a0: Vec3 = x - ox;
	
	let m = m * (1.79284291400159_f32 - 0.85373472095314_f32) * (a0 * a0 + h * h);
	
	let yz: Vec2 = a0.yz() * x12.xz() + h.yz() * x12.yw();
	let g: Vec3  = vec3(a0.x * x0.x + h.x * x0.y, yz.x, yz.y);
	
	130.0 * dot_v3(m, g)
}

#[shader_fn]
fn curl(p: Vec2, t: f32) -> Vec2 {
	let eps = 0.005_f32;
	
	let n = snoise(p + vec2(0.0,  eps) + t);
	let s = snoise(p + vec2(0.0, -eps) + t);
	let e = snoise(p + vec2( eps, 0.0) + t);
	let w = snoise(p + vec2(-eps, 0.0) + t);
	
	vec2(n - s, -(e - w)) / (2.0 * eps)
}
// ── Compute Kernel ────────────────────────────────────────────────────────────
#[compute_shader(workgroup = 512, helpers = [MOD289_V3_GLSL, MOD289_V2_GLSL, PERMUTE_GLSL, SNOISE_GLSL, CURL_GLSL])]
fn particle_comp(
	#[storage(set = 0, binding = 0, read)] particles_in:  &[Vec4],
	#[storage(set = 0, binding = 1, write)] particles_out: &mut [Vec4],
	#[push_constant] params: &ComputePush,
	#[builtin(global_invocation_id)] gid: UVec3,
) {
	let idx = gid.x as usize;
	if idx >= params.count as usize { return; }
	
	let p: Vec4 = particles_in[idx];
	let mut pos: Vec2 = p.xy();
	let mut vel: Vec2 = p.zw();
	
	let time = (params.frame as f32) * 0.008;
	
	// ── Pulsing vortex strengths ─────────────────────────────────────────────
	
	let pulse0 = 0.8 + 0.4 * sin(time * 1.3);
	let pulse1 = 0.6 + 0.3 * sin(time * 1.7 + 1.0);
	let pulse2 = 0.5 + 0.3 * sin(time * 2.1 + 2.5);
	
	// ── Orbiting vortex centers ──────────────────────────────────────────────
	
	let c0 = vec2(0.0, 0.0);
	let c1 = vec2(cos(time * 0.7) * 0.35, sin(time * 0.7) * 0.35);
	let c2 = vec2(cos(time * 0.5 + 2.1) * 0.3, sin(time * 0.5 + 2.1) * 0.3);
	
	let mut force = vec2(0.0, 0.0);
	
	// ── Vortex 0 ─────────────────────────────────────────────────────────────
	
	let delta0 = c0 - pos;
	let dist0  = delta0.length() + 0.001;
	
	let dir0 = delta0 / dist0;
	let tan0 = vec2(-dir0.y, dir0.x);
	
	let pull0  = pulse0 * 0.3 / (dist0 + 0.08);
	let swirl0 = pulse0 * 0.9 / (dist0 * dist0 + 0.015);
	let repel0 = -0.015 / (dist0 * dist0 * dist0 + 0.0005);
	
	force += dir0 * (pull0 + repel0) + tan0 * swirl0;
	
	// ── Vortex 1 ─────────────────────────────────────────────────────────────
	
	let delta1 = c1 - pos;
	let dist1  = delta1.length() + 0.001;
	
	let dir1 = delta1 / dist1;
	let tan1 = vec2(-dir1.y, dir1.x);
	
	let pull1  = pulse1 * 0.3 / (dist1 + 0.08);
	let swirl1 = pulse1 * 0.9 / (dist1 * dist1 + 0.015);
	let repel1 = -0.015 / (dist1 * dist1 * dist1 + 0.0005);
	
	force += dir1 * (pull1 + repel1) + tan1 * swirl1;
	
	// ── Vortex 2 ─────────────────────────────────────────────────────────────
	
	let delta2 = c2 - pos;
	let dist2  = delta2.length() + 0.001;
	
	let dir2 = delta2 / dist2;
	let tan2 = vec2(-dir2.y, dir2.x);
	
	let pull2  = pulse2 * 0.3 / (dist2 + 0.08);
	let swirl2 = pulse2 * 0.9 / (dist2 * dist2 + 0.015);
	let repel2 = -0.015 / (dist2 * dist2 * dist2 + 0.0005);
	
	force += dir2 * (pull2 + repel2) + tan2 * swirl2;
	
	// ── Periodic radial burst ────────────────────────────────────────────────
	
	let burst_phase = sin(time * 2.0);
	
	if burst_phase > 0.95 {
		let dist_center = pos.length() + 0.01;
		let outward     = pos / dist_center;
		
		force += outward * 2.0 * (burst_phase - 0.95) * 20.0;
	}
	
	// ── Multi-octave curl noise ──────────────────────────────────────────────
	
	let mut noise = vec2(0.0, 0.0);
	
	noise += curl(pos * 2.5,  time * 0.4) * 0.5;
	noise += curl(pos * 6.0,  time * 0.9) * 0.2;
	noise += curl(pos * 14.0, time * 1.5) * 0.08;
	
	// ── Rectangular boundary (prevents particles leaving NDC box) ────────────
	
	let bound = 0.85_f32;
	let boundary_strength = 3.0_f32;
	
	if pos.x > bound {
		force.x -= (pos.x - bound) * boundary_strength;
	}
	
	if pos.x < -bound {
		force.x -= (pos.x + bound) * boundary_strength;
	}
	
	if pos.y > bound {
		force.y -= (pos.y - bound) * boundary_strength;
	}
	
	if pos.y < -bound {
		force.y -= (pos.y + bound) * boundary_strength;
	}
	
	// ── Integrate velocity ───────────────────────────────────────────────────
	
	vel += (force + noise) * params.dt;
	
	// ── Speed-dependent drag ─────────────────────────────────────────────────
	
	let speed = vel.length();
	let drag  = mix(0.997, 0.965, clamp(speed * 1.5, 0.0, 1.0));
	
	vel *= drag;
	
	// ── Soft circular boundary ───────────────────────────────────────────────
	
	let r = pos.length();
	
	if r > 0.95 {
		vel -= pos * (r - 0.85) * 3.0 * params.dt;
		vel *= 0.95;
	}
	
	// ── Integrate position ───────────────────────────────────────────────────
	
	pos += vel * params.dt;
	
	particles_out[idx] = vec4_2v(pos, vel);
}

pub const COMP_SHADER: &str = PARTICLE_COMP_GLSL;

pub static COMP_SPV:    &[u8] = PARTICLE_COMP_SPV;