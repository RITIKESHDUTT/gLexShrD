use glex_shader_types::Vec4o1;
use glex_shader_types::Vec2o0;
use glex_shader_types::Vec4o0;
use glex_shader_types::Vec4i1;
use glex_shader_types::Vec2i0;
use crate::renderer::pipelines::TextPush;
use crate::renderer::prelude::*;

#[fragment_shader(bind = GlyphAtlas)]
fn text_frag(
	atlas: Sampler2D,     // descriptor
	frag_uv: Vec2i0,       // in location = 0
	frag_color: Vec4i1,    // in location = 1
	out_color: Vec4o0,      // out location = 0
) {
	let alpha: f32 = texture(atlas, frag_uv).r;
	
	out_color = Vec4::new(
		frag_color.x,
		frag_color.y,
		frag_color.z,
		frag_color.w * alpha,
	);
}

// Push constant driven glyph vertex shader.
// Input: unit quad vertices [0,1]x[0,1].
// Push constants position the glyph in pixel coords and map UV from atlas.


#[vertex_shader(vertex = Vertex2D)]
fn text_vert(
	// Input: The vertex of the unit quad [0..1]
	input_position: Vec2,
	input_texture: Vec2,
	#[push_constant] pc: &TextPush,
	// Outputs: Values passed to the Fragment Shader
	mut frag_uv: Vec2o0,
	mut frag_color:  Vec4o1,
	// Built-in: The final coordinate for the GPU
	mut out_position: Vec4
) {
	// 1. Calculate the pixel coordinate (Top-Left + Offset)
	let pixel: Vec2 = pc.glyph_pos + (input_position * pc.glyph_size);
	
	// 2. Map Pixels to NDC Space (-1.0 to 1.0)
	let ndc: Vec2 = (pixel / pc.screen_size) * 2.0 - 1.0;
	
	// 3. Map Unit UV [0..1] to Atlas UV [origin..origin+size]
	let uv: Vec2 = pc.uv_origin + (input_texture * pc.uv_size);
	
	// 4. Assign to outputs
	out_position   = vec4(ndc, 0.0, 1.0);
	frag_uv    = uv;
	frag_color = pc.color;
}

pub const VERT_SHADER: &str = TEXT_VERT_GLSL;
pub const FRAG_SHADER: &str = TEXT_FRAG_GLSL;


pub static VERT_SPV:    &[u8] = TEXT_VERT_SPV;
pub static FRAG_SPV:    &[u8] = TEXT_FRAG_SPV;