//! **修改版 SHA-1**——移植自 `rjsha1.c` (rhash → MentoHUST/minieap)。
//!
//! 与标准 SHA-1 的差异（需逐字节复刻）：
//!  1. 自定义 IV：`{0x32075416, 0xf8dae9bc, 0x73541260, 0x8acb9dfe, 0xfd0c2e1b}`；
//!  2. 轮常量改写为减法形式（-0x5d6aa4d4 / +0x16ae9deb / -0x34032e48 / -0x5cd39e93）；
//!  3. t20..39 额外加当前块首字节 `(uint8_t)le2me_32(block[0])`；
//!  4. 摘要回加时 `hash[0] += A + 1`。

#[inline]
fn f1(b: u32, c: u32, d: u32) -> u32 {
    // ((C^D)&B)^D 等价于 (B&C)|(~B&D)
    ((c ^ d) & b) ^ d
}

fn process_block(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for t in 0..16 {
        w[t] = u32::from_be_bytes(block[t * 4..t * 4 + 4].try_into().unwrap());
    }
    for t in 16..80 {
        w[t] = (w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16]).rotate_left(1);
    }

    let (mut a, mut b, mut c, mut d, mut e) = (state[0], state[1], state[2], state[3], state[4]);

    // t20..39 的消息相关项：当前块首字节
    let block0 = block[0] as u32;

    for t in 0..20 {
        let temp = a
            .rotate_left(5)
            .wrapping_add(f1(b, c, d))
            .wrapping_add(e)
            .wrapping_add(w[t])
            .wrapping_sub(0x5d6aa4d4);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    for t in 20..40 {
        let temp = a
            .rotate_left(5)
            .wrapping_add(b ^ c ^ d)
            .wrapping_add(e)
            .wrapping_add(w[t])
            .wrapping_add(0x16ae9deb)
            .wrapping_add(block0);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    for t in 40..61 {
        let temp = a
            .rotate_left(5)
            .wrapping_add((b & c) | (b & d) | (c & d))
            .wrapping_add(e)
            .wrapping_add(w[t])
            .wrapping_sub(0x34032e48);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    for t in 61..80 {
        let temp = a
            .rotate_left(5)
            .wrapping_add(b ^ c ^ d)
            .wrapping_add(e)
            .wrapping_add(w[t])
            .wrapping_sub(0x5cd39e93);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }

    state[0] = state[0].wrapping_add(a).wrapping_add(1);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

#[derive(Clone)]
pub struct RjSha1 {
    length: u64,
    buf: [u8; 64],
    state: [u32; 5],
}

impl Default for RjSha1 {
    fn default() -> Self {
        // 锐捷变体自定义 IV（照抄 rhash_sha1_init）
        RjSha1 {
            length: 0,
            buf: [0u8; 64],
            state: [0x32075416, 0xf8dae9bc, 0x73541260, 0x8acb9dfe, 0xfd0c2e1b],
        }
    }
}

impl RjSha1 {
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
            process_block(&mut self.state, &block);
            msg = &msg[64..];
        }
        if !msg.is_empty() {
            self.buf[..msg.len()].copy_from_slice(msg);
        }
    }

    fn process_buf(&mut self) {
        process_block(&mut self.state, &self.buf);
    }

    pub fn finalize(mut self) -> [u8; 20] {
        let len = self.length;
        let mut index = (len as usize) & 63;
        self.buf[index] = 0x80;
        index += 1;
        while (index & 3) != 0 {
            self.buf[index] = 0;
            index += 1;
        }
        let mut windex = index >> 2;
        if windex > 14 {
            while windex < 16 {
                self.buf[windex * 4..windex * 4 + 4].copy_from_slice(&0u32.to_be_bytes());
                windex += 1;
            }
            self.process_buf();
            windex = 0;
        }
        while windex < 14 {
            self.buf[windex * 4..windex * 4 + 4].copy_from_slice(&0u32.to_be_bytes());
            windex += 1;
        }
        let hi = (len >> 29) as u32;
        let lo = len.wrapping_shl(3) as u32;
        self.buf[56..60].copy_from_slice(&hi.to_be_bytes());
        self.buf[60..64].copy_from_slice(&lo.to_be_bytes());
        self.process_buf();

        let mut out = [0u8; 20];
        for i in 0..5 {
            out[i * 4..i * 4 + 4].copy_from_slice(&self.state[i].to_be_bytes());
        }
        out
    }
}

/// 一次性修改版 SHA-1（锐捷变体）哈希，20 字节输出。
pub fn rj_sha1(data: &[u8]) -> [u8; 20] {
    let mut m = RjSha1::default();
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
            (0, "b772187b3b0675807e77088e9c43fc02309c45a8"),
            (1, "e1591cfc705f4559d8716cef1a5fdbca7dc50e36"),
            (3, "22ad0a7db3eaab6a4efcecb3d7bf9b09981001ce"),
            (16, "5bb4720f608c0e44ee4adb79ade146592e4d16b2"),
            (55, "5bbffc9c53a18ad63f7db06b9999241d1f8a16a4"),
            (63, "0a6f771b413e19190167be6401069082d6f088d1"),
            (64, "823dab24f066d5679e5380ad900a2b88157c6f8a"),
            (65, "e74a244d8d26bfe0811a0c10e7f226e85b552944"),
            (128, "4ab35b52c60fc48793cbeacc343bc108d4100511"),
            (200, "2b1067b8b26477ca9ff873b49ec2fdd217dfa3ef"),
        ];
        for &(l, exp) in cases {
            let got = rj_sha1(&pat(l));
            let hs: String = got.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hs, exp, "len={l}");
        }
    }
}
