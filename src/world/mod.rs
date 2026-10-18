//! World: streams chunks around the player with a bounded per-frame
//! generation budget, renders only what is visible, and answers height
//! queries for the physics.
//!
//! Performance model:
//! - Terrain/decoration queries are *pure functions* of (x, z): physics
//!   never waits on chunk generation.
//! - Chunks are meshed once on demand (`CHUNKS_PER_FRAME` per frame,
//!   nearest first) so streaming never causes an FPS spike.
//! - Rendering culls by distance and rejects chunks behind the camera
//!   before any vertex work happens.

use std::collections::HashMap;

use macroquad::prelude::*;

use crate::config;
use crate::procedural::terrain::Terrain;
use chunk::{Chunk, SIZE};

pub mod chunk;

pub struct World {
    /// Pure-function terrain generator (heightmap, humidity, slope).
    pub terrain: Terrain,
    seed: u64,
    /// Loaded chunks around the player.
    chunks: HashMap<(i32, i32), Chunk>,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            terrain: Terrain::new(seed),
            seed,
            chunks: HashMap::new(),
        }
    }

    /// Height of the walkable surface at `(x, z)`. Delegates to the pure
    /// terrain function, so it works even for chunks that were not
    /// generated yet - physics can never deadlock on the streaming.
    pub fn get_height_at(&self, x: f32, z: f32) -> f32 {
        self.terrain.get_height_at(x, z)
    }

    /// Number of chunks currently resident in memory.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Synchronously generates the chunks around `pos`. Called once at
    /// startup (loading) so the first frames are hitch-free.
    pub fn warm_up(&mut self, pos: Vec3, radius: i32) {
        for (cx, cz) in self.missing_within(pos, radius) {
            self.generate(cx, cz);
        }
    }

    /// Streaming update, called once per frame:
    /// unloads far chunks and generates at most `CHUNKS_PER_FRAME` new
    /// ones, nearest to the player first.
    pub fn update(&mut self, player_pos: Vec3) {
        self.unload_far(player_pos);
        for (cx, cz) in self
            .missing_within(player_pos, config::RENDER_DISTANCE)
            .into_iter()
            .take(config::CHUNKS_PER_FRAME)
        {
            self.generate(cx, cz);
        }
    }

    /// Renders every visible chunk plus the sea. `time` (seconds) drives
    /// the water animation.
    ///
    /// Draw order matters: all opaque geometry first, then the translucent
    /// sea pass on top - that way the water blends *over* the seabed
    /// (depth testing rejects the hidden quads, so no sorting needed).
    pub fn render(&self, cam_pos: Vec3, cam_front: Vec3, time: f32) {
        let max_dist = (config::RENDER_DISTANCE as f32 + 0.75) * SIZE as f32;
        let (pcx, pcz) = Self::player_chunk(cam_pos);

        // --- Opaque pass: chunk meshes (collected water quads are drawn later)
        let mut water_quads: Vec<(&Chunk, f32)> = Vec::with_capacity(64);
        for c in self.chunks.values() {
            // Cull: too far behind the camera, or outside the draw radius
            let to_chunk = c.center - cam_pos;
            if to_chunk.length() > max_dist + c.radius {
                continue;
            }
            if to_chunk.dot(cam_front) < -c.radius {
                continue;
            }

            draw_mesh(&c.mesh);
            if let Some(depth) = c.water_depth {
                water_quads.push((c, depth));
            }
        }

        // --- Translucent pass: the sea -----------------------------------
        // Open sea: one huge quad centred on the player chunk, extending
        // past the loaded radius so the horizon is always water.
        let center_x = pcx as f32 * SIZE as f32 + SIZE as f32 * 0.5;
        let center_z = pcz as f32 * SIZE as f32 + SIZE as f32 * 0.5;
        let extent = max_dist + 48.0;
        draw_plane(
            vec3(
                center_x,
                config::SEA_LEVEL + 0.35 + 0.03 * (time * 0.7).sin(),
                center_z,
            ),
            vec2(extent, extent),
            None,
            Color::from_rgba(28, 92, 165, 210),
        );

        // Depth-shaded sea quads on top of the open-sea plane. Chunks far
        // away are blended towards the horizon color (cheap aerial
        // perspective that hides the render-distance edge).
        for (c, depth) in water_quads {
            let ring = (c.cx - pcx).abs().max((c.cz - pcz).abs()) as f32;
            let haze = (ring / config::RENDER_DISTANCE as f32).min(1.0) * 0.75;
            let t = (depth / 12.0).min(1.0);
            let water = Color::from_rgba(
                (52.0 - 30.0 * t) as u8,
                (135.0 - 60.0 * t) as u8,
                (200.0 - 45.0 * t) as u8,
                165,
            );
            let bob = 0.02 * (time * 1.3 + (c.cx + c.cz) as f32 * 0.6).sin();
            draw_plane(
                vec3(c.center.x, config::SEA_LEVEL + 0.45 + bob, c.center.z),
                vec2(SIZE as f32 * 0.5 + 0.03, SIZE as f32 * 0.5 + 0.03),
                None,
                lerp_color(water, Color::from_rgba(150, 180, 215, 255), haze),
            );
        }
    }

    /// Finds a pleasant spawn point: dry, flat ground, spiralling out
    /// from the island centre. Falls back to the centre itself.
    pub fn find_spawn(&self) -> Vec3 {
        for ring in 0..=40i32 {
            let steps = if ring == 0 { 1 } else { ring * 8 };
            for i in 0..steps {
                let a = i as f32 / steps as f32 * std::f32::consts::TAU;
                let x = a.cos() * (ring * 4) as f32;
                let z = a.sin() * (ring * 4) as f32;
                let h = self.get_height_at(x, z);
                if h > config::SEA_LEVEL + 1.0 && self.terrain.slope_at(x, z) <= 1.5 {
                    return vec3(x, h + config::PLAYER_HEIGHT / 2.0, z);
                }
            }
        }
        vec3(
            0.0,
            self.get_height_at(0.0, 0.0) + config::PLAYER_HEIGHT / 2.0,
            0.0,
        )
    }

    fn player_chunk(pos: Vec3) -> (i32, i32) {
        (
            (pos.x / SIZE as f32).floor() as i32,
            (pos.z / SIZE as f32).floor() as i32,
        )
    }

    fn generate(&mut self, cx: i32, cz: i32) {
        let chunk = chunk::build(cx, cz, &self.terrain, self.seed);
        self.chunks.insert((cx, cz), chunk);
    }

    /// Missing chunks inside a circular radius around the player, sorted
    /// nearest ring first.
    fn missing_within(&self, pos: Vec3, radius: i32) -> Vec<(i32, i32)> {
        let (pcx, pcz) = Self::player_chunk(pos);
        let mut missing: Vec<(i32, i32, i32)> = Vec::new(); // (ring, cx, cz)
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                let ring = dx.abs().max(dz.abs());
                if ring > radius {
                    continue; // circular, not square, loading area
                }
                let (cx, cz) = (pcx + dx, pcz + dz);
                if !self.chunks.contains_key(&(cx, cz)) {
                    missing.push((ring, cx, cz));
                }
            }
        }
        missing.sort_by_key(|&(ring, _, _)| ring);
        missing.into_iter().map(|(_, cx, cz)| (cx, cz)).collect()
    }

    /// Drops chunks that fell out of the streaming radius to keep memory
    /// bounded while the player explores.
    fn unload_far(&mut self, pos: Vec3) {
        let (pcx, pcz) = Self::player_chunk(pos);
        let keep = config::RENDER_DISTANCE + 1;
        self.chunks
            .retain(|&(cx, cz), _| (cx - pcx).abs() <= keep && (cz - pcz).abs() <= keep);
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let ch = |x: f32, y: f32| (x + (y - x) * t).clamp(0.0, 255.0) as u8;
    Color::from_rgba(
        ch(a.r * 255.0, b.r * 255.0),
        ch(a.g * 255.0, b.g * 255.0),
        ch(a.b * 255.0, b.b * 255.0),
        ch(a.a * 255.0, b.a * 255.0),
    )
}
