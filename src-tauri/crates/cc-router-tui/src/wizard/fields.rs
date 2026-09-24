//! 两条路径各自的字段、校验与预填。与 `mod.rs` 分开是因为这些是**纯数据与纯函数**: 给定
//! 一份草稿, 算出要画哪些行、哪些字段不合法。没有 `Frame`, 没有 `Cmd`, 好测。

use crate::client::dto::{AuthHeaderFormat, CustomProtocol, ModelInfo, ModelSlots, Slot};
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

/// 自定义厂商单页 (P5 Task 6) 的草稿。与 `BasicsDraft`/`SlotsDraft` 不同, 这一页把"选协议"
/// "填连接信息""探测模型""选槽位"全放在同一屏, 所以字段更多; `slots` 直接复用 `SlotsDraft`
/// (含它自己的 `note`——探测失败 / 创建失败的说明行共用这一个字段, 与 `Stage::Slots` 里
/// `ManualFallback` 与 `SaveSlots` 失败共用同一个 `note` 是同一条设计, "谁最后发生谁的文案盖住")。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomDraft {
    pub protocol: CustomProtocol,
    pub provider_display_name: String,
    pub base_url: String,
    pub messages_path: String,
    /// 锁定协议下恒等于 `protocol.preset()` 的那一对; Anthropic 下是 `ANTHROPIC_AUTH_PRESETS`
    /// 里选的那一对。
    pub auth_header_name: String,
    pub auth_header_format: AuthHeaderFormat,
    pub api_key: Secret,
    pub display_name: String,
    pub slots: SlotsDraft,
    /// 上一次**成功**探测的结果: 那一刻的 `base_url` 与后端回的 `models_url`。换协议、探测失败
    /// 都要清空 (`apply_protocol`); **编辑 `base_url` 本身不清**——`models_url()` 自己按值比对,
    /// 提前清空反而会丢掉"改回去又生效"这条桌面端行为。
    pub probe: Option<ProbedModels>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedModels {
    pub base_url: String,
    pub models_url: String,
}

impl CustomDraft {
    /// 选中厂商 picker 里的 `custom:<protocol>` 条目时构造一份全新草稿——除了协议预设字段,
    /// 其余全是空值 (还没填过任何东西)。内部就是"造一个占位壳 + `apply_protocol`", 不重复
    /// 一遍预设填充逻辑。
    pub fn new(protocol: CustomProtocol) -> Self {
        let mut draft = CustomDraft {
            protocol,
            provider_display_name: String::new(),
            base_url: String::new(),
            messages_path: String::new(),
            auth_header_name: String::new(),
            auth_header_format: AuthHeaderFormat::Bearer,
            api_key: Secret::default(),
            display_name: String::new(),
            slots: SlotsDraft::default(),
            probe: None,
        };
        draft.apply_protocol(protocol);
        draft
    }

    /// 换协议: 把 `base_url` / `messages_path` / 鉴权头重置成这个协议的预设, 并清掉 `probe`、
    /// 探测到的候选模型 (`slots.models`) 与"自动获取失败"的说明行 (`slots.note`)——旧协议探测
    /// 到的模型对新协议没有意义 (评审 5: OpenAI Responses 下探测到 `gpt-5.5`, 切到 Gemini 之后
    /// `Fable` 的 picker 候选里还挂着 `gpt-5.5`, 选上就建出模型名对不上协议的 Gemini 订阅), 旧的
    /// 失败说明同样过期。**已经填进槽位的值不动**(评审 5 明确: 只清候选与说明, 不清用户已经选定
    /// 的槽位), **API Key / 备注名也不动** (桌面端同规则——用户切协议大概率是选错了重选, 不该连
    /// 已经填好的凭据/名字都丢)。
    pub fn apply_protocol(&mut self, protocol: CustomProtocol) {
        let preset = protocol.preset();
        self.protocol = protocol;
        self.base_url = preset.base_url.to_string();
        self.messages_path = preset.messages_path.to_string();
        self.auth_header_name = preset.auth_header_name.to_string();
        self.auth_header_format = preset.auth_header_format;
        self.probe = None;
        self.slots.models = Vec::new();
        self.slots.note = None;
    }

    /// 只有探测成功、且此后 `base_url`(trim 后) 一个字都没改过时才回传 `models_url`。与桌面端
    /// `customProbe.baseUrl === baseUrl` 同规则——`probe.base_url` 在探测那一刻已经 trim 过
    /// (见 `wizard::mod::apply_wizard_result` 的 `Probed(Ok(Auto))` 分支), 这里只需要再 trim
    /// 一遍*当前*的 `base_url` 参与比较, 用户中途多打的首尾空白不该算"改过"。
    pub fn models_url(&self) -> Option<&str> {
        match &self.probe {
            Some(p) if p.base_url == self.base_url.trim() => Some(p.models_url.as_str()),
            _ => None,
        }
    }
}

/// 自定义单页的字段, 顺序即上下键的顺序——但与 `BasicsField`/`SlotsField` 不同, 这里的可聚焦
/// 列表**依赖运行时状态** (`Auth` 锁定时被排除), 所以没有固定的 `ALL` 常量, 改用 [`CustomField::all`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomField {
    Protocol,
    ProviderName,
    BaseUrl,
    MessagesPath,
    Auth,
    ApiKey,
    DisplayName,
    Probe,
    Slot(Slot),
    Submit,
}

impl CustomField {
    /// 焦点导航顺序。`auth_locked` 为真时 `Auth` 被剔除在外 (↑↓ 跳过它)——这一行仍然会被画出来
    /// (锁定态, muted), 只是键盘导航永远不会落到它上面; `⏎` 在它身上的时候 (理论上不该发生,
    /// 见 `wizard::mod::handle_custom_key`) 也什么都不做。
    pub fn all(auth_locked: bool) -> Vec<CustomField> {
        let mut fields = vec![CustomField::Protocol, CustomField::ProviderName, CustomField::BaseUrl, CustomField::MessagesPath];
        if !auth_locked {
            fields.push(CustomField::Auth);
        }
        fields.push(CustomField::ApiKey);
        fields.push(CustomField::DisplayName);
        fields.push(CustomField::Probe);
        fields.extend([Slot::Fable, Slot::Opus, Slot::Sonnet, Slot::Haiku, Slot::Fallback].map(CustomField::Slot));
        fields.push(CustomField::Submit);
        fields
    }
}

/// 校验顺序与桌面端 `saveCustom` 逐条对齐: 厂商名 → base_url 非空 → base_url 前缀 →
/// messages_path 前缀 → gemini 的 `{model}` → API Key → 备注名 → 四个核心槽。
///
/// **`base_url`/`messages_path` 校验的是 `trim` 后的值, 不是原样的草稿字符串**——这是有意
/// 修正桌面端"校验不 trim、提交时才 trim"的不一致 (桌面端校验用原始输入, 真正发请求前才
/// `.trim()`, 于是"Base URL 只有首尾空白"这种输入能通过校验、发请求时却变成空字符串; TUI
/// 这里直接校验 trim 后的值, 校验通过 ⇔ 提交时真正发出去的值也合法), 与后端自己对这些字段的
/// 校验口径一致 (评审确认: 不是 bug, 不要为了跟桌面端字面一致而改回去)。
pub fn validate_custom(d: &CustomDraft, s: &'static Strings) -> Option<(CustomField, &'static str)> {
    if d.provider_display_name.trim().is_empty() {
        return Some((CustomField::ProviderName, s.wiz_err_provider_name));
    }
    let base_url = d.base_url.trim();
    if base_url.is_empty() {
        return Some((CustomField::BaseUrl, s.wiz_err_base_url_empty));
    }
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Some((CustomField::BaseUrl, s.wiz_err_base_url_scheme));
    }
    let messages_path = d.messages_path.trim();
    if !messages_path.starts_with('/') {
        return Some((CustomField::MessagesPath, s.wiz_err_messages_path));
    }
    if d.protocol.requires_model_placeholder() && !messages_path.contains("{model}") {
        return Some((CustomField::MessagesPath, s.wiz_err_gemini_placeholder));
    }
    if d.api_key.is_empty() {
        return Some((CustomField::ApiKey, s.wiz_err_api_key));
    }
    if d.display_name.trim().is_empty() {
        return Some((CustomField::DisplayName, s.wiz_err_display_name));
    }
    validate_slots(&d.slots, s).map(|(slot, message)| (CustomField::Slot(slot), message))
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

    /// 五个协议各一次: `base_url`/`messages_path`/鉴权头都应该等于 `preset()`, 且旧的 `probe`
    /// 应该被清空——`apply_protocol` 是唯一改这三个连接字段的入口, 不管调用前草稿是什么状态。
    #[test]
    fn apply_protocol_prefills_and_clears_the_probe() {
        for protocol in CustomProtocol::ALL {
            let mut d = CustomDraft::new(protocol);
            d.probe = Some(ProbedModels { base_url: "https://old.example.com".into(), models_url: "https://old.example.com/v1/models".into() });

            d.apply_protocol(protocol);

            let preset = protocol.preset();
            assert_eq!(d.base_url, preset.base_url, "{protocol:?}");
            assert_eq!(d.messages_path, preset.messages_path, "{protocol:?}");
            assert_eq!(d.auth_header_name, preset.auth_header_name, "{protocol:?}");
            assert_eq!(d.auth_header_format, preset.auth_header_format, "{protocol:?}");
            assert!(d.probe.is_none(), "换协议应该清空 probe ({protocol:?})");
        }
    }

    /// 评审 5: 换协议还应该清空探测到的候选模型 (`slots.models`) 与"自动获取失败"的说明行
    /// (`slots.note`)——旧协议探测到的模型 (比如 OpenAI Responses 下的 `gpt-5.5`) 对新协议
    /// (比如切到 Gemini) 没有意义, 留着会让用户在 Fable 的 picker 里选出模型名对不上协议的值。
    /// **已经填进槽位的值不该被清**(与 API Key / 备注名同规则)。
    #[test]
    fn apply_protocol_clears_stale_candidates_and_note_but_keeps_chosen_slots() {
        let mut d = CustomDraft::new(CustomProtocol::OpenaiResponses);
        d.slots.models = vec![ModelInfo { id: "gpt-5.5".into(), display_name: None }];
        d.slots.note = Some("上一次自动获取失败的原因".into());
        d.slots.slots.fable = "gpt-5.5".into();

        d.apply_protocol(CustomProtocol::Gemini);

        assert!(d.slots.models.is_empty(), "换协议应该清空旧协议探测到的候选模型");
        assert!(d.slots.note.is_none(), "换协议应该清空旧的说明行");
        assert_eq!(d.slots.slots.fable, "gpt-5.5", "已经填进槽位的值不该被换协议清掉");
    }

    /// 一份填满全部字段 (含四个核心槽) 的草稿——`CustomProtocol::Anthropic` 的预设 `base_url`
    /// 是空串, 这里补一个真实值, 其它协议的预设本来就是非空 https 地址, 原样保留。
    fn filled_custom_draft(protocol: CustomProtocol) -> CustomDraft {
        let mut d = CustomDraft::new(protocol);
        d.provider_display_name = "中转站".into();
        if d.base_url.is_empty() {
            d.base_url = "https://relay.example.com".into();
        }
        d.api_key = Secret::new("sk-test");
        d.display_name = "中转站".into();
        d.slots.slots = filled_slots();
        d
    }

    /// 逐条构造只违反其中一条规则的草稿, 断言 `validate_custom` 报的是那一条、不是别的——顺序
    /// 与桌面端 `saveCustom` 一致 (简报): 厂商名 → base_url 非空 → base_url 前缀 → messages_path
    /// 前缀 → gemini 的 `{model}` → API Key → 备注名 → 四个核心槽。
    #[test]
    fn validate_custom_follows_the_desktop_order() {
        let s = &crate::i18n::ZH;

        let empty = CustomDraft::new(CustomProtocol::Anthropic);
        assert_eq!(validate_custom(&empty, s), Some((CustomField::ProviderName, s.wiz_err_provider_name)), "全空应该先报厂商名");

        let mut bad_scheme = filled_custom_draft(CustomProtocol::Anthropic);
        bad_scheme.base_url = "api.example.com".into();
        assert_eq!(validate_custom(&bad_scheme, s), Some((CustomField::BaseUrl, s.wiz_err_base_url_scheme)), "base_url 缺 scheme 应该报这一条");

        let mut bad_path = filled_custom_draft(CustomProtocol::Anthropic);
        bad_path.messages_path = "v1/messages".into();
        assert_eq!(validate_custom(&bad_path, s), Some((CustomField::MessagesPath, s.wiz_err_messages_path)), "请求路径不带前导 / 应该报这一条");

        let mut gemini_missing_placeholder = filled_custom_draft(CustomProtocol::Gemini);
        gemini_missing_placeholder.messages_path = "/v1beta/models/generateContent".into();
        assert_eq!(
            validate_custom(&gemini_missing_placeholder, s),
            Some((CustomField::MessagesPath, s.wiz_err_gemini_placeholder)),
            "Gemini 的请求路径缺 {{model}} 应该单独报这一条"
        );

        // 变异验证目标: 这一条如果被误改成对 GeminiInteractions 也要求占位符, 这里就会失败——
        // `requires_model_placeholder()` 只对 `Gemini` 返回真。
        let mut gemini_interactions_same_path = filled_custom_draft(CustomProtocol::GeminiInteractions);
        gemini_interactions_same_path.messages_path = "/v1beta/models/generateContent".into();
        assert_eq!(validate_custom(&gemini_interactions_same_path, s), None, "GeminiInteractions 不要求占位符, 同样的路径应该通过");

        let mut missing_key = filled_custom_draft(CustomProtocol::Anthropic);
        missing_key.api_key = Secret::default();
        assert_eq!(validate_custom(&missing_key, s), Some((CustomField::ApiKey, s.wiz_err_api_key)));

        let mut missing_name = filled_custom_draft(CustomProtocol::Anthropic);
        missing_name.display_name = "   ".into();
        assert_eq!(validate_custom(&missing_name, s), Some((CustomField::DisplayName, s.wiz_err_display_name)));

        let mut missing_slot = filled_custom_draft(CustomProtocol::Anthropic);
        missing_slot.slots.slots.opus = String::new();
        assert_eq!(validate_custom(&missing_slot, s), Some((CustomField::Slot(Slot::Opus), s.wiz_err_slot)), "四个核心槽任一空都应该报到那个槽");

        assert_eq!(validate_custom(&filled_custom_draft(CustomProtocol::Anthropic), s), None, "全填好应该通过");
    }

    /// 探测后原样 → `Some`; 改一个字符 → `None`; 改回去 → 又是 `Some`——与桌面端
    /// `customProbe.baseUrl === baseUrl` 同规则。
    #[test]
    fn models_url_is_only_sent_back_when_the_base_url_is_unchanged() {
        let mut d = CustomDraft::new(CustomProtocol::Anthropic);
        d.base_url = "https://relay.example.com".into();
        d.probe =
            Some(ProbedModels { base_url: "https://relay.example.com".into(), models_url: "https://relay.example.com/v1/models".into() });
        assert_eq!(d.models_url(), Some("https://relay.example.com/v1/models"), "探测后原样应该回传");

        d.base_url = "https://relay.example.com/changed".into();
        assert_eq!(d.models_url(), None, "改了一个字符应该不再回传");

        d.base_url = "https://relay.example.com".into();
        assert_eq!(d.models_url(), Some("https://relay.example.com/v1/models"), "改回去应该又生效");
    }
}
