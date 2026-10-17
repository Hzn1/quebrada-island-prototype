use macroquad::prelude::*;
use macroquad::material::{load_material, MaterialParams, gl_use_material, gl_use_default_material};

// GLSL vertex shader for macroquad materials: transforms the quad with the
// Model/Projection matrices and passes the texture coordinates along.
const CRT_VERTEX_SHADER: &str = r#"
#version 100

attribute vec3 position;
attribute vec2 texcoord;

uniform mat4 Model;
uniform mat4 Projection;

varying vec2 v_tex_coords;

void main() {
    v_tex_coords = texcoord;
    gl_Position = Projection * Model * vec4(position, 1.0);
}
"#;

// GLSL fragment shader for CRT monitor effect.
// - Horizontal scanlines
// - Light vignette on edges
const CRT_FRAGMENT_SHADER: &str = r#"
#version 100
precision highp float;

varying vec2 v_tex_coords;

// "Texture" is reserved by macroquad: it is automatically bound to the
// texture being drawn (the render target holding the scene).
uniform sampler2D Texture;
uniform vec2 resolution;

// Simple vignette function: 1.0 at the center, 0.0 at the edges.
float vignette(float strength) {
    vec2 centered = v_tex_coords * 2.0 - 1.0;
    // pow() with a negative base is undefined in GLSL; the screen corners
    // have radius > 1, where 1 - r*r goes negative.
    return pow(max(0.0, 1.0 - dot(centered, centered)), strength);
}

void main() {
    // Read the scene from the low-res render target
    vec4 tex_color = texture2D(Texture, v_tex_coords);

    // Scanlines darken alternate bands of the RGB color. The alpha channel
    // is left untouched: zeroing it would make the lines disappear (the
    // material has blending disabled) or punch holes in it (if enabled).
    // Period of 3 texels of the 640x360 target => ~120 lines.
    float band = fract(v_tex_coords.y * resolution.y / 3.0);
    float scanline = mix(0.7, 1.0, smoothstep(0.4, 0.6, band));

    // Vignette dims the corners slightly: v = 1.0 in the center, so
    // intensity = 1.0 - (1.0 - v) * 0.3 = 0.7 at the edges, 1.0 in the center.
    float intensity = mix(0.7, 1.0, vignette(2.0));

    gl_FragColor = vec4(tex_color.rgb * intensity * scanline, tex_color.a);
}
"#;

/// CRT post-processing filter that applies a CRT monitor effect to the rendered scene.
/// The scene is rendered into a low-resolution render target, which is then drawn
/// to the screen with the CRT shader applied, creating a scanline/vignette look.
pub struct CrtFilter {
    /// Low-resolution target the scene is rendered into.
    render_target: RenderTarget,
    /// Material for the CRT effect.
    material: macroquad::material::Material,
    /// Width of the render target in pixels.
    width: u32,
    /// Height of the render target in pixels.
    height: u32,
}

impl CrtFilter {
    /// Creates a new CRT filter with the given render target dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        // The scene is rendered into this low-resolution target and then
        // upscaled to the screen by the CRT shader.
        let render_target = render_target(width, height);
        render_target.texture.set_filter(FilterMode::Nearest);

        let material = load_material(
            ShaderSource::Glsl {
                vertex: CRT_VERTEX_SHADER,
                fragment: CRT_FRAGMENT_SHADER,
            },
            MaterialParams {
                pipeline_params: Default::default(),
                uniforms: vec![
                    UniformDesc {
                        name: "resolution".to_string(),
                        uniform_type: UniformType::Float2,
                        array_count: 1,
                    }
                ],
                ..Default::default()
            }
        ).expect("Failed to create CRT material");

        Self {
            render_target,
            material,
            width,
            height,
        }
    }

    /// Points the camera at the filter's low-resolution render target.
    ///
    /// The scene must be drawn after this call and before `draw_to_screen`.
    pub fn begin_render(&self, mut camera: Camera3D) {
        camera.render_target = Some(self.render_target.clone());
        set_camera(&camera);
    }

    /// Draws the stored render target with the CRT shader applied.
    pub fn draw_to_screen(&self) {
        // Back to the default camera so this pass is drawn to the screen.
        set_default_camera();

        // Use our custom material
        gl_use_material(&self.material);

        // Set the resolution uniform (target size, used for the scanline density)
        // NOTE: pass the Vec2 by value. A reference is also 8 bytes, so it
        // would pass set_uniform's size check and copy the pointer instead.
        let res = vec2(self.width as f32, self.height as f32);
        self.material.set_uniform("resolution", res);

        // Draw a full-screen rectangle with the render target stretched over it.
        //
        // flip_y is required: a scene rendered with Camera3D into a render
        // target comes out vertically flipped (macroquad's Camera2D compensates
        // for this internally, but Camera3D does not), so it must be flipped
        // back when blitting it to the screen.
        draw_texture_ex(
            &self.render_target.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                flip_y: true,
                ..Default::default()
            },
        );

        // Reset to default material
        gl_use_default_material();
    }
}
