//! Seeded integer gradient noise (W3a, P-136): Perlin's, on a lattice, in fixed point, the
//! same on every machine; and its octaves summed.

/// Binary places in the fixed point the noise is worked in.
pub(crate) const POINT: u32 = 24;

/// One, in that fixed point.
const ONE: i128 = 1 << POINT;

/// The gradients a lattice point can have: the four diagonals and the four axes.
const GRADIENTS: [[i128; 2]; 8] = [
    [1, 1],
    [-1, 1],
    [1, -1],
    [-1, -1],
    [1, 0],
    [-1, 0],
    [0, 1],
    [0, -1],
];

/// SplitMix64's step: `z` mixed so that every bit of it moves every bit of the result.
fn mix(z: u64) -> u64 {
    let z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The gradient at lattice point `x, y` of `octave`, picked by the seed.
fn gradient(seed: u32, octave: u32, [x, y]: [u64; 2]) -> [i128; 2] {
    let hash = [u64::from(octave), x, y]
        .into_iter()
        .fold(mix(u64::from(seed)), |hash, part| mix(hash ^ part));
    GRADIENTS[(hash % 8) as usize]
}

/// Perlin's fade, 6t⁵ − 15t⁴ + 10t³, of `t` in fixed point.
fn fade(t: i128) -> i128 {
    let cube = (((t * t) >> POINT) * t) >> POINT;
    let inner = ((t * (6 * t - 15 * ONE)) >> POINT) + 10 * ONE;
    (cube * inner) >> POINT
}

/// From `a` to `b` by `f`, in fixed point.
fn lerp(a: i128, b: i128, f: i128) -> i128 {
    a + (((b - a) * f) >> POINT)
}

/// The noise of `octave` at `at`, on a lattice `wavelength` apart, both in the same units, at
/// or past the lattice's origin: −1 to 1 in fixed point, 0 on a lattice point. Each octave has
/// twice the frequency of the one before, on a lattice of its own.
fn octave(seed: u32, octave: u32, at: [i128; 2], wavelength: i128) -> i128 {
    let scaled = at.map(|c| c << octave);
    let cell = scaled.map(|c| u64::try_from(c / wavelength).expect("past the origin"));
    let t = scaled.map(|c| ((c % wavelength) << POINT) / wavelength);
    let dot = |dx: u64, dy: u64| {
        let [gx, gy] = gradient(seed, octave, [cell[0] + dx, cell[1] + dy]);
        gx * (t[0] - i128::from(dx) * ONE) + gy * (t[1] - i128::from(dy) * ONE)
    };
    let [fx, fy] = t.map(fade);
    lerp(
        lerp(dot(0, 0), dot(1, 0), fx),
        lerp(dot(0, 1), dot(1, 1), fx),
        fy,
    )
}

/// The `octaves` summed at `at`, each half the weight of the one before, the first's
/// `2^(octaves − 1)`: their total weight is `2^octaves − 1`, by which the sum divides to lie
/// within −1 to 1.
pub(crate) fn octaves(seed: u32, octaves: u32, at: [i128; 2], wavelength: i128) -> i128 {
    (0..octaves)
        .map(|k| octave(seed, k, at, wavelength) << (octaves - 1 - k))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_runs_from_zero_to_one_through_a_half() {
        assert_eq!(fade(0), 0);
        assert_eq!(fade(ONE / 2), ONE / 2);
        assert_eq!(fade(ONE), ONE);
        // 6/1024 − 15/256 + 10/64 at a quarter: 0.103515625.
        assert_eq!(fade(ONE / 4), 1_736_704);
    }

    #[test]
    fn mix_is_splitmix64() {
        // SplitMix64's first output from a state of 0, as its reference gives it.
        assert_eq!(mix(0), 0xe220_a839_7b1d_cdaf);
        assert_eq!(lerp(ONE, 3 * ONE, ONE / 2), 2 * ONE);
    }

    #[test]
    fn the_noise_is_zero_on_the_lattice_and_halfway_takes_each_corner() {
        assert_eq!(octave(7, 0, [800, 1600], 800), 0);
        // Halfway, each corner's dot product counts a quarter.
        let corners: i128 = [[0, 0], [1, 0], [0, 1], [1, 1]]
            .into_iter()
            .map(|[dx, dy]| {
                let [gx, gy] = gradient(7, 0, [1 + dx, 2 + dy]);
                gx * (ONE / 2 - i128::from(dx) * ONE) + gy * (ONE / 2 - i128::from(dy) * ONE)
            })
            .sum();
        assert_eq!(octave(7, 0, [1200, 2000], 800), corners / 4);
    }
}
