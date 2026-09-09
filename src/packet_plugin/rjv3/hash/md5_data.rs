//! 自动生成的（实际为手工转录）：修改版 MD5 的 64 步轮表。
//! 每步包含：消息字下标 xi、循环左移位数 s、轮常量 ac。
//! 其中 `MessageDep` 表示常量依赖消息（见 md5.rs）。
//! 顺序与 `rjmd5.c` 的 4 个 ROUND 块完全一致。

pub enum Ac {
    Const(u32),
    MessageDep,
}

pub struct Step {
    pub xi: usize,
    pub s: u32,
    pub ac: Ac,
}

pub const K: [[Step; 16]; 4] = [
    // ---- 第 1 轮 (F) ----
    [
        Step { xi: 0, s: 7, ac: Ac::Const(0xd76aa478) },
        Step { xi: 1, s: 12, ac: Ac::Const(0xe8c7b756) },
        Step { xi: 2, s: 17, ac: Ac::Const(0x242070db) },
        Step { xi: 3, s: 22, ac: Ac::Const(0xc1bdceee) },
        Step { xi: 4, s: 7, ac: Ac::Const(0xf57c0faf) },
        Step { xi: 5, s: 12, ac: Ac::Const(0x4787c62a) },
        Step { xi: 6, s: 17, ac: Ac::Const(0xa8304613) },
        Step { xi: 7, s: 22, ac: Ac::Const(0xfd469501) },
        Step { xi: 8, s: 7, ac: Ac::Const(0x698098d8) },
        Step { xi: 9, s: 12, ac: Ac::Const(0x8b44f7af) },
        Step { xi: 10, s: 17, ac: Ac::Const(0xffff5bb1) },
        Step { xi: 11, s: 22, ac: Ac::Const(0x895cd7be) },
        Step { xi: 12, s: 7, ac: Ac::Const(0x6b901122) },
        Step { xi: 13, s: 12, ac: Ac::Const(0xfd987163) },
        Step { xi: 14, s: 17, ac: Ac::Const(0xa679438e) },
        Step { xi: 15, s: 22, ac: Ac::Const(0x49b40821) },
    ],
    // ---- 第 2 轮 (G) ----
    [
        Step { xi: 1, s: 5, ac: Ac::Const(0xf61e2562) },
        Step { xi: 6, s: 9, ac: Ac::Const(0xc040b340) },
        Step { xi: 11, s: 14, ac: Ac::Const(0x265e5a51) },
        Step { xi: 0, s: 20, ac: Ac::Const(0xe9b6c7aa) },
        Step { xi: 5, s: 5, ac: Ac::Const(0xd62f105d) },
        Step { xi: 10, s: 9, ac: Ac::Const(0x02442453) },
        Step { xi: 15, s: 14, ac: Ac::Const(0xd8a1e681) },
        Step { xi: 4, s: 20, ac: Ac::Const(0xe7d3fbc8) },
        Step { xi: 9, s: 5, ac: Ac::Const(0x21e1cde6) },
        Step { xi: 14, s: 9, ac: Ac::Const(0xc33707d6) },
        Step { xi: 3, s: 14, ac: Ac::Const(0xf4d50d87) },
        Step { xi: 8, s: 20, ac: Ac::Const(0x455a14ed) },
        Step { xi: 13, s: 5, ac: Ac::Const(0xa9e3e905) },
        Step { xi: 2, s: 9, ac: Ac::Const(0xfcefa3f8) },
        Step { xi: 7, s: 14, ac: Ac::Const(0x676f02d9) },
        Step { xi: 12, s: 20, ac: Ac::Const(0x8d2a4c8a) },
    ],
    // ---- 第 3 轮 (H) ----
    [
        Step { xi: 5, s: 5, ac: Ac::Const(0xfffa3492) },
        Step { xi: 8, s: 11, ac: Ac::Const(0x8771f681) },
        Step { xi: 11, s: 16, ac: Ac::Const(0x6d9d6122) },
        Step { xi: 14, s: 23, ac: Ac::Const(0xfde5380c) },
        Step { xi: 1, s: 5, ac: Ac::Const(0xa4beea44) },
        Step { xi: 4, s: 11, ac: Ac::Const(0x4bdecfa9) },
        Step { xi: 7, s: 16, ac: Ac::Const(0xf6bb4b60) },
        Step { xi: 10, s: 23, ac: Ac::Const(0xbebfbc70) },
        Step { xi: 13, s: 5, ac: Ac::Const(0x289b7ec6) },
        Step { xi: 0, s: 11, ac: Ac::Const(0xeaa127fa) },
        Step { xi: 3, s: 16, ac: Ac::Const(0xd4ef3085) },
        Step { xi: 6, s: 23, ac: Ac::MessageDep },
        Step { xi: 9, s: 5, ac: Ac::Const(0xd9d4d039) },
        Step { xi: 12, s: 11, ac: Ac::Const(0xe6db99e5) },
        Step { xi: 15, s: 16, ac: Ac::Const(0x1fa27cf8) },
        Step { xi: 2, s: 23, ac: Ac::Const(0xc4ac5665) },
    ],
    // ---- 第 4 轮 (I) ----
    [
        Step { xi: 0, s: 6, ac: Ac::Const(0xf4292244) },
        Step { xi: 7, s: 10, ac: Ac::Const(0x432aff97) },
        Step { xi: 14, s: 15, ac: Ac::Const(0xab9423a7) },
        Step { xi: 5, s: 19, ac: Ac::Const(0xfc93a039) },
        Step { xi: 12, s: 6, ac: Ac::Const(0x655659c3) },
        Step { xi: 3, s: 10, ac: Ac::Const(0x8f0ccc92) },
        Step { xi: 10, s: 15, ac: Ac::Const(0xffeff47d) },
        Step { xi: 1, s: 19, ac: Ac::Const(0x85845dd1) },
        Step { xi: 8, s: 6, ac: Ac::Const(0x6fa87e4f) },
        Step { xi: 15, s: 10, ac: Ac::Const(0xfe2ce6e0) },
        Step { xi: 6, s: 15, ac: Ac::Const(0xa3014314) },
        Step { xi: 13, s: 19, ac: Ac::Const(0x4e0811a1) },
        Step { xi: 4, s: 6, ac: Ac::Const(0xf7537e82) },
        Step { xi: 11, s: 10, ac: Ac::Const(0xbd3af335) },
        Step { xi: 2, s: 15, ac: Ac::Const(0x2ad7d2bb) },
        Step { xi: 9, s: 19, ac: Ac::Const(0xeb866391) },
    ],
];
