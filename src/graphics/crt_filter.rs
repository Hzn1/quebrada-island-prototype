use macroquad::prelude::*;

// GLSL fragment shader for CRT monitor effect.
// - Horizontal scanlines
// - Light vignette on edges
// - Subtle chromatic aberration/distortion
const CRT_FRAGMENT_SHADER: &str = r#"
#version 300 es
precision highp float;

in vec2 v_tex_coords;
out vec4 frag_color;

uniform vec2 resolution;

// Simple vignette function
float vignette(float strength) {
    vec2 centered = v_tex_coords * 2.0 - 1.0;
    float radius = length(centered);
    return pow(1.0 - radius * radius, strength);
}

void main() {
    // Scanlines: alternate bands create the scanline effect
    float scanline = fract(v_tex_coords.y * resolution.y * 1.0 / 64.0);
    float alpha = step(0.5, scanline); // half the bands are "on"

    // Vignette effect
    float v = vignette(2.0);

    // Read the texture sample from the render target
    vec4 tex_color = texture2D(v_resolution, v_tex_coords);

    // Modulate the texture color with vignette and scanlines
    float intensity = 1.0 - v * 0.3; // dim corners slightly
    frag_color = vec4(tex_color.rgb * intensity, alpha * tex_color.a);
}
"#;

/// CRT post-processing filter that applies a CRT monitor effect to the rendered scene.
/// The filter renders the scene to an off-screen texture and then draws it to the screen
/// with the CRT shader applied, creating a scanline/vignette look.
pub struct CrtFilter {
    /// The render target where the game scene is drawn first.
    render_target: RenderTexture2D,
    /// Shader handle for the CRT effect.
    shader: Shader,
    /// Width of the render target in pixels.
    width: u32,
    /// Height of the render target in pixels.
    height: u32,
}

impl CrtFilter {
    /// Creates a new CRT filter with the given dimensions.
    pub async fn new(width: u32, height: u32) -> Self {
        let mut rt = RenderTexture2D::default();
        rt.resize(width, height);

        // Load the shader from the embedded string
        let shader = load_string(CRT_FRAGMENT_SHADER)
            .await
            .expect("Failed to load CRT shader");

        Self {
            render_target: rt,
            shader,
            width,
            height,
        }
    }

    /// Begins rendering to the CRT render target.
    /// All subsequent draw calls will accumulate into the texture instead of the screen.
    pub fn begin_render(&mut self) {
        set_render_target(Some(self.render_target));
        clear_background(BLACK);
    }

    /// Ends rendering to the target and returns to screen space.
    pub fn end_render(&mut self) {
        set_render_target(None);
    }

    /// Draws the CRT-filtered render target to the screen with the shader applied.
    pub fn draw_to_screen(&self) {
        // Apply the shader
        use_macroquad::shader::use_shader(Some(self.shader));

        // Set the resolution uniform
        let res = vec2(self.width as f32, self.height as f32);
        set_shader_value(self.shader, "resolution", &res);

        // Draw the render texture covering the entire screen
        // Use draw_texture_ex with the shader active
        draw_texture_ex(
            self.render_target.texture.unwrap(),
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                source: Some(Rect::new(0.0, 0.0, self.width as f32, self.height as f32)),
                dest: None,
                rotation: 0.0,
                flip_x: false,
                flip_y: false,
            },
        );

        // Reset shader
        use_macroquad::shader::use_shader(None);
    }
}
