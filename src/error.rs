//! 统一错误类型。

use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// 配置 / 参数错误
    Config(String),
    /// 网络 / 套接字错误
    Net(String),
    /// 协议处理错误
    Proto(String),
    /// 操作系统调用错误
    Os(std::io::Error),
    /// 其它
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(s) => write!(f, "配置错误: {s}"),
            Error::Net(s) => write!(f, "网络错误: {s}"),
            Error::Proto(s) => write!(f, "协议错误: {s}"),
            Error::Os(e) => write!(f, "系统错误: {e}"),
            Error::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Os(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
