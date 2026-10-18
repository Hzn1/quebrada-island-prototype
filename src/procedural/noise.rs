//! Deterministic pseudo-random noise used by the procedural generator.
//!
//! Everything here is implemented from scratch (no external crates):
//! - `Perlin`: classic 2D gradient noise with a seeded permutation table.
//! - `Perlin::fbm`: fractal Brownian motion (layered octaves) for natural
//!   rolling terrain.
//! - `Perlin::ridged`: ridged multifractal noise, ideal for sharp mountain
//!   crests and cliff faces.
//! - `hash2` / `hash01`: fast integer hashes used to scatter decorations.

use std::f32::consts::SQRT_2;

/// The permutation table holds 256 entries duplicated to 512 so that
/// `perm[x] + y` never needs a second wrap for `y < 256`.
const PERM_SIZE: usize = 512;

/// Classic 2D gradient (Perlin) noise with a seeded permutation table.
/// All methods are pure: the same seed always yields the same values,
/// which is what makes the world reproducible across chunks.
pub struct Perlin {
    perm: [u8; PERM_SIZE],
}

/// mulberry32: a tiny, fast, deterministic PRNG. Not cryptographically
/// secure - only used to shuffle the permutation table at construction.
fn mulberry32(state: &mut u32) -> u32 {
    *state = state.wrapping_add(0x9E37_79B9);
    let mut z = *state;
    z = (z ^ (z >> 16)).wrapping_mul(0x21F0_AAAD);
    z = (z ^ (z >> 15)).wrapping_mul(0x735A_2D97);
    z ^ (z >> 15)
}

/// Linear interpolation.
#[inline]
fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + (b - a) * t
}

/// Picks one of 8 gradient directions and returns its dot product with
/// the offset vector. Cheap and artifact-free for terrain frequencies.
#[inline]
fn grad(hash: usize, x: f32, y: f32) -> f32 {
    match hash & 7 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x,
        5 => -x,
        6 => y,
        _ => -y,
    }
}

impl Perlin {
    /// Builds the shuffled permutation table for `seed`.
    pub fn new(seed: u64) -> Self {
        // Base table: identity permutation
        let mut p = [0u8; 256];
        for (i, v) in p.iter_mut().enumerate() {
            *v = i as u8;
        }
        // Fisher-Yates shuffle driven by the seeded PRNG
        let mut state = (seed as u32) ^ ((seed >> 32) as u32);
        for i in (1..256).rev() {
            let j = (mulberry32(&mut state) as usize) % (i + 1);
            p.swap(i, j);
        }
        // Duplicate to 512 entries to avoid wrapping on the y axis
        let mut perm = [0u8; PERM_SIZE];
        for i in 0..PERM_SIZE {
            perm[i] = p[i & 255];
        }
        Self { perm }
    }

    /// Classic Perlin gradient noise. Returns approximately [-1, 1].
    pub fn noise2(&self, x: f32, y: f32) -> f32 {
        // Integer lattice cell containing the point
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        // Position inside the cell, [0, 1)
        let xf = x - xi as f32;
        let yf = y - yi as f32;

        // Fade curve (smootherstep): C2 continuous, removes grid artefacts
        let u = xf * xf * xf * (xf * (xf * 6.0 - 15.0) + 10.0);
        let v = yf * yf * yf * (yf * (yf * 6.0 - 15.0) + 10.0);

        // Hash the 4 corners of the cell
        let get = |i: usize| self.perm[i & (PERM_SIZE - 1)] as usize;
        let x0 = (xi as usize) & 255;
        let x1 = ((xi + 1) as usize) & 255;
        let y0 = (yi as usize) & 255;
        let y1 = ((yi + 1) as usize) & 255;
        let aa = get(get(x0) + y0);
        let ba = get(get(x1) + y0);
        let ab = get(get(x0) + y1);
        let bb = get(get(x1) + y1);

        // Blend the 4 gradient contributions
        let x1v = lerp(u, grad(aa, xf, yf), grad(ba, xf - 1.0, yf));
        let x2v = lerp(u, grad(ab, xf, yf - 1.0), grad(bb, xf - 1.0, yf - 1.0));
        // sqrt(2) normalizes the theoretical range of these gradients to ~[-1, 1]
        lerp(v, x1v, x2v) * SQRT_2
    }

    /// Fractal Brownian motion: sums `octaves` layers of `noise2`, each
    /// `lacunarity` times higher in frequency and `gain` times lower in
    /// amplitude. The result is normalized to stay roughly in [-1, 1].
    pub fn fbm(&self, x: f32, y: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
        let mut sum = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut norm = 0.0;
        for _ in 0..octaves {
            sum += self.noise2(x * frequency, y * frequency) * amplitude;
            norm += amplitude;
            frequency *= lacunarity;
            amplitude *= gain;
        }
        sum / norm.max(f32::EPSILON)
    }

    /// Ridged multifractal noise in [0, 1]: every octave is folded around a
    /// ridge (`1 - |n|`) and squared, producing sharp crests separated by
    /// steep faces - exactly what penhascos/mountains need.
    pub fn ridged(&self, x: f32, y: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
        let mut sum = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut norm = 0.0;
        for _ in 0..octaves {
            let n = 1.0 - self.noise2(x * frequency, y * frequency).abs();
            sum += n * n * amplitude; // square -> sharpen the crests
            norm += amplitude;
            frequency *= lacunarity;
            amplitude *= gain;
        }
        sum / norm.max(f32::EPSILON)
    }
}

/// Deterministic hash of a pair of integer coordinates plus a seed.
/// Uses SplitMix64 as an avalanche finalizer so neighbouring cells
/// decorrelate completely (no visible patterns in the scatter).
pub fn hash2(x: i32, z: i32, seed: u64) -> u64 {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

/// Hash of `(x, z, seed)` mapped to the [0, 1) interval.
pub fn hash01(x: i32, z: i32, seed: u64) -> f32 {
    // Keep the top 24 bits -> exact in f32, uniform enough for gameplay
    (hash2(x, z, seed) >> 40) as f32 / (1u64 << 24) as f32
}
