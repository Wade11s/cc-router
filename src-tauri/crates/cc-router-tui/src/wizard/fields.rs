//! 两条路径各自的字段、校验与预填。与 `mod.rs` 分开是因为这些是**纯数据与纯函数**: 给定
//! 一份草稿, 算出要画哪些行、哪些字段不合法。没有 `Frame`, 没有 `Cmd`, 好测。

use crate::client::dto::{ModelInfo, ModelSlots, Slot};
use crate::i18n::Strings;
use crate::secret::Secret;
use crate::store::Store;

/// 内置路径第一步的草稿。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BasicsDraft {
    pub provider_id: String,
    pub endpoint_id: String,
    pub api_key: Secret,
    /// `tui_input::Input` 不参与 `PartialEq` (与 `PickerState` 同一条道理), 所以备注名的文本
    /// 存在这里, 输入框本身由 `Wizard` 持有。
    pub display_name: String,
}

/// 第一步的字段。顺序即上下键的顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicsField {
    Provider,
    Endpoint,
    ApiKey,
    DisplayName,
    Submit,
}

impl BasicsField {
    pub const ALL: [BasicsField; 5] =
        [BasicsField::Provider, BasicsField::Endpoint, BasicsField::ApiKey, BasicsField::DisplayName, BasicsField::Submit];
}

/// 校验失败的字段与原因 (原因是 `Strings` 的字段, 不是字面量)。第一个不合法的字段决定光标落点——
/// 顺序与 `BasicsField::ALL` 一致: 厂商 → 接入点 → API Key → 备注名 (`Submit` 本身不参与校验,
/// 它是触发校验的那个按钮, 不可能是校验失败的对象)。
pub fn validate_basics(d: &BasicsDraft, s: &'static Strings) -> Option<(BasicsField, &'static str)> {
    if d.provider_id.is_empty() {
        return Some((BasicsField::Provider, s.wiz_err_provider));
    }
    if d.endpoint_id.is_empty() {
        return Some((BasicsField::Endpoint, s.wiz_err_endpoint));
    }
    if d.api_key.is_empty() {
        return Some((BasicsField::ApiKey, s.wiz_err_api_key));
    }
    if d.display_name.trim().is_empty() {
        return Some((BasicsField::DisplayName, s.wiz_err_display_name));
    }
    None
}

/// 备注名的默认值: 厂商显示名; `Store` 里已经有同名订阅时追加 ` 2` / ` 3` … 直到不重名。
///
/// 桌面端用的是 `<厂商名> <6 位随机 base36>`, TUI 不照抄: 这个 crate 没有随机数依赖 (也不为了
/// 一个后缀去加), 而序号比随机串可读。后端对 `display_name` 没有唯一性约束, 这只是默认值,
/// 用户随时可以改。
pub fn default_display_name(provider_name: &str, store: &Store) -> String {
    let taken = |name: &str| store.subscriptions().iter().any(|sub| sub.display_name == name);
    if !taken(provider_name) {
        return provider_name.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{provider_name} {n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// 界面上要显示 API Key 时的**唯一**明文出口 (`Ctrl+R` 就地切换)。`reveal` 为假返回掩码,
/// 为真返回明文——`wizard/mod.rs::draw` 只调这一个函数, 不直接碰 `Secret::expose`
/// (`secret.rs::EXPOSE_ALLOWLIST` 的源码扫描测试盯着这一点)。
///
/// **刻意不用 `Secret::masked()`**（评审 I1）: 那个版本为了不泄露真实长度, 封顶在 `MASK_CAP`
/// (24) 个点; 但这里的光标 / 横向滚动是按**明文**的 `Input::visual_cursor()` 算的 (`wizard/mod.rs
/// ::draw_basics`), 一旦掩码文本比明文短, 光标就会飞到掩码串右边的空白里——64 字符以上的 key
/// (Anthropic 的约 108 字符) 掩码后甚至一个点都不剩, 看起来像没填, 用户会以为粘贴失败再粘一次,
/// 内容被拼成两份。这里要的是"挡住肉眼"而不是"隐藏长度"(表单正在编辑一条还没保存的 key, 长度
/// 泄露不是这个场景的威胁模型), 所以逐字给一个点、不封顶, 让掩码文本与明文逐字对齐, 光标/滚动
/// 天然正确。`Secret::masked()` 本身不改——它留给"不可编辑的只读展示"这个未来场景, 那里不涉及
/// 光标对齐, 封顶避免泄露长度是对的。
pub fn api_key_display(key: &Secret, reveal: bool) -> String {
    let plain = key.expose();
    if reveal { plain.to_string() } else { "•".repeat(plain.chars().count()) }
}

/// 第二步 (绑定模型) 的草稿。与订阅页的 `Draft<Subscription>` 不同: 向导是从零填, 没有"与 Store
/// 比对相等就丢弃"的问题, 所以直接放一份 `ModelSlots`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotsDraft {
    pub slots: ModelSlots,
    /// 可选的模型候选 (来自 `refresh_model_list` / `probe_custom_models`), 空 = 只能手输。
    pub models: Vec<ModelInfo>,
    /// 自动获取失败时的原因, 画成一条说明行。
    pub note: Option<String>,
}

/// 第二步的字段: 五个槽位行 (`Row`, 带着是哪个 `Slot`) + 保存按钮。顺序即上下键的顺序——与
/// `BasicsField` 同一套 `ALL` + `move_focus` 写法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotsField {
    Row(Slot),
    Save,
}

impl SlotsField {
    pub const ALL: [SlotsField; 6] = [
        SlotsField::Row(Slot::Fable),
        SlotsField::Row(Slot::Opus),
        SlotsField::Row(Slot::Sonnet),
        SlotsField::Row(Slot::Haiku),
        SlotsField::Row(Slot::Fallback),
        SlotsField::Save,
    ];
}

/// 四个核心槽位都要非空 (兜底槽可以空 = 未配置), 与桌面端 `allSlotsFilled` 同规则。第一个不合法的
/// 槽位决定光标落点, 顺序与 `SlotsField::ALL` 一致 (fable → opus → sonnet → haiku; `Fallback`
/// 不参与, 不可能是校验失败的对象)。
pub fn validate_slots(d: &SlotsDraft, s: &'static Strings) -> Option<(Slot, &'static str)> {
    for slot in [Slot::Fable, Slot::Opus, Slot::Sonnet, Slot::Haiku] {
        if d.slots.get(slot).is_empty() {
            return Some((slot, s.wiz_err_slot));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled_draft() -> BasicsDraft {
        BasicsDraft {
            provider_id: "zhipu".into(),
            endpoint_id: "default".into(),
            api_key: Secret::new("sk-test"),
            display_name: "智谱 AI".into(),
        }
    }

    #[test]
    fn validate_basics_reports_the_first_bad_field() {
        let s = &crate::i18n::ZH;

        let empty = BasicsDraft::default();
        assert_eq!(validate_basics(&empty, s), Some((BasicsField::Provider, s.wiz_err_provider)), "全空应该先报厂商");

        let missing_key = BasicsDraft { api_key: Secret::default(), ..filled_draft() };
        assert_eq!(validate_basics(&missing_key, s), Some((BasicsField::ApiKey, s.wiz_err_api_key)), "只缺 key 应该报 ApiKey, 不是别的字段");

        assert_eq!(validate_basics(&filled_draft(), s), None, "都填了应该通过");

        // 备注名全是空白也算空 (trim 之后判断)。
        let blank_name = BasicsDraft { display_name: "   ".into(), ..filled_draft() };
        assert_eq!(validate_basics(&blank_name, s), Some((BasicsField::DisplayName, s.wiz_err_display_name)));
    }

    fn store_with_names(names: &[&str]) -> Store {
        let mut store = Store::default();
        let subs = names
            .iter()
            .enumerate()
            .map(|(i, name)| crate::client::dto::Subscription {
                id: format!("s{i}"),
                display_name: (*name).to_string(),
                provider_display_name: "p".into(),
                enabled: true,
                state: crate::client::dto::SubscriptionState::Healthy,
                cooldown_until: None,
                last_error_message: None,
                is_dispatchable: true,
                quota_usage: vec![],
                provider_id: "p".into(),
                base_url: "https://example.invalid".into(),
                auth_type: "api_key".into(),
                model_slots: crate::client::dto::ModelSlots::pending(),
                slot_efforts: Default::default(),
                referenced_by: vec![],
                balance_supported: false,
                balance_cache: None,
                model_cache: None,
            })
            .collect();
        store.apply_subscriptions(1, subs);
        store
    }

    #[test]
    fn default_display_name_appends_a_number_on_collision() {
        let empty = Store::default();
        assert_eq!(default_display_name("智谱 AI", &empty), "智谱 AI", "没有重名时原样返回");

        let one_taken = store_with_names(&["智谱 AI"]);
        assert_eq!(default_display_name("智谱 AI", &one_taken), "智谱 AI 2");

        let two_taken = store_with_names(&["智谱 AI", "智谱 AI 2"]);
        assert_eq!(default_display_name("智谱 AI", &two_taken), "智谱 AI 3");
    }

    #[test]
    fn api_key_display_masks_unless_revealed() {
        let key = Secret::new("sk-test");
        assert_eq!(api_key_display(&key, false), "•".repeat(7));
        assert_eq!(api_key_display(&key, true), "sk-test");
    }

    /// I1: 掩码不能封顶在 `Secret::MASK_CAP` (24) —— 否则超长 key (Anthropic 实测约 108 字符)
    /// 掩码后比明文短, 靠明文 `visual_cursor()` 算的光标会飞到掩码串右边的空白里, 64 字符以上
    /// 甚至会显示成空字符串。
    #[test]
    fn api_key_display_masks_without_a_length_cap() {
        let long_key = "x".repeat(108);
        let key = Secret::new(long_key.clone());
        let masked = api_key_display(&key, false);
        assert_eq!(masked.chars().count(), 108, "掩码应该逐字对应明文长度, 不能封顶");
        assert_ne!(masked, key.masked(), "这里不该复用 Secret::masked() 的封顶版本");
        assert_eq!(api_key_display(&key, true), long_key);
    }

    fn filled_slots() -> ModelSlots {
        ModelSlots { fable: "glm-4.6".into(), opus: "glm-4.6".into(), sonnet: "glm-4.6".into(), haiku: "glm-4.6".into(), fallback: String::new() }
    }

    #[test]
    fn validate_slots_ignores_the_fallback_slot() {
        let s = &crate::i18n::ZH;

        let full = SlotsDraft { slots: filled_slots(), models: vec![], note: None };
        assert_eq!(validate_slots(&full, s), None, "四个核心槽填了、兜底空应该通过");

        let missing_opus = SlotsDraft { slots: ModelSlots { opus: String::new(), ..filled_slots() }, ..full.clone() };
        assert_eq!(validate_slots(&missing_opus, s), Some((Slot::Opus, s.wiz_err_slot)), "少一个核心槽应该报那个槽, 不是别的");

        // 兜底槽本身留空不该被当成校验失败的对象。
        let fallback_only = SlotsDraft { slots: ModelSlots { fallback: String::new(), ..filled_slots() }, ..full.clone() };
        assert_eq!(validate_slots(&fallback_only, s), None, "兜底槽空着不该报错");
    }
}
