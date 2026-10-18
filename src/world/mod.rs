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
use crate::procedural::structures;
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

    /// Vertical span of the building occupying column `(x, z)`, or `None`
    /// when the column is open. Physics uses it for horizontal collision.
    /// Chunks that are not loaded yet report `None`, so streaming never
    /// blocks the player (the same contract as `get_height_at`).
    pub fn wall_at(&self, x: f32, z: f32) -> Option<(i32, i32)> {
        let (cx, cz) = Self::player_chunk(vec3(x, 0.0, z));
        let c = self.chunks.get(&(cx, cz))?;
        let lx = x.floor() as i32 - cx * SIZE;
        let lz = z.floor() as i32 - cz * SIZE;
        if !(0..SIZE).contains(&lx) || !(0..SIZE).contains(&lz) {
            return None;
        }
        let i = (lx + lz * SIZE) as usize;
        let (lo, hi) = (c.wall_lo[i], c.wall_hi[i]);
        if lo <= hi {
            Some((lo, hi))
        } else {
            None
        }
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

        // --- Opaque pass: chunk meshes (recolher água por chunk) ---
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
            // Overhead wiring: a few line segments per chunk, stroked in the
            // same 3D pass so it depth-tests against the houses.
            for &(a, b) in &c.wires {
                draw_line_3d(a, b, Color::from_rgba(26, 26, 30, 235));
            }
            if let Some(depth) = c.water_depth {
                water_quads.push((c, depth));
            }
        }

        // --- Translucent pass: água por chunk ---
        // Desenhar DEPOIS do opaco. `draw_plane` trata `size` como meia-extensão,
        // por isso usamos S/2 + overlap para cobrir todo o chunk sem fendas.
        for (c, depth) in water_quads {
            let t = (depth / 12.0).min(1.0);
            let water = Color::from_rgba(
                (52.0 - 30.0 * t) as u8,
                (135.0 - 60.0 * t) as u8,
                (200.0 - 45.0 * t) as u8,
                190,
            );
            let bob = 0.02 * (time * 1.3 + (c.cx + c.cz) as f32 * 0.6).sin();
            draw_plane(
                vec3(c.center.x, config::SEA_LEVEL + 0.45 + bob, c.center.z),
                vec2(SIZE as f32 * 0.5 + 0.04, SIZE as f32 * 0.5 + 0.04),
                None,
                water,
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
                if h > config::SEA_LEVEL + 1.0
                    && self.terrain.slope_at(x, z) <= 1.5
                    // Never start inside a house (the walls are solid).
                    && !structures::is_built(x.floor() as i32, z.floor() as i32, &self.terrain, self.seed)
                {
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
