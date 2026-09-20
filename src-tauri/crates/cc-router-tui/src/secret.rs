//! API Key 一类的敏感文本。`Debug` 只印长度, 所以它可以安全地待在派生了 `Debug` 的 `Action` /
//! `Cmd` 里 (主循环、测试失败信息、将来任何 `{:?}` 都不会漏出明文)。读明文只有 `expose()` 一个
//! 入口, 名字刻意刺眼, 且有源码扫描测试限制它的调用点 (见 `expose_is_only_called_in_allowlisted_files`)。

/// 掩码最多画几个点。
pub const MASK_CAP: usize = 24;

#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// **唯一的明文出口。** 只允许出现在白名单文件里 (见本文件的
    /// `expose_is_only_called_in_allowlisted_files`)。
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 界面显示用: 与明文等长的 `•`, 超过 [`MASK_CAP`] 个字符时固定画 `MASK_CAP` 个
    /// (不泄露真实长度, 也不会撑破字段宽度)。空值返回空串。
    pub fn masked(&self) -> String {
        "•".repeat(self.0.chars().count().min(MASK_CAP))
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            write!(f, "Secret(empty)")
        } else {
            write!(f, "Secret({} chars)", self.0.chars().count())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_plaintext() {
        // "sk-abcdef" 是 9 个字符——与本文件顶部文档注释里许诺的例子对齐。
        let out = format!("{:?}", Secret::new("sk-abcdef"));
        assert!(!out.contains("sk-"), "{out}");
        assert!(out.contains("9 chars"), "{out}");

        let empty = format!("{:?}", Secret::default());
        assert_eq!(empty, "Secret(empty)");
    }

    #[test]
    fn masked_is_dots_and_caps_at_mask_cap() {
        assert_eq!(Secret::default().masked(), "");
        assert_eq!(Secret::new("sk-abcdef").masked(), "•".repeat(9));
        assert_eq!(Secret::new("x".repeat(100)).masked(), "•".repeat(MASK_CAP));
    }

    /// 照抄主 crate `runtime_file.rs::tests::local_secret_is_only_touched_by_allowlisted_files`
    /// 的写法: 遍历 `src/` 下的 `.rs`, 凡是出现 `.expose()` 的文件名必须在白名单里——防止有人在
    /// 表单渲染 / 日志 / 别的随手一个地方直接读明文。白名单三个成员见各自的注释。
    #[test]
    fn expose_is_only_called_in_allowlisted_files() {
        const EXPOSE_ALLOWLIST: [&str; 3] = [
            "secret.rs",  // 定义处与它自己的测试
            "dto.rs",     // `CreateInput::to_args()` / `ProbeInput::to_args()`, 唯一把它变成线上 JSON 的地方 (Task 3)
            "runtime.rs", // 把 to_args() 的结果发出去 (不直接碰明文, 但留给将来) (Task 3)
        ];

        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.filter_map(Result::ok) {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|e| e == "rs") {
                    out.push(p);
                }
            }
        }

        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src, &mut files);
        let offenders: Vec<String> = files
            .iter()
            .filter(|p| std::fs::read_to_string(p).is_ok_and(|text| text.contains(".expose(")))
            .filter_map(|p| p.strip_prefix(&src).ok().map(|rel| rel.to_string_lossy().replace('\\', "/")))
            .filter(|rel| !EXPOSE_ALLOWLIST.contains(&rel.as_str()))
            .collect();
        assert!(offenders.is_empty(), ".expose() 出现在白名单之外: {offenders:?}");
    }
}
