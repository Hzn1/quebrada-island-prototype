//! Advanced island heightmap built from layered noise.
//!
//! Height pipeline (evaluated per column, in this order):
//! 1. domain warp      -> organic, non-circular coastline
//! 2. radial island mask -> island shape, shelf falling into the sea
//! 3. fBm hills        -> rolling base relief
//! 4. central basin    -> "vale central" around the island centre
//! 5. river carving    -> dendritic valleys along noise zero-crossings
//! 6. ridged mountains -> sharp crests + steep cliff bands (penhascos)
//! 7. detail noise     -> small surface bumps
//!
//! Every method is a pure function of (x, z): the terrain costs no memory,
//! and any system (chunk meshing, physics, decoration) can query any
//! column without needing the chunk to exist.

use crate::config;
use crate::procedural::noise::Perlin;

pub struct Terrain {
    /// Rolling base relief and the "mountain gate" (same noise, different
    /// offsets, so highlands correlate with the hills below them).
    hills: Perlin,
    /// Low-frequency noise used to warp the coastline into organic shapes.
    warp: Perlin,
    /// Ridged noise: sharp crests for mountains and cliffs.
    ridge: Perlin,
    /// Zero-crossing noise carved into dendritic river valleys.
    rivers: Perlin,
    /// Humidity/rain map driving biomes and vegetation density.
    moisture: Perlin,
    /// High frequency bumps + coastline radius modulation.
    detail: Perlin,
    /// Soft "town mask" noise used by the favela generator to break the
    /// hillside into clusters of houses instead of a uniform carpet.
    city: Perlin,
}

/// Smooth Hermite interpolation between two edges.
/// Works with `edge0 > edge1` too (used for the inverted island mask).
#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Terrain {
    /// Creates the terrain generator for a given world seed.
    pub fn new(seed: u64) -> Self {
        Self {
            hills: Perlin::new(seed + 0),
            warp: Perlin::new(seed + 1),
            ridge: Perlin::new(seed + 2),
            rivers: Perlin::new(seed + 3),
            moisture: Perlin::new(seed + 4),
            detail: Perlin::new(seed + 5),
            city: Perlin::new(seed + 6),
        }
    }

    /// Raw (continuous) surface height in world units. May dip below
    /// `SEA_LEVEL` (inlets/lakes) and reaches ~45 at the highest peaks.
    pub fn raw_height(&self, x: f32, z: f32) -> f32 {
        // 1) Domain warp: displaces the sample point so the coastline and
        //    all features become organic instead of grid-aligned.
        let wx = x + self.warp.noise2(x * 0.006, z * 0.006) * 26.0;
        let wz = z + self.warp.noise2(x * 0.006 + 313.7, z * 0.006 - 127.1) * 26.0;

        // 2) Island mask: 1 inland -> 0 at the noisy coast. The wide falloff
        //    (1.0 -> 0.55) creates the shallow shelf where beaches form.
        let radius_mod = 1.0 + 0.30 * self.detail.noise2(wx * 0.007, wz * 0.007);
        let d = (wx * wx + wz * wz).sqrt() / (config::ISLAND_RADIUS * radius_mod);
        let mask = smoothstep(1.0, 0.55, d);

        // 3) Rolling hills (fBm, 5 octaves)
        let hills01 = self.hills.fbm(wx * 0.010, wz * 0.010, 5, 2.0, 0.5) * 0.5 + 0.5;
        let mut h = hills01 * 15.0;

        // 4) Vale central: broad depression around the island centre
        h -= smoothstep(0.34, 0.02, d) * 7.0;

        // 5) River carving: fBm zero-crossings make thin, branching valleys
        let river = self.rivers.fbm(wx * 0.0045, wz * 0.0045, 3, 2.0, 0.5);
        h -= (1.0 - smoothstep(0.0, 0.05, river.abs())) * 6.0;

        // 6) Mountains. The "gate" uses a *narrow* smoothstep band, so
        //    wherever it crosses its threshold the elevation jumps ~20
        //    blocks over a few units -> steep escarpments (penhascos).
        //    Ridged noise on top gives the crests their sharp texture.
        let gate = self
            .hills
            .fbm(wx * 0.013 + 91.3, wz * 0.013 - 44.9, 4, 2.1, 0.5)
            * 0.5
            + 0.5;
        let mountain = smoothstep(0.52, 0.62, gate);
        let ridge = self.ridge.ridged(wx * 0.012, wz * 0.012, 4, 2.0, 0.5);
        h += mountain * ridge * 24.0;

        // 7) Fine surface detail
        h += self.detail.noise2(x * 0.08, z * 0.08) * 0.55;

        // Assemble: deep sea floor outside the island, land inside. The
        // mix happens in *height* space so the coast ends submerged.
        let sea_floor = config::SEA_LEVEL - 5.0 - 9.0 * smoothstep(1.0, 1.5, d);
        let land = config::SEA_LEVEL + h;
        sea_floor + (land - sea_floor) * mask
    }

    /// Quantized surface height: the top of the highest solid block of the
    /// column. This is what the player walks on and what the mesher draws,
    /// so physics and rendering can never disagree.
    pub fn get_height_at(&self, x: f32, z: f32) -> f32 {
        self.raw_height(x, z).round()
    }

    /// Humidity/rain map in 0..1 (drives biome choice and tree density).
    pub fn humidity_at(&self, x: f32, z: f32) -> f32 {
        self.moisture.fbm(x * 0.006, z * 0.006, 3, 2.0, 0.5) * 0.5 + 0.5
    }

    /// Steepest slope at this column: the max height difference to the 4
    /// direct neighbours, in blocks. Used for cliff detection and for
    /// decoration placement ("nothing grows on a wall").
    pub fn slope_at(&self, x: f32, z: f32) -> f32 {
        let h = self.get_height_at(x, z);
        let dx = (self.get_height_at(x + 1.0, z) - h).abs();
        let dz = (self.get_height_at(x, z + 1.0) - h).abs();
        dx.max(dz)
    }

    /// Urban (favela) density in `0..1`, driving both the paving of the
    /// ground and where houses are allowed to spawn.
    ///
    /// `height` is the already-known column height, so this costs a single
    /// cheap noise lookup instead of a second full terrain evaluation - it is
    /// called once per column while meshing, so that matters.
    ///
    /// The town grows with altitude (nothing on the beach/water), and the
    /// `city` noise carves clusters and empty patches so the hillside is not
    /// uniformly covered.
    pub fn urban_density(&self, height: f32, x: f32, z: f32) -> f32 {
        // 0 at the shoreline, 1 up on the morro.
        let base = ((height - config::SEA_LEVEL - 1.0) / 10.0).clamp(0.0, 1.0);
        let mask = self.city.fbm(x * 0.010, z * 0.010, 2, 2.0, 0.5) * 0.5 + 0.5;
        (base * (0.50 + 0.50 * mask)).clamp(0.0, 1.0)
    }
}
