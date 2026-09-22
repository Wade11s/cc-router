//! 界面文案。`struct Strings` + 每种语言一个 `const`: 加字段时漏填任何一种语言都是编译错误,
//! 不需要运行时的「缺 key」检查。带参数的文案用 `fn` 指针, 各语言自己决定语序。
//!
//! 文案与桌面端独立一份 (TUI 用语更短), 但状态名等术语沿用桌面端 `src/i18n/locales/zh.json` 的叫法。
//! **en / ja 译文在 P6 补**: 现在 [`strings`] 对三种语言都返回 [`ZH`], 语言解析逻辑已经是最终形态。

use crate::client::dto::{QuotaPeriod, RoutingMode, SubscriptionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
    Ja,
}

impl Lang {
    /// `preferred` 来自桌面端设置 (`"system"` / `"zh"` / `"en"` / `"ja"`)。
    /// `"system"` 时按 `LC_ALL` → `LC_MESSAGES` → `LANG` 取系统语言, 映射规则与桌面端
    /// `src/i18n/index.tsx::detectSystemLocale` / `tray.rs` 一致: `zh*` → zh, `ja*` → ja, 其余 → en。
    pub fn resolve(preferred: &str, env: impl Fn(&str) -> Option<String>) -> Self {
        let tag = match preferred {
            "system" | "" => ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|k| env(k).filter(|v| !v.is_empty()))
                .unwrap_or_default(),
            other => other.to_string(),
        };
        let lower = tag.to_lowercase();
        if lower.starts_with("zh") {
            Self::Zh
        } else if lower.starts_with("ja") {
            Self::Ja
        } else {
            Self::En
        }
    }
}

pub struct Strings {
    /// 五个标签, 顺序即 `1`–`5`。
    pub tabs: [&'static str; 5],
    pub conn_connecting: &'static str,
    pub conn_connected: &'static str,
    pub conn_reconnecting: &'static str,

    pub key_switch_tab: &'static str,
    pub key_refresh: &'static str,
    pub key_help: &'static str,
    pub key_quit: &'static str,
    pub key_close: &'static str,
    pub key_select: &'static str,
    pub key_detail: &'static str,
    pub key_back: &'static str,
    pub key_toggle: &'static str,
    pub key_test: &'static str,
    pub key_models: &'static str,
    pub key_balance: &'static str,
    /// Task 5: 订阅详情里改当前槽位的模型 (⏎) / 思考档位 (o) / 保存草稿 (s)。
    pub key_edit_model: &'static str,
    pub key_edit_effort: &'static str,
    pub key_save: &'static str,
    /// M6 (fix round final): 脏页面上 `Esc` 的 hint 文案 ("放弃"), 与 `key_back` ("返回", 不脏时
    /// 用) 区分开——同一个键在脏/不脏两种状态下的语义不同, 底栏提示也该跟着换。
    pub key_discard: &'static str,
    /// Task 6: 虚拟模型页——成员列表里 `J`/`K` 重排序、`a` 加入、`x` 移除。
    pub key_move: &'static str,
    pub key_add: &'static str,
    pub key_remove: &'static str,
    pub key_mode: &'static str,
    /// M6 (fix round final): 虚拟模型页 Models 焦点下 `⏎` 的 hint 文案 ("成员")——比旧的
    /// `key_detail` ("详情") 更准确地描述这个键的作用 (进入这个虚拟模型的订阅成员列表)。
    pub key_members: &'static str,
    /// P5 Task 7 起订阅页 `n` 的 hint 文案 ("新建")。
    pub key_new: &'static str,
    /// 向导底栏右侧固定提示 ("取消"); 弹窗的取消统一用 `key_close` ("关闭"), 向导用这个不同的词是
    /// 因为向导按 Esc 退出会放弃已经填的内容, 语气上更接近「取消这次新建」。
    pub key_cancel: &'static str,

    pub help_title: &'static str,
    /// (键, 说明)
    pub help_rows: &'static [(&'static str, &'static str)],

    pub confirm_title: &'static str,
    /// 确认弹窗底部的键位提示 (y 是 / n 否)。
    pub confirm_keys: &'static str,
    /// 当前页面有未保存修改时, 退出 / 切页前弹出的确认文案。
    pub confirm_discard: &'static str,

    /// 过滤选择弹窗里「使用当前输入」那一行的文案, 参数是输入框里 (trim 过的) 文本。
    pub picker_use_typed: fn(text: &str) -> String,
    /// 过滤后没有任何匹配项时列表区显示的占位文案 (且 `allow_custom` 为 false, 或者输入框非空但
    /// 没有匹配——I2(d) 起 "零匹配 + 空输入 + 允许自定义" 这种情况改用 [`Strings::picker_type_to_enter`])。
    pub picker_empty: &'static str,
    /// I2(d): `allow_custom` 且输入框为空 (trim 之后) 且没有任何候选项时的占位文案——引导用户
    /// 打字后回车直接用输入的文本, 与 `picker_empty` ("没有匹配项", 用于确实存在候选但过滤不出
    /// 结果、或者压根不允许自定义的场景) 区分开。
    pub picker_type_to_enter: &'static str,
    /// 过滤选择弹窗底部的键位提示。
    pub picker_keys: &'static str,
    /// Task 5: 改模型 / 改思考档位两个 picker 的标题, 参数是槽位显示名 (四个主槽的英文原名, 或
    /// [`Strings::sub_slot_fallback`])。
    pub pick_model_title: fn(slot: &str) -> String,
    pub pick_effort_title: fn(slot: &str) -> String,
    /// 兜底槽模型 picker 里置顶的「清空」选项 (对应 `id: ""`)。
    pub pick_clear_fallback: &'static str,

    /// 只读详情弹窗 (`Popup::Detail`, Task 1; Task 8 起被请求日志详情页使用) 底部的键位提示。
    pub detail_keys: &'static str,

    pub too_small: &'static str,
    pub loading: &'static str,
    pub version_mismatch: fn(tui: &str, app: &str) -> String,

    pub ov_today: &'static str,
    pub ov_requests: &'static str,
    pub ov_success_rate: &'static str,
    pub ov_tokens: &'static str,
    pub ov_hourly: &'static str,
    pub ov_health: &'static str,
    pub ov_auth_on: &'static str,
    pub ov_auth_off: &'static str,
    pub ov_listen_all: &'static str,
    pub ov_subs_summary: fn(total: usize, dispatchable: usize) -> String,
    pub ov_no_subs: &'static str,
    pub ov_more_rows: fn(hidden: usize) -> String,

    pub st_healthy: &'static str,
    pub st_rate_limited: &'static str,
    pub st_quota_exhausted: &'static str,
    pub st_transient_error: &'static str,
    pub st_auth_failed: &'static str,
    pub st_disabled: &'static str,
    pub st_unknown: &'static str,
    /// 状态是健康的, 但用户自己设的 token 限额满了 (不是 `SubscriptionState`)。
    pub st_quota_reached: &'static str,

    pub q_daily: &'static str,
    pub q_weekly: &'static str,
    pub q_monthly: &'static str,
    pub q_total: &'static str,

    pub toast_reconnected: &'static str,
    pub toast_load_failed: fn(reason: &str) -> String,
    /// 断线时按 e/t/m/b 的提示。
    pub toast_offline: &'static str,
    pub toast_enabled: fn(name: &str) -> String,
    pub toast_disabled: fn(name: &str) -> String,
    /// `model` 为 `None` 时 (网络错误等测不出具体 model) 只显示前半句。
    pub toast_test_ok: fn(name: &str, model: Option<&str>) -> String,
    pub toast_test_failed: fn(name: &str, message: &str) -> String,
    pub toast_models_ok: fn(name: &str, n: usize) -> String,
    pub toast_models_manual: fn(name: &str, reason: &str) -> String,
    pub toast_balance_ok: fn(name: &str) -> String,
    pub toast_balance_failed: fn(name: &str, reason: &str) -> String,
    pub toast_mutation_failed: fn(name: &str, message: &str) -> String,
    pub toast_slots_saved: fn(name: &str) -> String,
    pub toast_vm_saved: fn(vm: &str) -> String,

    pub sub_title: fn(usize) -> String,
    pub sub_col_name: &'static str,
    pub sub_col_provider: &'static str,
    pub sub_col_sonnet: &'static str,
    pub sub_col_state: &'static str,
    pub sub_f_state: &'static str,
    pub sub_f_provider: &'static str,
    pub sub_f_endpoint: &'static str,
    pub sub_f_slots: &'static str,
    pub sub_f_quota: &'static str,
    pub sub_f_balance: &'static str,
    pub sub_f_models: &'static str,
    pub sub_f_referenced: &'static str,
    pub sub_f_last_error: &'static str,
    /// 详情面板「状态」行后面追加的「上次操作」行: 显示最近一次就地操作 (启停/测试/刷新模型/
    /// 刷新余额) 的完整结果文案 (与对应 toast 同一份文本), 不再被 toast 的单行截断限制。
    pub sub_f_last_action: &'static str,
    pub sub_slot_fallback: &'static str,
    pub sub_slot_unset: &'static str,
    pub sub_effort_auto: &'static str,
    pub sub_balance_unsupported: &'static str,
    pub sub_balance_never: &'static str,
    pub sub_balance_unavailable: &'static str,
    pub sub_models_cached: fn(usize) -> String,
    pub sub_models_never: &'static str,
    pub sub_unreferenced: &'static str,
    pub sub_help_rows: &'static [(&'static str, &'static str)],
    /// 详情面板「状态」行后面追加的进行中文案 (busy 行)。
    pub sub_busy_toggling: &'static str,
    pub sub_busy_testing: &'static str,
    pub sub_busy_models: &'static str,
    pub sub_busy_balance: &'static str,
    /// `Mutation::UpdateSlots` (Task 5 起从订阅详情页发起) 的进行中文案; 与四个既有就地操作用同一套
    /// 「状态行后追加 busy 文案」机制。
    pub sub_busy_saving: &'static str,
    /// Task 5: 草稿里跟 `Store` 当前值不同的槽位行末尾追加的 muted 提示。
    pub sub_slot_modified: &'static str,
    /// 有草稿时按 e/t/m/b 的拒绝提示 (避免重拉覆盖编辑基线)。
    pub sub_save_first: &'static str,
    /// 兜底槽 / Kiro 订阅上按 `o` 改思考档位的拒绝提示 (两种原因各一条)。
    pub sub_effort_na_fallback: &'static str,
    pub sub_effort_na_kiro: &'static str,
    /// 主槽 (非兜底) 选了空白自定义值时的拒绝提示。
    pub sub_model_required: &'static str,
    /// 草稿对应的订阅从 `Store` 消失 (被别处删除) 时的提示。
    pub sub_gone: &'static str,
    /// I1/M5 (fix round final): 这条订阅 (虚拟模型同理) 正有一次保存在飞行中时, 拒绝任何会修改
    /// 草稿的按键 (含再按一次 `s`) 时的提示——避免飞行中的编辑被落地的保存结果悄悄冲掉 (D1 的
    /// 姊妹问题: D1 保证了草稿不会被冲掉, 但没有在编辑发生的那一刻就告诉用户"现在编辑不安全")。
    pub saving_in_progress: &'static str,

    // ---------- Task 6: 虚拟模型页 ----------
    pub vm_title: &'static str,
    /// `RoutingMode` 的四个短名 (列表列用), 通过 [`Strings::vm_mode_short`] 取。
    pub vm_mode_seq: &'static str,
    pub vm_mode_rr: &'static str,
    pub vm_mode_sticky: &'static str,
    pub vm_mode_unknown: &'static str,
    /// 四个全名 (带线上名字, 成员面板 `title_bottom` 用), 通过 [`Strings::vm_mode_full`] 取。
    pub vm_mode_full_seq: &'static str,
    pub vm_mode_full_rr: &'static str,
    pub vm_mode_full_sticky: &'static str,
    pub vm_mode_full_unknown: &'static str,
    /// 成员面板 `title_bottom`: `{mode_full} · {n} 个订阅`。
    pub vm_members_summary: fn(mode_full: &str, n: usize) -> String,
    pub vm_empty: &'static str,
    /// 订阅 id 在 `Store` 里找不到 (被别处删除) 时, 名字退化成 id 前 8 位 + 这个后缀。
    pub vm_missing: &'static str,
    /// 仅 `model-fallback`: 订阅是翻译类 (`auth_type != "api_key"`) 且没配兜底槽时, 行尾追加这个
    /// 警告 (对应后端 pipeline 的统一跳过守卫)。
    pub vm_will_skip: &'static str,
    /// `a` 键在没有可加入的订阅时的提示 (就地回答, 不开弹窗)。
    pub vm_nothing_to_add: &'static str,
    /// V2 (fix round P3b): 草稿里还有 `Store` 找不到的 id (「已删除」的订阅) 时, `s` 拒绝保存的
    /// 提示——不能把这种裸 id 发给后端, 后端会用一句英文报错拒绝, 对用户毫无意义。
    pub vm_remove_ghosts_first: &'static str,
    /// `a` 弹窗的标题, 参数是虚拟模型名。
    pub vm_pick_add_title: fn(vm: &str) -> String,
    /// 草稿的调度模式是 `RoutingMode::Unknown` (后端某天加的新模式, 这版 TUI 不认得) 时, `s`
    /// 拒绝保存的提示——`Unknown.as_wire()` 会静默降级成 `"sequential"`, 不能让用户在不知情的
    /// 情况下把它发回后端。
    pub vm_unknown_mode: &'static str,
    /// I4 (fix round final): 订阅列表还没加载完 (或一直加载失败) 时, `a`/`x`/`J`/`K`/`s` 的拒绝
    /// 提示——这段时间不能断定成员列表里找不到的 id 到底是"已删除"还是"只是还没拉到", 所以不
    /// 显示 `vm_missing`、也不允许这几个会依赖订阅列表的编辑操作。
    pub vm_subs_not_loaded: &'static str,
    pub vm_help_rows: &'static [(&'static str, &'static str)],

    // ---------- Task 7: 实时路由页 ----------
    pub live_spark_title: &'static str,
    pub live_spark_total: fn(n: u64) -> String,
    pub live_title: &'static str,
    /// 还没有任何路由事件时, 表格区域居中显示的一行提示。
    pub live_empty: &'static str,
    /// 有过滤条件、但没有任何事件符合时的提示——不能用 `live_empty` 那句"还没有事件", 那是假的,
    /// 只是被过滤掉了 (与日志页的 `lg_empty_filtered` 同一条先例)。
    pub live_empty_filtered: &'static str,
    /// 未暂停且跟随最新时的底栏文案。
    pub live_following: &'static str,
    /// 暂停时的底栏文案, 参数是暂停后新增、且符合过滤的尝试数。
    pub live_paused: fn(n: usize) -> String,
    /// 可见尝试数 (不含断线分隔行)。
    pub live_count: fn(n: usize) -> String,
    pub live_gap: &'static str,
    pub live_interrupted: &'static str,
    pub live_filter_title: &'static str,
    pub live_filter_all: &'static str,
    /// 当前过滤的摘要, 参数是过滤条件的显示名。日志页 (Task 8) 共用。
    pub filter_summary: fn(what: &str) -> String,
    pub filter_dim_vm: &'static str,
    pub filter_dim_sub: &'static str,
    /// 空格键的显示名 (实时路由页 `hints()` 用它当键名, 不能像 "↑↓"/"m" 那样直接写死符号——
    /// "空格" 本身是中文, 必须走 `Strings`)。
    pub key_space: &'static str,
    pub key_pause: &'static str,
    pub key_resume: &'static str,
    pub key_latest: &'static str,
    pub key_filter: &'static str,
    pub key_clear_filter: &'static str,
    pub live_help_rows: &'static [(&'static str, &'static str)],

    // ---------- Task 8: 请求日志页 ----------
    pub lg_title: &'static str,
    pub lg_col_time: &'static str,
    pub lg_col_status: &'static str,
    pub lg_col_vm: &'static str,
    pub lg_col_sub: &'static str,
    pub lg_col_model: &'static str,
    pub lg_col_latency: &'static str,
    pub lg_col_tokens: &'static str,
    pub lg_col_client: &'static str,
    pub lg_page: fn(page: u32, pages: u32, total: i64) -> String,
    pub lg_empty: &'static str,
    pub lg_empty_filtered: &'static str,
    pub lg_filter_title: &'static str,
    pub lg_filter_clear: &'static str,
    pub filter_dim_status: &'static str,
    /// 过滤弹窗里当前生效的那个条目, hint 追加的后缀 (`" · " + filter_active`)。
    pub filter_active: &'static str,
    pub lg_status_success: &'static str,
    pub lg_status_error: &'static str,
    pub lg_status_timeout: &'static str,
    pub lg_status_unknown: &'static str,
    /// 日志页 `n`/`p` 翻页的 hint 键名。
    pub key_page: &'static str,
    /// 实时路由页 `⏎` 跳到日志页的 hint 键名。
    pub key_logs: &'static str,
    pub lg_help_rows: &'static [(&'static str, &'static str)],
    pub lg_d_title: &'static str,
    pub lg_d_basic: &'static str,
    pub lg_d_effort: &'static str,
    pub lg_d_tools: &'static str,
    pub lg_d_error: &'static str,
    pub lg_d_body: &'static str,
    pub lg_d_time: &'static str,
    pub lg_d_id: &'static str,
    pub lg_d_status: &'static str,
    pub lg_d_vm: &'static str,
    pub lg_d_real_model: &'static str,
    pub lg_d_resp_model: &'static str,
    pub lg_d_sub: &'static str,
    pub lg_d_provider: &'static str,
    pub lg_d_latency: &'static str,
    pub lg_d_streaming: &'static str,
    pub lg_d_tokens: &'static str,
    pub lg_d_client: &'static str,
    pub lg_d_ip: &'static str,
    pub lg_d_ua: &'static str,
    pub lg_d_entry: &'static str,
    pub lg_d_http_version: &'static str,
    pub lg_d_status_value: fn(status: &str, http: Option<i64>) -> String,
    pub lg_d_tokens_value: fn(i: &str, o: &str, cw: &str, cr: &str) -> String,
    pub lg_yes: &'static str,
    pub lg_no: &'static str,
    pub lg_d_effort_client: &'static str,
    pub lg_d_effort_effective: &'static str,
    pub lg_d_effort_upstream: &'static str,
    pub lg_d_effort_upstream_none: &'static str,
    pub lg_effort_source: fn(src: &str) -> String,
    pub lg_d_stop_reason: &'static str,
    pub lg_d_tools_offered: &'static str,
    pub lg_d_tool_results: &'static str,
    pub lg_d_tool_uses: &'static str,
    pub lg_d_tool_names: &'static str,
    pub lg_d_truncated: &'static str,
    pub lg_d_unnamed: &'static str,

    // ---------- P5 Task 2: 新建订阅向导 (骨架; Task 4 起补表单文案) ----------
    pub wiz_title: &'static str,
    pub wiz_loading_providers: &'static str,
    pub wiz_load_failed: fn(reason: &str) -> String,

    // ---------- P5 Task 4: 向导第一步 (内置厂商: 选厂商 / 选接入点 / API Key / 备注名) ----------
    /// 步骤条的两段文案 (含序号), `form::FormView::steps` 直接用。
    pub wiz_steps: [&'static str; 2],
    pub wiz_f_provider: &'static str,
    pub wiz_f_endpoint: &'static str,
    pub wiz_f_api_key: &'static str,
    pub wiz_f_display_name: &'static str,
    pub wiz_btn_next: &'static str,
    pub wiz_pick_provider: &'static str,
    pub wiz_pick_endpoint: &'static str,
    /// 厂商还没选时按 `⏎` 打开接入点 picker 的拒绝提示。
    pub wiz_pick_provider_first: &'static str,
    /// OAuth 类厂商 (`chatgpt_oauth` / `kiro_oauth`, TUI 不做设备码流程) 选中时的提示; 同时也
    /// 追加在厂商 picker 里这一项的 label 后面 (`label · wiz_desktop_only`)。
    pub wiz_desktop_only: &'static str,
    /// 自定义厂商条目选中后的占位提示 (Task 6 会把这一条换成真正的 `Stage::Custom`, 并删掉这个
    /// 字段——见 `wizard/mod.rs::apply_provider_choice`)。
    pub wiz_custom_todo: &'static str,
    /// 5 个自定义协议条目的 label, 顺序与 `CustomProtocol::ALL` 一致。
    pub wiz_custom_labels: [&'static str; 5],
    pub wiz_err_api_key: &'static str,
    pub wiz_err_display_name: &'static str,
    pub wiz_err_provider: &'static str,
    pub wiz_err_endpoint: &'static str,
    /// `Stage::Creating` 时表单的按钮文案 (busy 态)。
    pub wiz_creating: &'static str,
    /// `create_subscription` 成功后的 notice 文案 (Task 4 临时关掉向导时用; Task 5 换成"接着拉
    /// 模型列表"之后这条文案挪到保存槽位成功那一刻, 但字段本身继续用)。
    pub wiz_created: fn(name: &str) -> String,
    /// `create_subscription` 失败时挂在表单顶部的说明行 (`FormRow::Note`)。
    pub wiz_create_failed: fn(reason: &str) -> String,
    /// 向导表单底栏: `↑↓` 在字段间移动。
    pub key_field: &'static str,
    /// 向导表单底栏 / 选择行右端 hint: `⏎` 打开选择弹窗。
    pub key_pick: &'static str,
    /// 向导表单底栏: 焦点在文本行 (`ApiKey`/`DisplayName`) 时 `⏎` 的说明 (移到下一项, 不是提交)。
    /// 评审 I2: 底栏之前写死三条固定提示, 文本行上 `⏎` 实际是"下一项"却显示成"选择", 需要单独
    /// 一个字段区分开。
    pub key_next_field: &'static str,
    /// 向导表单底栏 / API Key 行右端 hint: `Ctrl+R` 切换明文/掩码。
    pub key_reveal: &'static str,
    /// 表单内容超过可视高度、被截断时最后一行的提示。
    pub form_more: &'static str,
}

impl Strings {
    pub fn state(&self, state: SubscriptionState) -> &'static str {
        match state {
            SubscriptionState::Healthy => self.st_healthy,
            SubscriptionState::RateLimited => self.st_rate_limited,
            SubscriptionState::QuotaExhausted => self.st_quota_exhausted,
            SubscriptionState::TransientError => self.st_transient_error,
            SubscriptionState::AuthFailed => self.st_auth_failed,
            SubscriptionState::Disabled => self.st_disabled,
            SubscriptionState::Unknown => self.st_unknown,
        }
    }

    pub fn quota_period(&self, period: QuotaPeriod) -> &'static str {
        match period {
            QuotaPeriod::Daily => self.q_daily,
            QuotaPeriod::Weekly => self.q_weekly,
            QuotaPeriod::Monthly => self.q_monthly,
            QuotaPeriod::Total | QuotaPeriod::Unknown => self.q_total,
        }
    }

    pub fn vm_mode_short(&self, mode: RoutingMode) -> &'static str {
        match mode {
            RoutingMode::Sequential => self.vm_mode_seq,
            RoutingMode::RoundRobin => self.vm_mode_rr,
            RoutingMode::Sticky => self.vm_mode_sticky,
            RoutingMode::Unknown => self.vm_mode_unknown,
        }
    }

    pub fn vm_mode_full(&self, mode: RoutingMode) -> &'static str {
        match mode {
            RoutingMode::Sequential => self.vm_mode_full_seq,
            RoutingMode::RoundRobin => self.vm_mode_full_rr,
            RoutingMode::Sticky => self.vm_mode_full_sticky,
            RoutingMode::Unknown => self.vm_mode_full_unknown,
        }
    }
}

pub const ZH: Strings = Strings {
    tabs: ["总览", "订阅", "虚拟模型", "实时路由", "日志"],
    conn_connecting: "连接中",
    conn_connected: "已连接",
    conn_reconnecting: "重连中",

    key_switch_tab: "切页",
    key_refresh: "刷新",
    key_help: "帮助",
    key_quit: "退出",
    key_close: "关闭",
    key_select: "选择",
    key_detail: "详情",
    key_back: "返回",
    key_toggle: "启停",
    key_test: "测试",
    key_models: "模型",
    key_balance: "余额",
    key_edit_model: "改模型",
    key_edit_effort: "改档位",
    key_save: "保存",
    key_discard: "放弃",
    key_move: "移动",
    key_add: "加入",
    key_remove: "移除",
    key_mode: "模式",
    key_members: "成员",
    key_new: "新建",
    key_cancel: "取消",

    help_title: "键位",
    help_rows: &[
        ("1-5", "直达对应页面"),
        ("Tab / Shift+Tab", "下一页 / 上一页"),
        ("r", "刷新当前页面"),
        ("?", "打开 / 关闭本帮助"),
        ("Esc", "关闭弹窗"),
        ("q / Ctrl+C", "退出"),
    ],

    confirm_title: "确认",
    confirm_keys: "y 是   n 否",
    confirm_discard: "有未保存的修改,确定放弃吗?",

    picker_use_typed: |text| format!("使用「{text}」"),
    picker_empty: "没有匹配项",
    picker_type_to_enter: "输入后按 ⏎ 使用该文本",
    picker_keys: "⏎ 选择   Esc 取消",
    pick_model_title: |slot| format!("选择 {slot} 的模型"),
    pick_effort_title: |slot| format!("选择 {slot} 的思考档位"),
    pick_clear_fallback: "(清空兜底槽)",

    detail_keys: "↑↓ 滚动   Esc 关闭",

    too_small: "请放大终端窗口（至少 80×24）",
    loading: "加载中",
    version_mismatch: |tui, app| format!("终端界面版本 {tui} 与 app 版本 {app} 不一致，请在桌面 app 的设置页重新添加到 PATH"),

    ov_today: "今日",
    ov_requests: "请求",
    ov_success_rate: "成功率",
    ov_tokens: "Token",
    ov_hourly: "每小时请求",
    ov_health: "订阅健康度",
    ov_auth_on: "鉴权 开启",
    ov_auth_off: "鉴权 关闭",
    ov_listen_all: "监听 0.0.0.0",
    ov_subs_summary: |total, ok| format!("{total} 个订阅 · {ok} 个可调度"),
    ov_no_subs: "还没有订阅，请先在桌面 app 里添加",
    ov_more_rows: |n| format!("… 还有 {n} 个"),

    st_healthy: "正常",
    st_rate_limited: "限流",
    st_quota_exhausted: "配额耗尽",
    st_transient_error: "临时错误",
    st_auth_failed: "凭证失效",
    st_disabled: "已禁用",
    st_unknown: "未知",
    st_quota_reached: "已达限额",

    q_daily: "日限额",
    q_weekly: "周限额",
    q_monthly: "月限额",
    q_total: "总限额",

    toast_reconnected: "已重新连接",
    toast_load_failed: |reason| format!("加载失败：{reason}"),
    toast_offline: "未连接,暂时无法操作",
    toast_enabled: |name| format!("已启用 {name}"),
    toast_disabled: |name| format!("已停用 {name}"),
    toast_test_ok: |name, model| match model {
        Some(model) => format!("{name}：连接正常 ({model})"),
        None => format!("{name}：连接正常"),
    },
    toast_test_failed: |name, message| format!("{name}：{message}"),
    toast_models_ok: |name, n| format!("{name}：获取到 {n} 个模型"),
    toast_models_manual: |name, reason| format!("{name}：无法自动获取模型 ({reason})"),
    toast_balance_ok: |name| format!("{name}：余额已刷新"),
    toast_balance_failed: |name, reason| format!("{name}：余额查询失败 ({reason})"),
    toast_mutation_failed: |name, message| format!("{name}：操作失败 ({message})"),
    toast_slots_saved: |name| format!("{name}：槽位已保存"),
    toast_vm_saved: |vm| format!("{vm}：已保存"),

    sub_title: |n| format!("订阅 ({n})"),
    sub_col_name: "备注名",
    sub_col_provider: "厂商",
    sub_col_sonnet: "sonnet",
    sub_col_state: "状态",
    sub_f_state: "状态",
    sub_f_provider: "厂商",
    sub_f_endpoint: "端点",
    sub_f_slots: "槽位",
    sub_f_quota: "限额",
    sub_f_balance: "余额",
    sub_f_models: "模型",
    sub_f_referenced: "被引用",
    sub_f_last_error: "最近错误",
    sub_f_last_action: "上次操作",
    sub_slot_fallback: "兜底",
    sub_slot_unset: "(未配置)",
    sub_effort_auto: "auto",
    sub_balance_unsupported: "该厂商不支持余额查询",
    sub_balance_never: "还没查过,按 b 刷新",
    sub_balance_unavailable: "账户不可用 (可能欠费或被封)",
    sub_models_cached: |n| format!("已缓存 {n} 个"),
    sub_models_never: "还没获取过,按 m 刷新",
    sub_unreferenced: "没有被任何虚拟模型引用",
    sub_help_rows: &[
        ("↑↓ / j k", "上一条 / 下一条"),
        ("g / G", "第一条 / 最后一条"),
        ("PgUp / PgDn", "翻页"),
        ("⏎ / → / l", "进入详情"),
        ("Esc / ← / h", "退出详情"),
        ("⏎ (详情内)", "改当前槽位的模型"),
        ("o", "改当前槽位的思考档位"),
        ("s", "保存槽位修改"),
        ("e", "启用 / 停用"),
        ("t", "测试连接"),
        ("m", "刷新模型列表"),
        ("b", "刷新余额"),
    ],
    sub_busy_toggling: "正在切换…",
    sub_busy_testing: "正在测试连接…",
    sub_busy_models: "正在获取模型…",
    sub_busy_balance: "正在查询余额…",
    sub_busy_saving: "正在保存…",
    sub_slot_modified: "已修改",
    sub_save_first: "先按 s 保存或 Esc 放弃当前修改",
    sub_effort_na_fallback: "兜底槽没有思考档位",
    sub_effort_na_kiro: "Kiro 不支持思考档位",
    sub_model_required: "模型不能为空",
    sub_gone: "这条订阅已不存在",
    saving_in_progress: "正在保存,请稍候",

    vm_title: "虚拟模型",
    vm_mode_seq: "顺序",
    vm_mode_rr: "轮询",
    vm_mode_sticky: "会话",
    vm_mode_unknown: "未知",
    vm_mode_full_seq: "顺序 (sequential)",
    vm_mode_full_rr: "轮询 (round_robin)",
    vm_mode_full_sticky: "会话亲和 (sticky)",
    vm_mode_full_unknown: "未知",
    vm_members_summary: |mode, n| format!("{mode} · {n} 个订阅"),
    vm_empty: "还没有绑定订阅,按 a 加入",
    vm_missing: "(已删除)",
    vm_will_skip: "将被跳过",
    vm_nothing_to_add: "所有订阅都已在列表里",
    vm_remove_ghosts_first: "列表里有已删除的订阅,请先按 x 移除",
    vm_pick_add_title: |vm| format!("给 {vm} 加入订阅"),
    vm_unknown_mode: "这个调度模式当前版本不认识,请在桌面 app 里修改",
    vm_subs_not_loaded: "订阅列表还没加载完,请稍候",
    vm_help_rows: &[
        ("↑↓ / j k", "上一项 / 下一项"),
        ("⏎ / → / l", "进入订阅列表"),
        ("Esc / ← / h", "返回虚拟模型列表"),
        ("J / K", "下移 / 上移当前订阅"),
        ("a", "加入订阅"),
        ("x", "移除当前订阅"),
        ("m", "切换调度模式"),
        ("s", "保存修改"),
    ],

    live_spark_title: "最近 60 秒",
    live_spark_total: |n| format!("{n} 次"),
    live_title: "实时路由",
    live_empty: "还没有路由事件，Claude Code 发出请求后会出现在这里",
    live_empty_filtered: "没有符合过滤条件的事件 · Esc 清除过滤",
    live_following: "跟随最新",
    live_paused: |n| format!("已暂停 · 新增 {n} 条"),
    live_count: |n| format!("共 {n} 条"),
    live_gap: "连接中断，期间的事件未收到",
    live_interrupted: "中断",
    live_filter_title: "按虚拟模型或订阅过滤",
    live_filter_all: "全部 (清除过滤)",
    filter_summary: |what| format!("过滤 {what}"),
    filter_dim_vm: "虚拟模型",
    filter_dim_sub: "订阅",
    key_space: "空格",
    key_pause: "暂停",
    key_resume: "继续",
    key_latest: "最新",
    key_filter: "过滤",
    key_clear_filter: "清除过滤",
    live_help_rows: &[
        ("空格", "暂停 / 继续 (暂停时新事件先缓冲)"),
        ("↑↓ / j k", "选择一行 (离开「跟随最新」)"),
        ("PgUp / PgDn", "翻页"),
        ("g / G", "最早一行 / 回到最新并继续"),
        ("⏎", "查看该订阅的请求日志"),
        ("/", "按虚拟模型或订阅过滤"),
        ("Esc", "清除过滤 / 回到最新"),
        ("耗时", "流式到上游开始响应, 非流式到响应结束"),
        ("并发", "同一虚拟模型 + 订阅的并发尝试按先后配对"),
    ],

    lg_title: "请求日志",
    lg_col_time: "时间",
    lg_col_status: "状态",
    lg_col_vm: "虚拟模型",
    lg_col_sub: "订阅",
    lg_col_model: "模型",
    lg_col_latency: "耗时",
    lg_col_tokens: "Token 入/出",
    lg_col_client: "客户端",
    lg_page: |page, pages, total| format!("第 {page}/{pages} 页 · 共 {total} 条"),
    lg_empty: "还没有请求记录",
    lg_empty_filtered: "没有符合条件的请求 · Esc 清除过滤",
    lg_filter_title: "过滤请求日志",
    lg_filter_clear: "清除全部过滤",
    filter_dim_status: "状态",
    filter_active: "当前 · 再选一次取消",
    lg_status_success: "成功",
    lg_status_error: "失败",
    lg_status_timeout: "超时",
    lg_status_unknown: "未知",
    key_page: "翻页",
    key_logs: "日志",
    lg_help_rows: &[
        ("↑↓ / j k", "上一条 / 下一条"),
        ("g / G", "本页第一条 / 最后一条"),
        ("PgUp / PgDn", "本页内翻屏"),
        ("n / p", "下一页 / 上一页"),
        ("⏎", "查看详情"),
        ("/", "按订阅 / 虚拟模型 / 状态过滤"),
        ("Esc", "清除过滤"),
        ("自动刷新", "只在第 1 页, 每 5 秒"),
    ],
    lg_d_title: "请求详情",
    lg_d_basic: "基本信息",
    lg_d_effort: "思考强度",
    lg_d_tools: "工具调用",
    lg_d_error: "错误信息",
    lg_d_body: "上游响应",
    lg_d_time: "时间",
    lg_d_id: "请求 ID",
    lg_d_status: "状态",
    lg_d_vm: "虚拟模型",
    lg_d_real_model: "真实模型",
    lg_d_resp_model: "响应模型",
    lg_d_sub: "订阅",
    lg_d_provider: "厂商 / 端点",
    lg_d_latency: "耗时",
    lg_d_streaming: "流式",
    lg_d_tokens: "Token",
    lg_d_client: "客户端",
    lg_d_ip: "客户端 IP",
    lg_d_ua: "User-Agent",
    lg_d_entry: "入口",
    lg_d_http_version: "HTTP 版本",
    lg_d_status_value: |status, http| match http {
        Some(code) => format!("{status} · HTTP {code}"),
        None => status.to_string(),
    },
    lg_d_tokens_value: |i, o, cw, cr| format!("输入 {i} · 输出 {o} · 缓存写 {cw} · 缓存读 {cr}"),
    lg_yes: "是",
    lg_no: "否",
    lg_d_effort_client: "客户端请求",
    lg_d_effort_effective: "实际生效",
    lg_d_effort_upstream: "上游回显",
    lg_d_effort_upstream_none: "上游未回显",
    lg_effort_source: |src| match src {
        "slot" => "订阅槽位强制".to_string(),
        "client" => "客户端透传".to_string(),
        "yaml" => "provider 默认".to_string(),
        other => other.to_string(),
    },
    lg_d_stop_reason: "结束原因",
    lg_d_tools_offered: "声明工具数",
    lg_d_tool_results: "回传结果数",
    lg_d_tool_uses: "本次调用",
    lg_d_tool_names: "工具名",
    lg_d_truncated: "(已截断)",
    lg_d_unnamed: "(未命名)",

    wiz_title: "新建订阅",
    wiz_loading_providers: "正在获取厂商列表…",
    wiz_load_failed: |reason| format!("获取厂商列表失败: {reason}"),

    wiz_steps: ["① 基本信息", "② 绑定模型"],
    wiz_f_provider: "厂商",
    wiz_f_endpoint: "接入点",
    wiz_f_api_key: "API Key",
    wiz_f_display_name: "备注名",
    wiz_btn_next: "下一步",
    wiz_pick_provider: "选择厂商",
    wiz_pick_endpoint: "选择接入点",
    wiz_pick_provider_first: "请先选择厂商",
    wiz_desktop_only: "请在桌面端添加",
    wiz_custom_todo: "自定义厂商下一步做",
    wiz_custom_labels: [
        "自定义 · Anthropic 兼容",
        "自定义 · Gemini",
        "自定义 · OpenAI Responses",
        "自定义 · OpenAI Chat Completions",
        "自定义 · Gemini Interactions",
    ],
    wiz_err_api_key: "API Key 不能为空",
    wiz_err_display_name: "备注名不能为空",
    wiz_err_provider: "请选择厂商",
    wiz_err_endpoint: "请选择接入点",
    wiz_creating: "正在创建订阅…",
    wiz_created: |name| format!("已创建「{name}」"),
    wiz_create_failed: |reason| format!("创建失败: {reason}"),
    key_field: "字段",
    key_pick: "选择",
    key_next_field: "下一项",
    key_reveal: "显示 / 隐藏",
    form_more: "… 内容放不下",
};

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        // P6 在这里接上 EN / JA 两个 const; 在那之前三种语言都显示中文。
        Lang::Zh | Lang::En | Lang::Ja => &ZH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn explicit_preference_wins_over_system() {
        assert_eq!(Lang::resolve("ja", env(&[("LANG", "zh_CN.UTF-8")])), Lang::Ja);
        assert_eq!(Lang::resolve("en", env(&[("LANG", "zh_CN.UTF-8")])), Lang::En);
    }

    #[test]
    fn system_follows_the_same_prefix_rule_as_the_desktop_app() {
        assert_eq!(Lang::resolve("system", env(&[("LANG", "zh_CN.UTF-8")])), Lang::Zh);
        assert_eq!(Lang::resolve("system", env(&[("LANG", "zh-Hant-TW")])), Lang::Zh);
        assert_eq!(Lang::resolve("system", env(&[("LANG", "ja_JP.UTF-8")])), Lang::Ja);
        assert_eq!(Lang::resolve("system", env(&[("LANG", "de_DE.UTF-8")])), Lang::En);
        assert_eq!(Lang::resolve("system", env(&[])), Lang::En);
    }

    #[test]
    fn lc_all_beats_lang_and_empty_values_are_skipped() {
        assert_eq!(Lang::resolve("system", env(&[("LC_ALL", "ja_JP"), ("LANG", "zh_CN")])), Lang::Ja);
        assert_eq!(Lang::resolve("system", env(&[("LC_ALL", ""), ("LANG", "zh_CN")])), Lang::Zh);
    }

    /// 标签栏一行放得下: 每个标签渲染成 ` N 名称 `, 之间一个分隔符, 总宽 ≤ 76 (80 列减边框与内距)。
    #[test]
    fn tab_bar_fits_in_80_columns() {
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            let s = strings(lang);
            let total: usize = s.tabs.iter().map(|t| t.width() + 4).sum::<usize>() + (s.tabs.len() - 1);
            assert!(total <= 76, "{lang:?}: 标签栏宽 {total}");
        }
    }
}
