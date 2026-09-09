//! 锐捷 v3/v4 认证插件（Rjv3）。
//!
//! 将锐捷私有字段附加到标准 EAP 报文中（`0x1a` 头 + `00 00 13 11` magic 的私有
//! trailer），处理二次认证、Keep-Alive 心跳等。算法移植自 MentoHUST/minieap。

pub mod hash;
pub mod keepalive;
pub mod plugin;
pub mod prop;
