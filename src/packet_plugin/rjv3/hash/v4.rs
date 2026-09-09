//! 锐捷 v3/v4 校验算法 —— 移植自 `checkV4.c` (MentoHUST → minieap)。
//!
//! `compute_v4(seed16) -> 128 字符 hex`：根据 MD5 挑战值的前两个字节决定类型
//! （0..=4），用一大一小两张常量表（`v4_data.rs`）配合 MD5/SHA-1/RIPEMD-128/Tiger
//! 拼出中间量，再经 **锐捷修改版 Whirlpool** 得到 64 字节摘要并转小写十六进制。
//!
//! `compute_pwd(username, password, md5) -> 16 字节`：MD5(username||md5) 后再与
//! 密码（不足 16 字节补 0）逐字节异或——即锐捷版“加密后的密码哈希”。

use super::tiger::tiger;
use super::v4_data::{ARRAY, ARRAY_1};
use super::whirlpool::whirlpool;
use super::{rj_md5, rj_ripemd128, rj_sha1, std_md5};

/// 把 `b` 按 C 中 `sprintf("%02x", (signed char)b)` 的语义输出。
/// 即 <0x80 → 2 位小写 hex；>=0x80 → 符号扩展到 32 位后的 8 位 hex。
fn signed_hex(b: u8) -> Vec<u8> {
    if b >= 0x80 {
        let v = (b as i8) as i32; // 负数，如 -1 => 0xffffffff
        format!("{v:x}").into_bytes()
    } else {
        format!("{b:02x}").into_bytes()
    }
}

/// computeV4 的核心（把结果作为 128 个 ascii 十六进制字节返回）。
fn compute_v4_hex(s: &[u8; 16]) -> Option<[u8; 128]> {
    // 与 C 一致：两个 signed char 相加后按 u32 取模 5
    let sum = (s[0] as i8) as i32 + (s[3] as i8) as i32;
    let check_type = (sum as u32) % 5;

    // 中间量缓冲（足够容纳 case 0 的越界写法，最后截断到 0x60）
    let mut wtmp: Vec<u8> = Vec::with_capacity(200);

    match check_type {
        0 => {
            // md5(ARRAY) → hex  (注意：这里用的是锐捷修改版 rj_md5)
            let h = rj_md5(&ARRAY);
            for x in h {
                let t = format!("{x:02x}");
                wtmp.extend_from_slice(t.as_bytes());
            }
            // s 的偶数下标，按 signed char 输出
            for i in 0..8 {
                wtmp.extend_from_slice(&signed_hex(s[2 * i]));
            }
            // md5(ARRAY_1) → hex
            let h2 = rj_md5(&ARRAY_1);
            for x in h2 {
                let t = format!("{x:02x}");
                wtmp.extend_from_slice(t.as_bytes());
            }
            // s 的奇数下标
            for i in 0..8 {
                wtmp.extend_from_slice(&signed_hex(s[2 * i + 1]));
            }
            wtmp.truncate(0x60);
        }
        1 => {
            wtmp.extend_from_slice(&rj_sha1(&ARRAY_1));
            wtmp.extend_from_slice(&s[0..6]);
            wtmp.extend_from_slice(&rj_sha1(&ARRAY));
            wtmp.extend_from_slice(&s[6..16]);
        }
        2 => {
            wtmp.extend_from_slice(&rj_sha1(&ARRAY_1));
            wtmp.extend_from_slice(&s[0..6]);
            wtmp.extend_from_slice(&rj_ripemd128(&ARRAY));
            wtmp.extend_from_slice(&s[6..16]);
        }
        3 => {
            wtmp.extend_from_slice(&tiger(&ARRAY));
            wtmp.extend_from_slice(&s[0..10]);
            wtmp.extend_from_slice(&rj_ripemd128(&ARRAY_1));
            wtmp.extend_from_slice(&s[10..16]);
        }
        4 => {
            wtmp.extend_from_slice(&tiger(&ARRAY_1));
            wtmp.extend_from_slice(&s[0..8]);
            wtmp.extend_from_slice(&rj_sha1(&ARRAY));
            wtmp.extend_from_slice(&s[8..16]);
        }
        _ => return None,
    }

    let digest = whirlpool(&wtmp);
    let mut out = [0u8; 128];
    let hexstr = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    out.copy_from_slice(hexstr.as_bytes());
    Some(out)
}

/// computeV4：16 字节种子 → 128 字节小写 hex 字符串字节。
pub fn compute_v4(seed: &[u8; 16]) -> Option<[u8; 128]> {
    compute_v4_hex(seed)
}

/// computePwd：锐捷“密码摘要”。注意这里用的是标准 MD5 (ComputeHash)。
pub fn compute_pwd(username: &str, password: &str, md5seed: &[u8; 16]) -> [u8; 16] {
    let mut tmp: Vec<u8> = Vec::with_capacity(40);
    tmp.extend_from_slice(username.as_bytes());
    tmp.extend_from_slice(md5seed);
    let mut out = std_md5(&tmp);
    let pw = password.as_bytes();
    for i in 0..16 {
        if i < pw.len() {
            out[i] ^= pw[i];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hexout(a: &[u8; 128]) -> String {
        a.iter().map(|b| *b as char).collect()
    }

    #[test]
    fn v4_golden_spot_check() {
        // 与 /tmp/ref/golden_v4.txt 抽样比对（前几条 + 类型 0/1/2/3/4 各覆盖一条）
        let cases: &[(u8, &str)] = &[
            (0x00, "c43c16515651db747f5267c09bde73da469b06e81ca5afc35aeaf920c3ba9c98af9a3d106344d9b83b1eb6ba5606efdff0acc44f4aae19caf2a8172baf68d2b2"),
            (0x01, "61b57599f2a7694907d1c5a9b27191c8f6c339135a0ba84f2bf2fc3208f7792668a15c32fa9ed252678a225cdc5a2e5478c8ee3ab345df4ba611c2806bd20edd"),
            (0x02, "67ed2a81a753a5f2826bc413517bff274ead621ecc2719d954c2c9e0b28ca9332c1b64925dfd8537049e359a22151df24de5e3bc198be68c3123151c6a07b7f7"),
            (0x03, "1092bb0148bf42d9a935e57b3a742822ce345ef1e5e547a0b37ba740dab919d01b6a27775880026167f9d52f22d0349ec4cdbde61b032f8280d658949fbf9024"),
            (0x04, "109b2e7a40e92ed8bdcf15550388d77d7dcaf8baff6db278a0cf6156478871a2a5780522cf7c63b661c9bd68ba1f400ebe31725d77a9d66685f25cfb732521a8"),
            (0x05, "7549637b37f67291e945962e1c4d92b8e5be29cf950c3c3501a110ffe4f5388ef7295e7e284a94580593ae1449b36327150c6c34642aee4e57476f1b65e1d608"),
            // type=0 且 s 中存在高位字节（触发 signed 符号扩展路径）的向量
            (0x0c, "3db45875a869bbf5c59689f4cb0f9b133a9f02dcc01627c8c340f5224be4ea587fd70673c76d10e534c4934a6bd277361c1319f75a1e3804e89008d645a86bf7"),
        ];
        for &(first, expected) in cases {
            let mut seed = [0u8; 16];
            for (j, x) in seed.iter_mut().enumerate() {
                *x = ((first as usize * 7 + j * 3) & 0xff) as u8;
            }
            seed[0] = first;
            let r = compute_v4(&seed).expect("compute_v4");
            assert_eq!(hexout(&r), expected, "first=0x{first:02x}");
        }
    }

    #[test]
    fn pwd_golden_spot_check() {
        let cases: &[(u8, &str)] = &[
            (0x00, "5317cb2011bc8821fe004934c12529a5"),
            (0x01, "ff93f22351f17d716e142c21353ccdb6"),
            (0x02, "2fcd69cf0fdc533956e605cd8a857d78"),
        ];
        for &(first, expected) in cases {
            let mut md5seed = [0u8; 16];
            for (j, x) in md5seed.iter_mut().enumerate() {
                *x = (first as usize + j * 17) as u8;
            }
            let r = compute_pwd("201000000", "15000000000", &md5seed);
            let hs: String = r.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hs, expected, "pwd first=0x{first:02x}");
        }
    }
}
