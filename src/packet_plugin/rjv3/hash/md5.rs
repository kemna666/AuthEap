//! **修改版 MD5**——移植自 `rjmd5.c` (rhash → MentoHUST/minieap)。
//!
//! 与标准 MD5 的差异（需逐字节复刻，不能替换为标准实现）：
//!  1. 自定义 IV：`{0x50137246, 0x8acf9dbe, 0xc9efaced, 0x25647013}`；
//!  2. 每轮 rotate 后分别 `+ b + 2 / - 2 / - 1 / + 1`；
//!  3. 部分轮常量被改写（如 0x02442453、0xfffa3492 等），其中一轮常量与消息相关：
//!     `(uint8_t)(x[2] >> 8) + 0x04881d05`；
//!  4. F 函数使用等价替代式 `((y^z)&x)^z`。

use crate::packet_plugin::rjv3::hash::md5_data::{Ac, K};

#[inline]
fn f(x: u32, y: u32, z: u32) -> u32 {
    // ((y^z)&x)^z，与标准 (x&y)|(~x&z) 等价
    ((y ^ z) & x) ^ z
}
#[inline]
fn g(x: u32, y: u32, z: u32) -> u32 {
    (x & z) | (y & !z)
}
#[inline]
fn h(x: u32, y: u32, z: u32) -> u32 {
    x ^ y ^ z
}
#[inline]
fn i(x: u32, y: u32, z: u32) -> u32 {
    y ^ (x | !z)
}

fn process_block(state: &mut [u32; 4], x: &[u32; 16]) {
    let mut s = *state;
    let mut k = 0usize;

    for (round, defs) in K.iter().enumerate() {
        for def in defs {
            // C 中每步目标寄存器循环变化：a,d,c,b,a,d,c,b,...
            let ti = (4 - (k % 4)) % 4;
            let p1 = s[(ti + 1) % 4];
            let p2 = s[(ti + 2) % 4];
            let p3 = s[(ti + 3) % 4];

            let func = match round {
                0 => f(p1, p2, p3),
                1 => g(p1, p2, p3),
                2 => h(p1, p2, p3),
                _ => i(p1, p2, p3),
            };
            let ac = match def.ac {
                Ac::MessageDep => {
                    // C: (uint8_t)(x[2] >> 8) + 0x4881d05
                    (((x[2] >> 8) & 0xff) as u32).wrapping_add(0x04881d05)
                }
                Ac::Const(v) => v,
            };

            let mut acc = s[ti]
                .wrapping_add(func)
                .wrapping_add(x[def.xi])
                .wrapping_add(ac);
            acc = acc.rotate_left(def.s);
            acc = match round {
                0 => acc.wrapping_add(p1).wrapping_add(2),
                1 => acc.wrapping_add(p1).wrapping_sub(2),
                2 => acc.wrapping_add(p1).wrapping_sub(1),
                _ => acc.wrapping_add(p1).wrapping_add(1),
            };
            s[ti] = acc;
            k += 1;
        }
    }

    for i in 0..4 {
        state[i] = state[i].wrapping_add(s[i]);
    }
}

#[derive(Clone)]
pub struct RjMd5 {
    length: u64,
    buf: [u8; 64],
    state: [u32; 4],
}

impl Default for RjMd5 {
    fn default() -> Self {
        // 锐捷变体自定义 IV（照抄 rhash_md5_init）
        RjMd5 {
            length: 0,
            buf: [0u8; 64],
            state: [0x50137246, 0x8acf9dbe, 0xc9efaced, 0x25647013],
        }
    }
}

impl RjMd5 {
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
            self.process_buf();
            msg = &msg[left..];
        }
        while msg.len() >= 64 {
            let block: [u8; 64] = msg[..64].try_into().unwrap();
            self.process_bytes(&block);
            msg = &msg[64..];
        }
        if !msg.is_empty() {
            self.buf[..msg.len()].copy_from_slice(msg);
        }
    }

    fn process_buf(&mut self) {
        let mut x = [0u32; 16];
        for i in 0..16 {
            x[i] = u32::from_le_bytes(self.buf[i * 4..i * 4 + 4].try_into().unwrap());
        }
        process_block(&mut self.state, &x);
    }

    fn process_bytes(&mut self, block: &[u8; 64]) {
        let mut x = [0u32; 16];
        for i in 0..16 {
            x[i] = u32::from_le_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        process_block(&mut self.state, &x);
    }

    pub fn finalize(mut self) -> [u8; 16] {
        let len = self.length;
        // 逐字操作（等价 C 中以 u32 为单位打 padding）
        let mut index = ((len & 63) >> 2) as usize;
        let shift = ((len & 3) * 8) as u32;

        let mut w = u32::from_le_bytes(self.buf[index * 4..index * 4 + 4].try_into().unwrap());
        w &= !(0xffff_ffffu32 << shift);
        w ^= 0x80u32 << shift;
        self.buf[index * 4..index * 4 + 4].copy_from_slice(&w.to_le_bytes());
        index += 1;

        if index > 14 {
            while index < 16 {
                self.buf[index * 4..index * 4 + 4].copy_from_slice(&0u32.to_le_bytes());
                index += 1;
            }
            self.process_buf();
            index = 0;
        }
        while index < 14 {
            self.buf[index * 4..index * 4 + 4].copy_from_slice(&0u32.to_le_bytes());
            index += 1;
        }
        let lo = len.wrapping_shl(3) as u32;
        let hi = (len >> 29) as u32;
        self.buf[56..60].copy_from_slice(&lo.to_le_bytes());
        self.buf[60..64].copy_from_slice(&hi.to_le_bytes());
        self.process_buf();

        let mut out = [0u8; 16];
        for i in 0..4 {
            out[i * 4..i * 4 + 4].copy_from_slice(&self.state[i].to_le_bytes());
        }
        out
    }
}

/// 一次性修改版 MD5（锐捷变体）哈希，16 字节输出。
pub fn rj_md5(data: &[u8]) -> [u8; 16] {
    let mut m = RjMd5::default();
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
            (0, "2d1a67b2bd1982d14ed10e44dd83b394"),
            (1, "d7ac0ba5eff2ae1883276106f8c58583"),
            (3, "144b5ccac0f6fdad2cda0ceaac8cd7cb"),
            (16, "9795b1d58ddca85bcdf8c189387c5ce8"),
            (55, "d7323b05fb29636b1468a876961997d4"),
            (63, "26af3f8fa82c7c8a1ccb73fb6947e19c"),
            (64, "63de39e387866694f4cc34b983631153"),
            (65, "a3dfe5972a0f7a6781b63e4383b181ec"),
            (128, "cc8a3f7b42a277dd93cdce77e908114d"),
            (200, "47526a6c8458304fbdf2b02903938fc9"),
        ];
        for &(l, exp) in cases {
            let got = rj_md5(&pat(l));
            let hs: String = got.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hs, exp, "len={l}");
        }
    }
}
