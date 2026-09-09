//! rjv3 插件使用的专有哈希工具。
//!
//! 移植自原 minieap（及其上游 MentoHUST / rhash / ampheck）中
//! `packet_plugin/rjv3/rjv3_hashes/` 的算法。
//!
//! **重要**：锐捷对 MD5 / SHA-1 / RIPEMD-128 / Tiger / Whirlpool 五种算法的
//! 初始向量乃至轮常量都做了修改（极可能是从真实客户端逆向得到的变体），因此
//! `rj_md5` / `rj_sha1` / `rj_ripemd128` / `tiger` / `whirlpool` 均为逐字节忠实
//! 移植的自定义实现，**不能**替换为任何标准算法库。参考向量由原版 C 代码编译
//! 生成，并写入各文件单元测试中进行比对。
//!
//! 另外这里还提供标准 MD5（`std_md5`，来自 md5std crate）——它对应原工程顶层
//! `md5.c` 的 `ComputeHash`，用于 EAP-MD5 应答与 `compute_pwd`。

pub mod crc16;
pub mod md5;
pub mod md5_data;
pub mod ripemd128;
pub mod rjencode;
pub mod sha1;
pub mod tiger;
pub mod tiger_sbox;
pub mod v4;
pub mod v4_data;
pub mod whirlpool;
pub mod whirlpool_sbox;

use md5std::{Digest, Md5};

pub use md5::RjMd5;
pub use ripemd128::RjRipemd128;
pub use sha1::RjSha1;
pub use tiger::Tiger;
pub use whirlpool::Whirlpool;

/// 标准 MD5（对应原版 `md5.c::ComputeHash`），16 字节输出。
pub fn std_md5(data: &[u8]) -> [u8; 16] {
    let d = Md5::digest(data);
    let mut out = [0u8; 16];
    out.copy_from_slice(&d);
    out
}

/// 便捷入口：修改版 MD5（锐捷变体）。
pub fn rj_md5(data: &[u8]) -> [u8; 16] {
    md5::rj_md5(data)
}
/// 便捷入口：修改版 SHA-1（锐捷变体）。
pub fn rj_sha1(data: &[u8]) -> [u8; 20] {
    sha1::rj_sha1(data)
}
/// 便捷入口：修改版 RIPEMD-128（锐捷变体）。
pub fn rj_ripemd128(data: &[u8]) -> [u8; 16] {
    ripemd128::rj_ripemd128(data)
}
