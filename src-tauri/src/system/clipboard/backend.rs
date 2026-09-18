// src-tauri/src/system/clipboard/backend.rs
// 剪贴板读取后端抽象。
//
// 抽出 trait 的目的只有可测性：worker 的轮询、防抖、app 写入吸收全部依赖此抽象，
// 于是不需要 Windows、不需要真实剪贴板就能驱动完整的故障路径
// （初始化失败 → worker 退出 → supervisor 重启 → 恢复）。

use super::clipboard_seq;

/// 一次读取的结局。
///
/// `Empty` 必须与「读取失败」分开：用户复制一张图片时 arboard 返回
/// `ContentNotAvailable`，这是**正常状态**而非故障。若把它计入失败，
/// 用户复制一张图就会让监控退避到上限，此后复制文字要等几十秒才翻译 ——
/// 把正常操作误判成故障的直接后果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardRead {
    Text(String),
    /// 剪贴板为空，或内容是图片等非文本格式。不计入失败计数。
    Empty,
}

/// 失败严重程度，决定 worker 是就地重试还是退出交给 supervisor 重建句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardErrorKind {
    /// 暂时性：剪贴板被其他进程占用、瞬时读取失败。
    /// worker 就地退避重试，不重建句柄、不退出。
    Transient,
    /// 致命：句柄本身不可用。worker 退出，supervisor 退避后重建句柄再试。
    Fatal,
}

#[derive(Debug, Clone)]
pub struct ClipboardBackendError {
    pub kind: ClipboardErrorKind,
    /// 稳定的错误码，用于健康快照与结构化日志。
    /// 不得包含用户复制的内容（计划第 32 节）。
    pub code: &'static str,
    pub message: String,
}

impl ClipboardBackendError {
    pub fn transient(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ClipboardErrorKind::Transient,
            code,
            message: message.into(),
        }
    }

    pub fn fatal(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ClipboardErrorKind::Fatal,
            code,
            message: message.into(),
        }
    }
}

/// 剪贴板读取后端。生产实现是 arboard + Win32，测试实现是脚本化 mock。
pub trait ClipboardBackend: Send {
    fn get_text(&mut self) -> Result<ClipboardRead, ClipboardBackendError>;

    /// 平台变更序列号。`None` = 平台不提供，调用方走降级策略。
    fn seq(&self) -> Option<u32>;
}

/// 生产实现：arboard 读文本 + Win32 读序列号。
pub struct ArboardBackend {
    inner: arboard::Clipboard,
}

impl ArboardBackend {
    /// 句柄建不起来时是 Fatal：对同一个失败的实例重试没有意义，
    /// 必须由 supervisor 重新走一次 `new()`。这正是 Phase 2 要修的故障点 ——
    /// 此前这里的失败会让监控线程直接 return，且没有任何人观察到它退出。
    pub fn new() -> Result<Self, ClipboardBackendError> {
        arboard::Clipboard::new()
            .map(|inner| Self { inner })
            .map_err(|e| ClipboardBackendError::fatal("CLIPBOARD_INIT_FAILED", e.to_string()))
    }
}

impl ClipboardBackend for ArboardBackend {
    fn get_text(&mut self) -> Result<ClipboardRead, ClipboardBackendError> {
        match self.inner.get_text() {
            Ok(text) => Ok(ClipboardRead::Text(text)),
            // 空 / 非文本内容 —— 正常状态，不是故障
            Err(arboard::Error::ContentNotAvailable) => Ok(ClipboardRead::Empty),
            // 被其他进程占用：最典型的暂时性失败
            Err(arboard::Error::ClipboardOccupied) => Err(ClipboardBackendError::transient(
                "CLIPBOARD_OCCUPIED",
                "剪贴板被其他进程占用",
            )),
            // 当前配置下剪贴板不可用：对同一句柄重试无意义，交出控制权重建
            Err(arboard::Error::ClipboardNotSupported) => Err(ClipboardBackendError::fatal(
                "CLIPBOARD_NOT_SUPPORTED",
                "当前系统配置不支持剪贴板访问",
            )),
            // 转换失败 / 未知：保守归为暂时性。未知错误不该让监控永久降级，
            // 持续失败由退避上限兜住。
            Err(e) => Err(ClipboardBackendError::transient(
                "CLIPBOARD_READ_FAILED",
                e.to_string(),
            )),
        }
    }

    fn seq(&self) -> Option<u32> {
        clipboard_seq()
    }
}

#[cfg(test)]
pub(crate) mod mock {
    //! 脚本化的剪贴板后端，供 worker / supervisor 的单元测试驱动故障路径。

    use std::collections::VecDeque;

    use super::{ClipboardBackend, ClipboardBackendError, ClipboardRead};

    /// 脚本里的一个步骤。
    #[derive(Debug, Clone)]
    pub enum Step {
        Text(&'static str),
        Empty,
        Transient(&'static str),
        Fatal(&'static str),
    }

    pub struct MockClipboardBackend {
        script: VecDeque<Step>,
        seq: Option<u32>,
    }

    impl MockClipboardBackend {
        pub fn new(script: Vec<Step>) -> Self {
            Self {
                script: script.into(),
                seq: Some(1),
            }
        }

        /// 覆盖序列号。`None` 模拟「平台不提供序列号」的降级路径。
        pub fn with_seq(mut self, seq: Option<u32>) -> Self {
            self.seq = seq;
            self
        }
    }

    impl ClipboardBackend for MockClipboardBackend {
        /// 脚本按顺序消费；只剩最后一步时**原地重复**，模拟真实剪贴板
        /// 「内容停在最后一次复制」的行为。这样 worker 的防抖逻辑才有机会
        /// 被触发 —— 一次性消费完会让读取变成 Empty，而 Empty 分支不会
        /// 走到防抖检查（与重构前 `Err(_) => continue` 的位置一致）。
        fn get_text(&mut self) -> Result<ClipboardRead, ClipboardBackendError> {
            let step = if self.script.len() > 1 {
                self.script.pop_front().expect("len > 1")
            } else {
                self.script.front().cloned().unwrap_or(Step::Empty)
            };

            match step {
                Step::Text(t) => Ok(ClipboardRead::Text(t.to_string())),
                Step::Empty => Ok(ClipboardRead::Empty),
                Step::Transient(code) => {
                    Err(ClipboardBackendError::transient(code, "mock transient"))
                }
                Step::Fatal(code) => Err(ClipboardBackendError::fatal(code, "mock fatal")),
            }
        }

        fn seq(&self) -> Option<u32> {
            self.seq
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::{MockClipboardBackend, Step};
    use super::{ClipboardBackend, ClipboardErrorKind, ClipboardRead};

    #[test]
    fn empty_clipboard_is_not_an_error() {
        let mut b = MockClipboardBackend::new(vec![Step::Empty]);
        assert_eq!(b.get_text().unwrap(), ClipboardRead::Empty);
    }

    #[test]
    fn transient_and_fatal_are_distinguished() {
        let mut b = MockClipboardBackend::new(vec![
            Step::Transient("CLIPBOARD_OCCUPIED"),
            Step::Fatal("CLIPBOARD_INIT_FAILED"),
        ]);
        assert_eq!(
            b.get_text().unwrap_err().kind,
            ClipboardErrorKind::Transient
        );
        assert_eq!(b.get_text().unwrap_err().kind, ClipboardErrorKind::Fatal);
    }

    #[test]
    fn seq_is_reported_when_platform_provides_it() {
        let b = MockClipboardBackend::new(vec![Step::Text("a")]).with_seq(Some(7));
        assert_eq!(b.seq(), Some(7));

        let b = MockClipboardBackend::new(vec![Step::Text("a")]).with_seq(None);
        assert_eq!(b.seq(), None);
    }

    #[test]
    fn script_repeats_last_step_by_default() {
        let mut b = MockClipboardBackend::new(vec![Step::Text("only")]);
        for _ in 0..5 {
            assert_eq!(
                b.get_text().unwrap(),
                ClipboardRead::Text("only".to_string())
            );
        }
    }
}
