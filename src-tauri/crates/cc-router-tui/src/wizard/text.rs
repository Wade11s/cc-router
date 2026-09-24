//! 向导的文本字段。**文本只存一处**: 值就在 `tui_input::Input` 里, 草稿直接持有这两种类型——
//! 以前草稿存一份 `String`/`Secret`、`Wizard` 另存一份 `Input`, 每次程序化地改草稿 (选厂商时自动
//! 填备注名、换协议时重置 Base URL……) 都得记得再 `Input::new(...)` 重建输入框, 漏一处下一次按键
//! 就会拿输入框里的旧值把草稿盖回去。
//!
//! `SecretField` 的明文只在本文件里读 (字段私有, 外面只能拿到 `Secret` 或掩码后的显示文本), 所以
//! 本文件在 `secret.rs::EXPOSE_ALLOWLIST` 里。

use ratatui::crossterm::event::{Event, KeyEvent};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::secret::Secret;

/// 能接收文本编辑按键的字段——向导「按焦点找到那一格、交给它处理」那条统一路径只认这个。
pub trait TextInput {
    /// 处理一个按键, 返回**值**是否改变 (只移动光标返回 `false`)。
    fn handle(&mut self, key: KeyEvent) -> bool;

    /// 切换明文 / 掩码显示。只有 [`SecretField`] 有这个开关, 普通文本字段什么都不做。
    fn toggle_reveal(&mut self) {}
}

/// 普通文本字段。`PartialEq` 只比较值 (光标位置不是草稿内容)。
#[derive(Clone, Default)]
pub struct TextField {
    input: Input,
}

impl TextField {
    pub fn new(value: impl Into<String>) -> Self {
        Self { input: Input::new(value.into()) }
    }

    pub fn value(&self) -> &str {
        self.input.value()
    }

    /// 整体替换值, 光标放到末尾 (与用户刚打完这段文字时一致)。
    pub fn set(&mut self, value: impl Into<String>) {
        *self = Self::new(value);
    }

    /// 光标在值里的**显示列**偏移 (宽字符占两列), 交给 `FormRow::Field::cursor`。
    pub fn visual_cursor(&self) -> usize {
        self.input.visual_cursor()
    }
}

impl TextInput for TextField {
    fn handle(&mut self, key: KeyEvent) -> bool {
        self.input.handle_event(&Event::Key(key)).is_some_and(|changed| changed.value)
    }
}

impl PartialEq for TextField {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

impl Eq for TextField {}

impl std::fmt::Debug for TextField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TextField").field(&self.value()).finish()
    }
}

/// API Key 一类的敏感文本字段: 编辑行为同 [`TextField`], 外加 `Ctrl+R` 切换的明文显示开关。
/// `Debug` 与 [`Secret`] 一样只印长度 (草稿派生了 `Debug`, 不能从这里漏出明文); `PartialEq` 只比较值。
#[derive(Clone, Default)]
pub struct SecretField {
    input: TextField,
    reveal: bool,
}

impl SecretField {
    /// 只有测试需要直接造一个有值的 (界面上它总是从空值开始, 靠按键填)。
    #[cfg(test)]
    pub fn new(value: impl Into<String>) -> Self {
        Self { input: TextField::new(value), reveal: false }
    }

    /// 发请求时用的值——按需生成, 草稿里不另存一份。
    pub fn secret(&self) -> Secret {
        Secret::new(self.input.value())
    }

    pub fn is_empty(&self) -> bool {
        self.input.value().is_empty()
    }

    pub fn visual_cursor(&self) -> usize {
        self.input.visual_cursor()
    }

    /// 界面上显示 API Key 的**唯一**出口: 没按 `Ctrl+R` 时是掩码, 按了是明文。
    ///
    /// **刻意不用 `Secret::masked()`**（评审 I1）: 那个版本为了不泄露真实长度, 封顶在 `MASK_CAP`
    /// (24) 个点; 但表单的光标 / 横向滚动是按**明文**的 `visual_cursor()` 算的 (`widgets::form`),
    /// 一旦掩码文本比明文短, 光标就会飞到掩码串右边的空白里——64 字符以上的 key (Anthropic 的约
    /// 108 字符) 掩码后甚至一个点都不剩, 看起来像没填, 用户会以为粘贴失败再粘一次, 内容被拼成两份。
    /// 这里要的是"挡住肉眼"而不是"隐藏长度"(表单正在编辑一条还没保存的 key, 长度泄露不是这个场景
    /// 的威胁模型), 所以逐字给一个点、不封顶, 让掩码文本与明文逐字对齐, 光标/滚动天然正确。
    /// `Secret::masked()` 本身不改——它留给"不可编辑的只读展示"这个未来场景, 那里不涉及光标对齐,
    /// 封顶避免泄露长度是对的。
    pub fn display(&self) -> String {
        let secret = self.secret();
        let plain = secret.expose();
        if self.reveal { plain.to_string() } else { "•".repeat(plain.chars().count()) }
    }
}

impl TextInput for SecretField {
    fn handle(&mut self, key: KeyEvent) -> bool {
        self.input.handle(key)
    }

    fn toggle_reveal(&mut self) {
        self.reveal = !self.reveal;
    }
}

impl PartialEq for SecretField {
    fn eq(&self, other: &Self) -> bool {
        self.input == other.input
    }
}

impl Eq for SecretField {}

impl std::fmt::Debug for SecretField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretField({:?}, reveal: {})", self.secret(), self.reveal)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn set_replaces_the_value_and_puts_the_cursor_at_the_end() {
        let mut f = TextField::new("abc");
        assert_eq!(f.value(), "abc");
        f.set("中转站");
        assert_eq!(f.value(), "中转站");
        assert_eq!(f.visual_cursor(), 6, "光标应该在末尾 (三个宽字符 = 6 列)");
        // 接着打字应该追加在末尾, 不是插在旧光标位置。
        f.handle(key(KeyCode::Char('X')));
        assert_eq!(f.value(), "中转站X");
    }

    #[test]
    fn handle_reports_whether_the_value_changed() {
        let mut f = TextField::default();
        assert!(f.handle(key(KeyCode::Char('a'))), "打字改变了值");
        assert!(!f.handle(key(KeyCode::Left)), "只移动光标不算改变");
        assert!(!f.handle(key(KeyCode::Right)), "只移动光标不算改变");
        assert!(!f.handle(key(KeyCode::Delete)), "光标在末尾, 删除什么也没改");
        assert!(f.handle(key(KeyCode::Backspace)), "退格删掉了 a");
        assert_eq!(f.value(), "");
        assert!(!f.handle(key(KeyCode::Backspace)), "空值再退格不算改变");
    }

    #[test]
    fn text_fields_compare_by_value_only() {
        let mut moved = TextField::new("abc");
        moved.handle(key(KeyCode::Left));
        assert_eq!(moved, TextField::new("abc"), "光标位置不同不影响相等");
        assert_ne!(TextField::new("abc"), TextField::new("abd"));
    }

    #[test]
    fn secret_field_display_masks_unless_revealed() {
        let mut key_field = SecretField::new("sk-test");
        assert_eq!(key_field.display(), "•".repeat(7));
        key_field.toggle_reveal();
        assert_eq!(key_field.display(), "sk-test");
        key_field.toggle_reveal();
        assert_eq!(key_field.display(), "•".repeat(7));
    }

    /// I1: 掩码不能封顶在 `Secret::MASK_CAP` (24) —— 否则超长 key (Anthropic 实测约 108 字符)
    /// 掩码后比明文短, 靠明文 `visual_cursor()` 算的光标会飞到掩码串右边的空白里, 64 字符以上
    /// 甚至会显示成空字符串。
    #[test]
    fn secret_field_display_masks_without_a_length_cap() {
        let long_key = "x".repeat(108);
        let mut field = SecretField::new(long_key.clone());
        let masked = field.display();
        assert_eq!(masked.chars().count(), 108, "掩码应该逐字对应明文长度, 不能封顶");
        assert_ne!(masked, field.secret().masked(), "这里不该复用 Secret::masked() 的封顶版本");
        field.toggle_reveal();
        assert_eq!(field.display(), long_key);
    }

    /// 草稿派生了 `Debug`, `SecretField` 的 `Debug` 不能漏出明文 (与 `Secret` 同一条纪律)。
    #[test]
    fn secret_field_debug_never_prints_the_plaintext() {
        let mut field = SecretField::new("sk-abcdef");
        field.toggle_reveal(); // 明文显示开关不影响 Debug
        let out = format!("{field:?}");
        assert!(!out.contains("sk-"), "{out}");
        assert!(out.contains("9 chars"), "{out}");
    }

    #[test]
    fn secret_field_edits_like_a_text_field() {
        let mut field = SecretField::default();
        assert!(field.is_empty());
        assert!(field.handle(key(KeyCode::Char('s'))));
        assert!(field.handle(key(KeyCode::Char('k'))));
        assert!(!field.handle(key(KeyCode::Left)));
        assert_eq!(field.secret(), Secret::new("sk"));
        assert!(!field.is_empty());
    }
}
