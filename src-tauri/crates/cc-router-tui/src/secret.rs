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
    /// 的写法: 遍历 `src/` 下的 `.rs`, 凡是出现 `expose(` 的文件名必须在白名单里——防止有人在
    /// 表单渲染 / 日志 / 别的随手一个地方直接读明文。白名单两个成员见各自的注释。
    ///
    /// Review round 1 的两处修正:
    /// - `p.strip_prefix(&src)` 产出的是**相对 `src/` 的路径**, 与后端 `runtime_file.rs` 白名单
    ///   写 `"proxy/server.rs"` 这种带目录的形式同一套规则——`dto.rs` 真实路径是
    ///   `src/client/dto.rs`, 白名单必须写 `"client/dto.rs"`, 不能只写文件名 (原来的
    ///   `"dto.rs"` 是个死条目, 永远不命中, Task 3 一往 `client/dto.rs` 里写 `.expose()` 就会被
    ///   误判成「出现在白名单之外」)。
    /// - 匹配串从 `.expose(` 放宽成 `expose(`: 前者只认得到方法调用语法 `x.expose()`, UFCS 写法
    ///   `Secret::expose(&x)` 不含 `.` 会被绕过。放宽后会多抓到本文件自己的定义/文档, 但白名单本来
    ///   就含 `secret.rs`, 无妨。
    ///
    /// Task 3 评审 #8: `runtime.rs` 曾经也在这张白名单里 (当初是「留给将来」占的位), 但
    /// `call_wizard`/`call_mutation` 实际调的是 `to_args()`, 一个 `expose(` 都没有——它又恰好是
    /// 「最容易不小心把明文拼进某条错误文本 / 日志」的地方, 所以从白名单里去掉: 谁真的要在这个文件
    /// 里读明文, 得先说明白为什么, 而不是顺着一个「留着也无妨」的旧条目继续写下去。
    ///
    /// 另外加一条「白名单成员必须真的存在于 `src/` 下」的断言, 防止以后再出现同类死条目。
    #[test]
    fn expose_is_only_called_in_allowlisted_files() {
        const EXPOSE_ALLOWLIST: [&str; 3] = [
            "secret.rs",     // 定义处与它自己的测试
            "client/dto.rs", // `CreateInput::to_args()` / `ProbeInput::to_args()`, 唯一把它变成线上 JSON 的地方 (Task 3)
            // `SecretField::display()`: 向导 API Key 行 `Ctrl+R` 就地切换明文/掩码显示 (P5 Task 4 起;
            // 7R-a 随文本字段类型从 `wizard/fields.rs` 挪到这里)。`SecretField` 的输入框私有, 明文
            // 在这个文件之外只能经 `secret()` → `expose()` 拿到, 所以这个文件是向导里唯一读明文的地方。
            "wizard/text.rs",
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

        for member in EXPOSE_ALLOWLIST {
            assert!(src.join(member).is_file(), "白名单成员 {member:?} 在 src/ 下不存在, 是个死条目");
        }

        let mut files = Vec::new();
        walk(&src, &mut files);
        let offenders: Vec<String> = files
            .iter()
            .filter(|p| {
                // 读不了源码文件说明这条扫描规则没有真正覆盖全部文件——宁可 panic 也不要把它当
                // 「没有 expose(」悄悄放过 (fail-closed, Review round 1)。
                let text = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("读取 {p:?} 失败: {e}"));
                text.contains("expose(")
            })
            .filter_map(|p| p.strip_prefix(&src).ok().map(|rel| rel.to_string_lossy().replace('\\', "/")))
            .filter(|rel| !EXPOSE_ALLOWLIST.contains(&rel.as_str()))
            .collect();
        assert!(offenders.is_empty(), "expose( 出现在白名单之外: {offenders:?}");
    }
}
