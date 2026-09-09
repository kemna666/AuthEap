//! **修改版 RIPEMD-128**——移植自 `rjripemd128.c` (ampheck → MentoHUST/minieap)。
//!
//! 这是逆向锐捷客户端得到的、被深度改写的变体：
//!  1. 自定义 IV：`{0x10257436, 0xa8bd9cfe, 0x9efcad8b, 0x12375460}`；
//!  2. 布尔轮函数宏内嵌了非标准常量（如 `(x^y^z)+2`、`+0x325b99a1` 等）；
//!  3. 大量原始轮被改写为带 `0x43234e0x` 系列魔改常量的显式语句；
//!  4. 组合阶段的 `h[2] = h[3]+...+1` 亦被改动。
//! 必须照抄，不能替换为标准 RIPEMD-128。

// 四个布尔函数
#[inline]
fn f1(x: u32, y: u32, z: u32) -> u32 {
    x ^ y ^ z
}
#[inline]
fn f2(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | (!x & z)
}
#[inline]
fn f3(x: u32, y: u32, z: u32) -> u32 {
    (x | !y) ^ z
}
#[inline]
fn f4(x: u32, y: u32, z: u32) -> u32 {
    (x & z) | (y & !z)
}

/// 执行一步：wv[a] = rol(wv[a] + f(wv[b],wv[c],wv[d]) + word + k, rot)
fn st(wv: &mut [u32; 8], a: usize, b: usize, c: usize, d: usize, word: u32, rot: u32, f: fn(u32, u32, u32) -> u32, k: u32) {
    wv[a] = wv[a]
        .wrapping_add(f(wv[b], wv[c], wv[d]))
        .wrapping_add(word)
        .wrapping_add(k)
        .rotate_left(rot);
}

/// 根据轮号给出 (布尔函数, 宏常量)。注意左右两侧同名布尔函数所带的
/// 常量并不同（如 R2 与 R7、R3 与 R6），需逐轮区分。
fn rnd(r: u32) -> (fn(u32, u32, u32) -> u32, u32) {
    match r {
        1 => (f1 as fn(u32, u32, u32) -> u32, 2),
        2 => (f2 as fn(u32, u32, u32) -> u32, 0x325b99a1),
        3 => (f3 as fn(u32, u32, u32) -> u32, 0x1baed69c),
        4 => (f4 as fn(u32, u32, u32) -> u32, 0xbcdcb1f9),
        5 => (f4 as fn(u32, u32, u32) -> u32, 0x5a82798a),
        6 => (f3 as fn(u32, u32, u32) -> u32, 0x41d42d5d),
        7 => (f2 as fn(u32, u32, u32) -> u32, 0x30ed3f68),
        8 => (f1 as fn(u32, u32, u32) -> u32, 2),
        _ => unreachable!("rnd"),
    }
}

/// 处理单个 64 字节块。`h` 为上下文状态（会被更新）。
fn transform(h: &mut [u32; 4], block: &[u8; 64]) {
    let mut w = [0u32; 16];
    for i in 0..16 {
        w[i] = u32::from_le_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
    }
    let mut wv = [0u32; 8];
    wv[0] = h[0];
    wv[1] = h[1];
    wv[2] = h[2];
    wv[3] = h[3];
    wv[4] = h[0];
    wv[5] = h[1];
    wv[6] = h[2];
    wv[7] = h[3];

    macro_rules! s {
        ($a:expr, $b:expr, $c:expr, $d:expr, $w:expr, $rot:expr, $r:expr) => {{
            let (f, k) = rnd($r);
            st(&mut wv, $a, $b, $c, $d, $w, $rot, f, k);
        }};
    }

    // ================= 第 1 轮（左线） =================
    s!(0, 1, 2, 3, w[0], 11, 1);
    s!(3, 0, 1, 2, w[1], 14, 1);
    s!(2, 3, 0, 1, w[2], 15, 1);
    s!(1, 2, 3, 0, w[3], 12, 1);
    s!(0, 1, 2, 3, w[4], 5, 1);
    s!(3, 0, 1, 2, w[5], 8, 1);
    s!(2, 3, 0, 1, w[6], 7, 1);
    s!(1, 2, 3, 0, w[7], 9, 1);
    s!(0, 1, 2, 3, w[8], 11, 1);
    s!(3, 0, 1, 2, w[9], 13, 1);
    s!(2, 3, 0, 1, w[10], 14, 1);
    s!(1, 2, 3, 0, w[11], 15, 1);
    s!(0, 1, 2, 3, w[12], 6, 1);
    s!(3, 0, 1, 2, w[13], 7, 1);
    s!(2, 3, 0, 1, w[14], 9, 1);
    s!(1, 2, 3, 0, w[15], 8, 1);

    // ================= 第 2 轮（左线，含改写） =================
    s!(0, 1, 2, 3, w[7], 7, 2);
    st(&mut wv, 3, 0, 1, 2, w[4], 6, f2, 2); // +w[4]+2
    s!(2, 3, 0, 1, w[13], 8, 2);
    s!(1, 2, 3, 0, w[1], 13, 2);
    s!(0, 1, 2, 3, w[10], 11, 2);
    s!(3, 0, 1, 2, w[6], 9, 2);
    s!(2, 3, 0, 1, w[15], 7, 2);
    st(&mut wv, 1, 2, 3, 0, w[3], 15, f2, 0x43234e07u32.wrapping_neg()); // -0x43234e07
    s!(0, 1, 2, 3, w[12], 7, 2);
    st(&mut wv, 3, 0, 1, 2, w[0], 12, f2, 2); // +w[0]+2
    s!(2, 3, 0, 1, w[9], 15, 2);
    s!(1, 2, 3, 0, w[5], 9, 2);
    s!(0, 1, 2, 3, w[2], 11, 2);
    s!(3, 0, 1, 2, w[14], 7, 2);
    s!(2, 3, 0, 1, w[11], 13, 2);
    st(&mut wv, 1, 2, 3, 0, w[8], 12, f2, 2); // +w[8]+2

    // ================= 第 3 轮（左线，含改写） =================
    s!(0, 1, 2, 3, w[3], 11, 3);
    s!(3, 0, 1, 2, w[10], 13, 3);
    s!(2, 3, 0, 1, w[14], 6, 3);
    s!(1, 2, 3, 0, w[4], 7, 3);
    s!(0, 1, 2, 3, w[9], 14, 3);
    s!(3, 0, 1, 2, w[15], 9, 3);
    s!(2, 3, 0, 1, w[8], 13, 3);
    s!(1, 2, 3, 0, w[1], 15, 3);
    s!(0, 1, 2, 3, w[2], 14, 3);
    s!(3, 0, 1, 2, w[7], 8, 3);
    s!(2, 3, 0, 1, w[0], 13, 3);
    st(&mut wv, 1, 2, 3, 0, w[6], 6, f3, 0x325b99a1); // +w[6]+0x325b99a1
    s!(0, 1, 2, 3, w[13], 5, 3);
    st(&mut wv, 3, 0, 1, 2, w[11], 12, f3, 0x43234e06u32.wrapping_neg()); // -0x43234e06
    s!(2, 3, 0, 1, w[5], 7, 3);
    s!(1, 2, 3, 0, w[12], 5, 3);

    // ================= 第 4 轮（左线，含改写） =================
    s!(0, 1, 2, 3, w[1], 11, 4);
    s!(3, 0, 1, 2, w[9], 12, 4);
    st(&mut wv, 2, 3, 0, 1, w[11], 14, f4, 0x325b99a1); // +w[11]+0x325b99a1
    s!(1, 2, 3, 0, w[10], 15, 4);
    s!(0, 1, 2, 3, w[0], 14, 4);
    s!(3, 0, 1, 2, w[8].wrapping_add(2), 15, 4); // w[8]+2
    s!(2, 3, 0, 1, w[12], 9, 4);
    s!(1, 2, 3, 0, w[4], 8, 4);
    s!(0, 1, 2, 3, w[13], 9, 4);
    st(&mut wv, 3, 0, 1, 2, w[3], 14, f4, 0x1baed69c); // +w[3]+0x1baed69c
    s!(2, 3, 0, 1, w[7], 5, 4);
    st(&mut wv, 1, 2, 3, 0, w[15], 6, f4, 2); // +w[15]+2
    s!(0, 1, 2, 3, w[14], 8, 4);
    s!(3, 0, 1, 2, w[5], 6, 4);
    s!(2, 3, 0, 1, w[6], 5, 4);
    st(&mut wv, 1, 2, 3, 0, w[2], 12, f4, 0x43234e04u32.wrapping_neg()); // -0x43234e04

    // ================= 第 5 轮（右线，含改写） =================
    s!(4, 5, 6, 7, w[5], 8, 5);
    s!(7, 4, 5, 6, w[14], 9, 5);
    s!(6, 7, 4, 5, w[7], 9, 5);
    s!(5, 6, 7, 4, w[0], 11, 5);
    s!(4, 5, 6, 7, w[9], 13, 5);
    s!(7, 4, 5, 6, w[2], 15, 5);
    st(&mut wv, 6, 7, 4, 5, w[11], 15, f4, 0x325b99a1); // +w[11]+0x325b99a1
    s!(5, 6, 7, 4, w[4], 5, 5);
    s!(4, 5, 6, 7, w[13], 7, 5);
    s!(7, 4, 5, 6, w[6], 7, 5);
    st(&mut wv, 6, 7, 4, 5, w[15], 8, f4, 0x43234e07u32.wrapping_neg()); // -0x43234e07
    s!(5, 6, 7, 4, w[8], 11, 5);
    s!(4, 5, 6, 7, w[1], 14, 5);
    s!(7, 4, 5, 6, w[10], 14, 5);
    st(&mut wv, 6, 7, 4, 5, w[3], 12, f4, 0x43234e04u32.wrapping_neg()); // -0x43234e04
    s!(5, 6, 7, 4, w[12], 6, 5);

    // ================= 第 6 轮（右线，含改写） =================
    s!(4, 5, 6, 7, w[6], 9, 6);
    s!(7, 4, 5, 6, w[11], 13, 6);
    s!(6, 7, 4, 5, w[3], 15, 6);
    s!(5, 6, 7, 4, w[7], 7, 6);
    s!(4, 5, 6, 7, w[0], 12, 6);
    s!(7, 4, 5, 6, w[13], 8, 6);
    st(&mut wv, 6, 7, 4, 5, w[5], 9, f3, 0x43234e07u32.wrapping_neg()); // -0x43234e07
    st(&mut wv, 5, 6, 7, 4, w[10], 11, f3, 0x43234e03u32.wrapping_neg()); // -0x43234e03
    s!(4, 5, 6, 7, w[14], 7, 6);
    s!(7, 4, 5, 6, w[15], 7, 6);
    s!(6, 7, 4, 5, w[8], 12, 6);
    s!(5, 6, 7, 4, w[12], 7, 6);
    s!(4, 5, 6, 7, w[4], 6, 6);
    s!(7, 4, 5, 6, w[9], 15, 6);
    st(&mut wv, 6, 7, 4, 5, w[1], 13, f3, 0x325b99a1); // +w[1]+0x325b99a1
    s!(5, 6, 7, 4, w[2], 11, 6);

    // ================= 第 7 轮（右线，含改写） =================
    s!(4, 5, 6, 7, w[15], 9, 7);
    s!(7, 4, 5, 6, w[5], 7, 7);
    s!(6, 7, 4, 5, w[1], 15, 7);
    s!(5, 6, 7, 4, w[3], 11, 7);
    s!(4, 5, 6, 7, w[7], 8, 7);
    s!(7, 4, 5, 6, w[14], 6, 7);
    st(&mut wv, 6, 7, 4, 5, w[6], 6, f2, 0x43234e02u32.wrapping_neg()); // -0x43234e02
    s!(5, 6, 7, 4, w[9], 14, 7);
    s!(4, 5, 6, 7, w[11], 12, 7);
    st(&mut wv, 7, 4, 5, 6, w[8], 13, f2, 0x5a82798a); // +w[8]+0x5a82798a
    s!(6, 7, 4, 5, w[12], 5, 7);
    s!(5, 6, 7, 4, w[2], 14, 7);
    s!(4, 5, 6, 7, w[10], 13, 7);
    s!(7, 4, 5, 6, w[0], 13, 7);
    st(&mut wv, 6, 7, 4, 5, w[4], 7, f2, 0x41d42d5d); // +w[4]+0x41d42d5d
    s!(5, 6, 7, 4, w[13], 5, 7);

    // ================= 第 8 轮（右线，含改写） =================
    s!(4, 5, 6, 7, w[8], 15, 8);
    st(&mut wv, 7, 4, 5, 6, w[6], 5, f1, 0x43234e02u32.wrapping_neg()); // -0x43234e02
    s!(6, 7, 4, 5, w[4], 8, 8);
    s!(5, 6, 7, 4, w[1], 11, 8);
    s!(4, 5, 6, 7, w[3], 14, 8);
    s!(7, 4, 5, 6, w[11], 14, 8);
    s!(6, 7, 4, 5, w[15], 6, 8);
    s!(5, 6, 7, 4, w[0], 14, 8);
    s!(4, 5, 6, 7, w[5], 6, 8);
    s!(7, 4, 5, 6, w[12], 9, 8);
    s!(6, 7, 4, 5, w[2], 12, 8);
    s!(5, 6, 7, 4, w[13], 9, 8);
    s!(4, 5, 6, 7, w[9], 12, 8);
    s!(7, 4, 5, 6, w[7], 5, 8);
    s!(6, 7, 4, 5, w[10], 15, 8);
    s!(5, 6, 7, 4, w[14], 8, 8);

    // ================= 组合（Miyaguchi 变体） =================
    let (h0, h1, h2, h3) = (h[0], h[1], h[2], h[3]);
    let wv7 = wv[7]
        .wrapping_add(wv[2])
        .wrapping_add(h1);
    h[1] = h2.wrapping_add(wv[3]).wrapping_add(wv[4]);
    h[2] = h3
        .wrapping_add(wv[0])
        .wrapping_add(wv[5])
        .wrapping_add(1);
    h[3] = h0.wrapping_add(wv[1]).wrapping_add(wv[6]);
    h[0] = wv7;
}

#[derive(Clone)]
pub struct RjRipemd128 {
    length: u64,
    buf: [u8; 64],
    h: [u32; 4],
}

impl Default for RjRipemd128 {
    fn default() -> Self {
        // 锐捷变体自定义 IV（照抄 ampheck_ripemd128_init）
        RjRipemd128 {
            length: 0,
            buf: [0u8; 64],
            h: [0x10257436, 0xa8bd9cfe, 0x9efcad8b, 0x12375460],
        }
    }
}

impl RjRipemd128 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut size = data.len();
        let mut dpos = 0usize;
        let left = (self.length % 64) as usize;

        if size >= 64 - left {
            let fill = 64 - left;
            self.buf[left..64].copy_from_slice(&data[..fill]);
            dpos += fill;
            size -= fill;

            let block: [u8; 64] = self.buf;
            transform(&mut self.h, &block);

            // 处理后续的整块
            let blocks = size / 64;
            for _ in 0..blocks {
                let b: [u8; 64] = data[dpos..dpos + 64].try_into().unwrap();
                transform(&mut self.h, &b);
                dpos += 64;
            }
            size %= 64;
            self.buf[..size].copy_from_slice(&data[dpos..dpos + size]);
        } else {
            self.buf[left..left + size].copy_from_slice(data);
        }
        self.length = self.length.wrapping_add(data.len() as u64);
    }

    pub fn finalize(mut self) -> [u8; 16] {
        let tlen = (self.length % 64) as usize;

        self.buf[tlen] = 0x80;
        if tlen < 56 {
            self.buf[tlen + 1..56].fill(0);
        } else {
            self.buf[tlen + 1..64].fill(0);
            let block: [u8; 64] = self.buf;
            transform(&mut self.h, &block);
            self.buf[..56].fill(0);
        }
        self.buf[56..64].copy_from_slice(&(self.length.wrapping_mul(8)).to_le_bytes());
        let block: [u8; 64] = self.buf;
        transform(&mut self.h, &block);

        let mut out = [0u8; 16];
        for i in 0..4 {
            out[i * 4..i * 4 + 4].copy_from_slice(&self.h[i].to_le_bytes());
        }
        out
    }
}

/// 一次性修改版 RIPEMD-128（锐捷变体）哈希，16 字节输出。
pub fn rj_ripemd128(data: &[u8]) -> [u8; 16] {
    let mut m = RjRipemd128::default();
    m.update(data);
    m.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(l: usize) -> Vec<u8> {
        (0..l).map(|i| ((i * 31 + 7) & 0xff) as u8).collect()
    }

    #[test]
    fn golden_vectors() {
        // 参考：/tmp/ref/golden_hashes.txt（由原版 C 编译生成）
        let cases: &[(usize, &str)] = &[
            (0, "e2cf51ff5f7d08e090a206613d633194"),
            (1, "8af564b7c9f48b0788d0a996afa6c5c6"),
            (3, "cbe669c8a6abd67986b6552b17fa1ade"),
            (16, "0ae3c1c78c2de9642fd4c888fad47e93"),
            (55, "dd20364985880b39f129140d14c17f29"),
            (63, "4fe26ee1a7911ea69f84c456d1647744"),
            (64, "1b43a5515f591f73d0ec86b866e7a819"),
            (65, "4d5663d6a1d9368029751a4136abeb10"),
            (128, "59fa78692e6e815d6f670ed3eeeaa5bf"),
            (200, "d295ef4b7b13612ed929ff7f67f77382"),
        ];
        for &(l, exp) in cases {
            let got = rj_ripemd128(&pat(l));
            let hs: String = got.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hs, exp, "len={l}");
        }
    }
}
