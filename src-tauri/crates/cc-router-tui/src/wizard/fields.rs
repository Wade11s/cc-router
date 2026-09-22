//! 两条路径各自的字段、校验与预填。与 `mod.rs` 分开是因为这些是**纯数据与纯函数**: 给定
//! 一份草稿, 算出要画哪些行、哪些字段不合法。没有 `Frame`, 没有 `Cmd`, 好测。

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
pub fn api_key_display(key: &Secret, reveal: bool) -> String {
    if reveal { key.expose().to_string() } else { key.masked() }
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
        assert_eq!(api_key_display(&key, false), key.masked());
        assert_eq!(api_key_display(&key, true), "sk-test");
    }
}
