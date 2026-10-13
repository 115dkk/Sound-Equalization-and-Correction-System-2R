//! NumPy 2.5.3 legacy `RandomState` draws used by SciPy differential evolution.
//!
//! Port of numpy/random/{mtrand.pyx,_mt19937.pyx,src/mt19937/mt19937.{c,h},
//! src/distributions/distributions.c} at <https://github.com/numpy/numpy/tree/v2.5.3>.
//! NumPy Developers, BSD-3-Clause; MT19937 carries the notices below.
//! This is integer-seeded RandomState, not NumPy's SeedSequence-based MT19937.
//! It is reproducible, not cryptographically secure.
/*
Copyright (c) 2003-2005, Jean-Sebastien Roy (js@jeannot.org)

The rk_random and rk_seed functions algorithms and the original design of
 the Mersenne Twister RNG:
Copyright (C) 1997 - 2002, Makoto Matsumoto and Takuji Nishimura,
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:
1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.
3. The names of its contributors may not be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT,
INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED
OF THE POSSIBILITY OF SUCH DAMAGE.

Original algorithm for the implementation of rk_interval function from Richard
J. Wagner's implementation of the Mersenne Twister RNG, optimised by Magnus Jonsson.
Constants used in the rk_double implementation by Isaku Wada.

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
 the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:
The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/

use crate::{DspError, invalid};

/// Legacy MT19937 with NumPy's integer-seed initialization and draw order.
#[derive(Debug, Clone)]
pub struct RandomState {
    key: [u32; 624],
    pos: usize,
}

impl RandomState {
    /// `RandomState(seed)` for an integer in `0..=2**32-1`.
    pub fn new(seed: u32) -> Self {
        let mut key = [0; 624];
        key[0] = seed;
        for i in 1..624 {
            key[i] = 1_812_433_253_u32
                .wrapping_mul(key[i - 1] ^ (key[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { key, pos: 624 }
    }

    /// One tempered 32-bit MT19937 output (also the full-width uint32 randint).
    pub fn next_u32(&mut self) -> u32 {
        if self.pos == 624 {
            // In-place twist: the second part uses already-updated state words.
            for i in 0..624 {
                let y = (self.key[i] & 0x8000_0000) | (self.key[(i + 1) % 624] & 0x7fff_ffff);
                self.key[i] = self.key[(i + 397) % 624]
                    ^ (y >> 1)
                    ^ (0_u32.wrapping_sub(y & 1) & 0x9908_b0df);
            }
            self.pos = 0;
        }
        let mut y = self.key[self.pos];
        self.pos += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// A 53-bit double in `[0, 1)`, consuming two 32-bit outputs.
    pub fn random_sample(&mut self) -> f64 {
        let a = self.next_u32() >> 5;
        let b = self.next_u32() >> 6;
        (f64::from(a) * 67_108_864.0 + f64::from(b)) * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Scalar NumPy `uniform(low, high)`, including a draw when `low == high`.
    /// Bounds must be finite and ordered, with a finite difference.
    pub fn uniform(&mut self, low: f64, high: f64) -> Result<f64, DspError> {
        let range = high - low;
        if !low.is_finite() || !high.is_finite() || !range.is_finite() || range < 0.0 {
            return Err(invalid("uniform bounds must be finite and ordered"));
        }
        Ok(low + range * self.random_sample())
    }

    /// NumPy's legacy `randint(high)` for the nonnegative index domain used by DE.
    /// Unlike modern Generator integers this uses masked rejection, not Lemire.
    /// `high == 1` returns zero without advancing the generator.
    pub fn randint(&mut self, high: usize) -> Result<usize, DspError> {
        if high == 0 {
            return Err(invalid("high <= 0"));
        }
        Ok(self.interval(high - 1))
    }

    fn interval(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        let mask = usize::MAX >> max.leading_zeros();
        loop {
            let value = if max <= u32::MAX as usize {
                self.next_u32() as usize
            } else {
                // mt19937_next64: high word first, low word second.
                let high = u64::from(self.next_u32());
                ((high << 32) | u64::from(self.next_u32())) as usize
            } & mask;
            if value <= max {
                return value;
            }
        }
    }

    /// In-place legacy shuffle of a one-dimensional sequence, last index first.
    pub fn shuffle<T>(&mut self, values: &mut [T]) {
        for i in (1..values.len()).rev() {
            let j = self.interval(i);
            values.swap(i, j);
        }
    }

    /// NumPy `permutation(range(n))` (the same stream as `permutation(n)`).
    pub fn permutation(&mut self, n: usize) -> Vec<usize> {
        let mut values: Vec<_> = (0..n).collect();
        self.shuffle(&mut values);
        values
    }
}
