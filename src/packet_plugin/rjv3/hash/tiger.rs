//! Tiger-192 哈希——移植自 `rjtiger.c` (rhash)。
//!
//! **注意**：与 Whirlpool 一样，这里的 Tiger 也是锐捷修改过的变体：初始化状态
//! （IV）并非标准 Tiger IV（`0x158e427ac96b03df / 0xf025c13b8e9da784 /
//! 0xb690ab45c3e21b74`），需逐字节照抄 C 实现，切勿替换成标准实现。

use crate::packet_plugin::rjv3::hash::tiger_sbox::SBOXES;

#[derive(Clone)]
pub struct Tiger {
    length: u64,
    buf: [u8; 64],
    state: [u64; 3],
}

#[inline]
fn round(a: &mut u64, b: &mut u64, c: &mut u64, x: u64, mul: u64) {
    *c ^= x;
    let c0 = *c;
    let t1 = SBOXES[0];
    let t2 = SBOXES[1];
    let t3 = SBOXES[2];
    let t4 = SBOXES[3];
    *a = a.wrapping_sub(
        t1[(c0 & 0xff) as usize]
            ^ t2[((c0 >> 16) & 0xff) as usize]
            ^ t3[((c0 >> 32) & 0xff) as usize]
            ^ t4[((c0 >> 48) & 0xff) as usize],
    );
    *b = b.wrapping_add(
        t4[((c0 >> 8) & 0xff) as usize]
            ^ t3[((c0 >> 24) & 0xff) as usize]
            ^ t2[((c0 >> 40) & 0xff) as usize]
            ^ t1[((c0 >> 56) & 0xff) as usize],
    );
    *b = b.wrapping_mul(mul);
}

fn key_schedule(x: &mut [u64; 8]) {
    x[0] = x[0].wrapping_sub(x[7] ^ 0xa5a5b5a5a5a5a7a5u64);
    x[1] ^= x[0];
    x[2] = x[2].wrapping_add(x[1]);
    x[3] = x[3].wrapping_sub(x[2] ^ (!x[1]).wrapping_shl(19));
    x[4] ^= x[3];
    x[5] = x[5].wrapping_add(x[4]);
    x[6] = x[6].wrapping_sub(x[5] ^ (!x[4]).wrapping_shr(23));
    x[7] ^= x[6];
    x[0] = x[0].wrapping_add(x[7]);
    x[1] = x[1].wrapping_sub(x[0] ^ (!x[7]).wrapping_shl(19));
    x[2] ^= x[1];
    x[3] = x[3].wrapping_add(x[2]);
    x[4] = x[4].wrapping_sub(x[3] ^ (!x[2]).wrapping_shr(23));
    x[5] ^= x[4];
    x[6] = x[6].wrapping_add(x[5]);
    x[7] = x[7].wrapping_sub(x[6] ^ 0x0123456789abcdefu64);
}

/// 执行一轮 pass（内部按 8 个 round 轮转三个寄存器）。
fn pass(a: &mut u64, b: &mut u64, c: &mut u64, x: &[u64; 8], mul: u64) {
    round(a, b, c, x[0], mul);
    round(b, c, a, x[1], mul);
    round(c, a, b, x[2], mul);
    round(a, b, c, x[3], mul);
    round(b, c, a, x[4], mul);
    round(c, a, b, x[5], mul);
    round(a, b, c, x[6], mul);
    round(b, c, a, x[7], mul);
}

fn process_block(state: &mut [u64; 3], block: &[u8; 64]) {
    let mut x = [0u64; 8];
    for i in 0..8 {
        x[i] = u64::from_le_bytes(block[i * 8..i * 8 + 8].try_into().unwrap());
    }
    let (mut a, mut b, mut c) = (state[0], state[1], state[2]);

    pass(&mut a, &mut b, &mut c, &x, 5);
    key_schedule(&mut x);
    pass(&mut c, &mut a, &mut b, &x, 7);
    key_schedule(&mut x);
    pass(&mut b, &mut c, &mut a, &x, 9);

    state[0] = a ^ state[0];
    state[1] = b.wrapping_sub(state[1]);
    state[2] = c.wrapping_add(state[2]);
}

impl Default for Tiger {
    fn default() -> Self {
        // 锐捷变体自定义 IV（照抄 rjtiger.c 的 rhash_tiger_init）
        Tiger {
            length: 0,
            buf: [0u8; 64],
            state: [0x158e427ac96b03df, 0xf025c13b8e9da784, 0xb690ab45c3e21b74],
        }
    }
}

impl Tiger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, mut msg: &[u8]) {
        let index = (self.length as usize) & 63;
        self.length = self.length.wrapping_add(msg.len() as u64);

        if index != 0 {
            let left = 64 - index;
            if msg.len() < left {
                self.buf[index..index + msg.len()].copy_from_slice(msg);
                return;
            }
            self.buf[index..64].copy_from_slice(&msg[..left]);
            let block: [u8; 64] = self.buf;
            process_block(&mut self.state, &block);
            msg = &msg[left..];
        }
        while msg.len() >= 64 {
            let block: [u8; 64] = msg[..64].try_into().unwrap();
            process_block(&mut self.state, &block);
            msg = &msg[64..];
        }
        if !msg.is_empty() {
            self.buf[..msg.len()].copy_from_slice(msg);
        }
    }

    pub fn finalize(mut self) -> [u8; 24] {
        let mut index = (self.length as usize) & 63;
        self.buf[index] = 0x01; // tiger2 == false => 用 0x01
        index += 1;
        if index > 56 {
            while index < 64 {
                self.buf[index] = 0;
                index += 1;
            }
            let block: [u8; 64] = self.buf;
            process_block(&mut self.state, &block);
            index = 0;
        }
        while index < 56 {
            self.buf[index] = 0;
            index += 1;
        }
        let bits = self.length.wrapping_shl(3);
        self.buf[56..64].copy_from_slice(&bits.to_le_bytes());
        let block: [u8; 64] = self.buf;
        process_block(&mut self.state, &block);

        let mut out = [0u8; 24];
        for i in 0..3 {
            out[i * 8..i * 8 + 8].copy_from_slice(&self.state[i].to_le_bytes());
        }
        out
    }
}

/// 一次性 Tiger（锐捷变体）哈希，24 字节输出。
pub fn tiger(data: &[u8]) -> [u8; 24] {
    let mut t = Tiger::default();
    t.update(data);
    t.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(l: usize) -> Vec<u8> {
        (0..l).map(|i| ((i * 31 + 7) & 0xff) as u8).collect()
    }

    #[test]
    fn golden_vectors() {
        // 参考：/tmp/ref/golden_hashes.txt (由原版 C 编译生成)
        let cases: &[(usize, &str)] = &[
            (0, "0b7917e593ef3887dd0e20a952ddf3e4fb11e09e053f350a"),
            (1, "36c37b89804d4a1b2d805e3258546c21a24917025fa0d3f7"),
            (3, "1c938e2094c5464b28c8cbb6d126868cd4891b1a70828773"),
            (16, "fd17495d084157312b4faf3f4565a7244fa619e44922329f"),
            (55, "0a74eb8ed353aec66e4480e721943d89e82f6c4b837ffcd9"),
            (64, "1dba932b3ea246914c1e2b7c5fc655cf349164af9ac8b5a6"),
            (65, "110b3ae4fd586c6ef2b4f962ce5753c25c94132463738772"),
            (128, "72c521b6a53cd538f6d2bd4df4e08f222e355c3f790ccc8e"),
        ];
        for &(l, exp) in cases {
            let got = tiger(&pat(l));
            let hs: String = got.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hs, exp, "len={l}");
        }
    }
}
