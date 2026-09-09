//! Whirlpool 哈希——移植自 `rjwhirlpool.c` (rhash)。
//!
//! **重要**：原 `rjwhirlpool.c` 已被锐捷私有算法修改：
//!  1. 初始状态不是全 0，而是 `{0, 3, 5, 2, 1, 7, 4, 6}`；
//!  2. 每轮对 `state` 的列 2、5、6 在异或 K 后额外 `+ 1`；
//!  3. 压缩函数使用 Miyaguchi–Preneel 风格（与标准实现差异已在 C 注释中标出）。
//! 这里按 C 源码逐句移植，**不能**“修复”为标准的 Whirlpool。

use crate::packet_plugin::rjv3::hash::whirlpool_sbox::SBOXES;

/// 每轮轮常数
const RC: [u64; 10] = [
    0x1823c6e887b8014f,
    0x36a6d2f5796f9152,
    0x60bc9b8ea30c7b35,
    0x1de0d7c22e4bfe57,
    0x157737e59ff04ada,
    0x58c9290ab1a06b85,
    0xbd5d10f4cb3e0567,
    0xe427418ba77d95d8,
    0xfbee7c66dd17479e,
    0xca2dbf07ad5a8333,
];

/// 按 C 的 WHIRLPOOL_OP 宏定义实现。`src` 为 8 个 u64（大端语义的机器字）。
#[inline]
fn op(src: &[u64; 8], shift: usize) -> u64 {
    SBOXES[0][(src[shift & 7] >> 56) as usize]
        ^ SBOXES[1][(src[(shift + 7) & 7] >> 48) as usize & 0xff]
        ^ SBOXES[2][(src[(shift + 6) & 7] >> 40) as usize & 0xff]
        ^ SBOXES[3][(src[(shift + 5) & 7] >> 32) as usize & 0xff]
        ^ SBOXES[4][(src[(shift + 4) & 7] >> 24) as usize & 0xff]
        ^ SBOXES[5][(src[(shift + 3) & 7] >> 16) as usize & 0xff]
        ^ SBOXES[6][(src[(shift + 2) & 7] >> 8) as usize & 0xff]
        ^ SBOXES[7][(src[(shift + 1) & 7]) as usize & 0xff]
}

fn process_block(hash: &mut [u64; 8], block: &[u64; 8]) {
    let mut k = [[0u64; 8]; 2];
    let mut state = [[0u64; 8]; 2];
    let mut m = 0usize;

    for i in 0..8 {
        k[0][i] = hash[i];
        state[0][i] = block[i] ^ hash[i];
        hash[i] = state[0][i];
    }

    for i in 0..10 {
        // 计算 K^i from K^{i-1}
        k[m ^ 1][0] = op(&k[m], 0) ^ RC[i];
        k[m ^ 1][1] = op(&k[m], 1);
        k[m ^ 1][2] = op(&k[m], 2);
        k[m ^ 1][3] = op(&k[m], 3);
        k[m ^ 1][4] = op(&k[m], 4);
        k[m ^ 1][5] = op(&k[m], 5);
        k[m ^ 1][6] = op(&k[m], 6);
        k[m ^ 1][7] = op(&k[m], 7);

        // 应用轮变换（注意 2、5、6 列在异或 K 后 +1，这是锐捷修改点）
        state[m ^ 1][0] = op(&state[m], 0) ^ k[m ^ 1][0];
        state[m ^ 1][1] = op(&state[m], 1) ^ k[m ^ 1][1];
        state[m ^ 1][2] = (op(&state[m], 2) ^ k[m ^ 1][2]).wrapping_add(1);
        state[m ^ 1][3] = op(&state[m], 3) ^ k[m ^ 1][3];
        state[m ^ 1][4] = op(&state[m], 4) ^ k[m ^ 1][4];
        state[m ^ 1][5] = (op(&state[m], 5) ^ k[m ^ 1][5]).wrapping_add(1);
        state[m ^ 1][6] = (op(&state[m], 6) ^ k[m ^ 1][6]).wrapping_add(1);
        state[m ^ 1][7] = op(&state[m], 7) ^ k[m ^ 1][7];

        m ^= 1;
    }

    // Miyaguchi–Preneel 压缩函数
    for i in 0..8 {
        hash[i] ^= state[0][i];
    }
}

#[derive(Clone)]
pub struct Whirlpool {
    length: u64,
    buf: [u8; 64],
    hash: [u64; 8],
}

impl Default for Whirlpool {
    fn default() -> Self {
        // 锐捷变体自定义初始状态（照抄 rjwhirlpool.c 的 rhash_whirlpool_init）
        Whirlpool {
            length: 0,
            buf: [0u8; 64],
            hash: [0, 3, 5, 2, 1, 7, 4, 6],
        }
    }
}

impl Whirlpool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, mut msg: &[u8]) {
        let mut index = (self.length as usize) & 63;
        let left;
        self.length = self.length.wrapping_add(msg.len() as u64);

        if index != 0 {
            left = 64 - index;
            let take = msg.len().min(left);
            self.buf[index..index + take].copy_from_slice(&msg[..take]);
            if msg.len() < left {
                return;
            }
            let block = self.be_words();
            process_block(&mut self.hash, &block);
            msg = &msg[left..];
            index = 0;
        }
        while msg.len() >= 64 {
            let block = be_words(&msg[..64]);
            process_block(&mut self.hash, &block);
            msg = &msg[64..];
        }
        if !msg.is_empty() {
            self.buf[..msg.len()].copy_from_slice(msg);
        }
        let _ = index;
    }

    fn be_words(&self) -> [u64; 8] {
        be_words(&self.buf)
    }

    pub fn finalize(mut self) -> [u8; 64] {
        let mut index = (self.length as usize) & 63;
        self.buf[index] = 0x80;
        index += 1;

        if index > 32 {
            while index < 64 {
                self.buf[index] = 0;
                index += 1;
            }
            let block = self.be_words();
            process_block(&mut self.hash, &block);
            index = 0;
        }
        while index < 56 {
            self.buf[index] = 0;
            index += 1;
        }
        // 只存储 64 位消息长度（大端）
        let bits = self.length.wrapping_shl(3);
        self.buf[56..64].copy_from_slice(&bits.to_be_bytes());
        let block = self.be_words();
        process_block(&mut self.hash, &block);

        let mut out = [0u8; 64];
        for i in 0..8 {
            out[i * 8..i * 8 + 8].copy_from_slice(&self.hash[i].to_be_bytes());
        }
        out
    }
}

/// 将 64 字节按大端解释为 8 个 u64（等价 C 中 `be2me_64` + 指针强转的行为）。
fn be_words(bytes: &[u8]) -> [u64; 8] {
    let mut out = [0u64; 8];
    for i in 0..8 {
        out[i] = u64::from_be_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
    }
    out
}

/// 一次性 Whirlpool（锐捷变体）哈希，64 字节输出。
pub fn whirlpool(data: &[u8]) -> [u8; 64] {
    let mut w = Whirlpool::default();
    w.update(data);
    w.finalize()
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
            (0, "58261111fbe753fb02537c90aaac1a35f4235aae5a2a1e855532ba55b4df6704d2e55ab0bcdc74d56292e2c2d1827b9e88b479aee59242349d234f64d5be300b"),
            (1, "4f229dde52fa9ee4c6c3bf10075f62d1267e481d911fad6cf6c3d08a898e1dc66cbf30d479f41d4d0b1781eca6c47b0454c41301f2237887e8495b7ba460342f"),
            (3, "4cc523b4fa6ea5755aff915d56693bd6b48de1738477e0bae20fb1c90d6c1b9948487d3551247be984aba0d73570794ecd952fb17b740d1e2a28b1f47189ba32"),
            (16, "7e75ef1fea2947b49eb887542d96d7b24d9cecaab1d9fa03577dbb1b6e7136c12ecc7a228f9c7f87f86c4baa4f84f63d2d2c64e12734fca317d732bc31424373"),
            (55, "410bee2d07b2a8d4983b5445093f2fea8e4f1c44876af41ca4a4c6e30b345a42b5777fa567e90a3ff47f9ad8a2097e66487709d9bdaca0e02c2681f9651c139e"),
            (64, "39d09615e773bc2510f76306aae30215f5e2679609b4422fb34976293ff2c0e473d49de396e651dc48bee438b4442b6e9cf39a0cc25d202782beb2d52d6358ca"),
            (65, "593fff5788cb23e5be9a44d2108bdfff336d26afe278a7308d01bf19b1449ace986cfa66598785f3a949862e38b89a540c2f822f0f1976738be1c000a17c0d54"),
            (128, "632be0fa9e38d84a9e0177dd8967d9f2691e4c514f7eace01c72f37a6ca20042704c8362e6ba912209cda5ffb3ab629241a7a8a8bf92350ad133097f12a18db2"),
        ];
        for &(l, exp) in cases {
            let got = whirlpool(&pat(l));
            let hs: String = got.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hs, exp, "len={l}");
        }
    }
}
