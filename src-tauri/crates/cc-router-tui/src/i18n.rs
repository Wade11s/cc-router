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
    /// `system_tag` 是 runtime.json 里桌面端下发的原始系统语言标签 (`tauri_plugin_os::locale()`
    /// 的原样值), 只有连上桌面端才拿得到。
    ///
    /// 解析顺序: 显式 `zh`/`en`/`ja` 优先命中即返回; `"system"` / 空串 / **任何未知值**都跟随系统——
    /// 与 `tray.rs::TrayLocale::resolve` 一致, 未知偏好不是 en, 而是像 `"system"` 一样继续探测。
    /// 系统探测顺序: `system_tag` (非空) → 环境变量 `LC_ALL` → `LC_MESSAGES` → `LANG` → en。
    /// 映射规则 (大小写不敏感) 与桌面端 `src/i18n/index.tsx::detectSystemLocale` / `tray.rs` 一致:
    /// `zh*` → zh, `ja*` → ja, 其余 → en。
    pub fn resolve(preferred: &str, system_tag: Option<&str>, env: impl Fn(&str) -> Option<String>) -> Self {
        let tag = match preferred {
            "zh" => return Self::Zh,
            "en" => return Self::En,
            "ja" => return Self::Ja,
            // "system"、空串、以及任何未知值都走系统探测。
            _ => system_tag.filter(|t| !t.is_empty()).map(str::to_string).unwrap_or_else(|| {
                ["LC_ALL", "LC_MESSAGES", "LANG"]
                    .iter()
                    .find_map(|k| env(k).filter(|v| !v.is_empty()))
                    .unwrap_or_default()
            }),
        };
        let lower = tag.to_ascii_lowercase();
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
    /// 订阅详情里改当前槽位的模型 (⏎) / 思考档位 (o) / 保存草稿 (s)。
    pub key_edit_model: &'static str,
    pub key_edit_effort: &'static str,
    pub key_save: &'static str,
    /// 脏页面上 `Esc` 的 hint 文案 ("放弃"), 与 `key_back` ("返回", 不脏时
    /// 用) 区分开——同一个键在脏/不脏两种状态下的语义不同, 底栏提示也该跟着换。
    pub key_discard: &'static str,
    /// 虚拟模型页——成员列表里 `J`/`K` 重排序、`a` 加入、`x` 移除。
    pub key_move: &'static str,
    pub key_add: &'static str,
    pub key_remove: &'static str,
    pub key_mode: &'static str,
    /// 虚拟模型页 Models 焦点下 `⏎` 的 hint 文案 ("成员")——比 `key_detail` ("详情") 更准确地
    /// 描述这个键的作用 (进入这个虚拟模型的订阅成员列表)。
    pub key_members: &'static str,
    /// 订阅页 `n` 的 hint 文案 ("新建")。
    pub key_new: &'static str,
    /// 订阅页 `d` 的 hint 文案 ("删除")。
    pub key_delete: &'static str,
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
    /// 没有匹配; "零匹配 + 空输入 + 允许自定义" 这种情况用 [`Strings::picker_type_to_enter`])。
    pub picker_empty: &'static str,
    /// `allow_custom` 且输入框为空 (trim 之后) 且没有任何候选项时的占位文案——引导用户
    /// 打字后回车直接用输入的文本, 与 `picker_empty` ("没有匹配项", 用于确实存在候选但过滤不出
    /// 结果、或者压根不允许自定义的场景) 区分开。
    pub picker_type_to_enter: &'static str,
    /// 过滤选择弹窗底部的键位提示。
    pub picker_keys: &'static str,
    /// 改模型 / 改思考档位两个 picker 的标题, 参数是槽位显示名 (四个主槽的英文原名, 或
    /// [`Strings::sub_slot_fallback`])。
    pub pick_model_title: fn(slot: &str) -> String,
    pub pick_effort_title: fn(slot: &str) -> String,
    /// 兜底槽模型 picker 里置顶的「清空」选项 (对应 `id: ""`)。
    pub pick_clear_fallback: &'static str,

    /// 只读详情弹窗 (`Popup::Detail`, 请求日志详情用) 底部的键位提示。
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
    /// 同一条订阅 (或同一个虚拟模型) 上一个就地操作还没回来时又发起一个——比如测试连接在飞时确认
    /// 删除。
    pub toast_busy: fn(name: &str) -> String,
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
    /// `Mutation::UpdateSlots` (订阅详情页发起) 的进行中文案; 与四个既有就地操作用同一套
    /// 「状态行后追加 busy 文案」机制。
    pub sub_busy_saving: &'static str,
    /// `Mutation::Delete` 的进行中文案, 同一套机制。
    pub sub_busy_deleting: &'static str,
    /// 草稿里跟 `Store` 当前值不同的槽位行末尾追加的 muted 提示。
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
    /// 这条订阅 (虚拟模型同理) 正有一次保存在飞行中时, 拒绝任何会修改草稿的按键 (含再按一次 `s`)
    /// 时的提示——避免飞行中的编辑被落地的保存结果悄悄冲掉, 并在编辑发生的那一刻就告诉用户。
    pub saving_in_progress: &'static str,

    // ---------- 删除订阅 ----------
    /// 删除确认弹窗的正文, 参数是订阅备注名; `referenced_by` 为空时单行只有这一句。
    pub sub_confirm_delete: fn(name: &str) -> String,
    /// `referenced_by` 非空时追加的第二行, 参数是引用它的虚拟模型个数。
    pub sub_delete_refs: fn(n: usize) -> String,
    /// 引用方超过 4 个时, 第三行 (虚拟模型名列表) 只列前 4 个, 再接这句, 参数是剩余个数
    /// (总数减 4, 不是总数本身)。
    pub sub_delete_refs_more: fn(n: usize) -> String,
    /// 删除确认弹窗第三行 (虚拟模型名列表) 的分隔符——与详情面板「被引用」字段的 `", "` 各自
    /// 独立配置。
    pub list_sep: &'static str,
    pub toast_deleted: fn(name: &str) -> String,

    // ---------- 虚拟模型页 ----------
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
    /// 草稿里还有 `Store` 找不到的 id (「已删除」的订阅) 时, `s` 拒绝保存的
    /// 提示——不能把这种裸 id 发给后端, 后端会用一句英文报错拒绝, 对用户毫无意义。
    pub vm_remove_ghosts_first: &'static str,
    /// `a` 弹窗的标题, 参数是虚拟模型名。
    pub vm_pick_add_title: fn(vm: &str) -> String,
    /// 草稿的调度模式是 `RoutingMode::Unknown` (后端某天加的新模式, 这版 TUI 不认得) 时, `s`
    /// 拒绝保存的提示——`Unknown.as_wire()` 会静默降级成 `"sequential"`, 不能让用户在不知情的
    /// 情况下把它发回后端。
    pub vm_unknown_mode: &'static str,
    /// 订阅列表还没加载完 (或一直加载失败) 时, `a`/`x`/`J`/`K`/`s` 的拒绝
    /// 提示——这段时间不能断定成员列表里找不到的 id 到底是"已删除"还是"只是还没拉到", 所以不
    /// 显示 `vm_missing`、也不允许这几个会依赖订阅列表的编辑操作。
    pub vm_subs_not_loaded: &'static str,
    pub vm_help_rows: &'static [(&'static str, &'static str)],

    // ---------- 实时路由页 ----------
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
    /// 当前过滤的摘要, 参数是过滤条件的显示名。日志页共用。
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

    // ---------- 请求日志页 ----------
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
    /// 「实际生效」值后面追加的来源说明, 参数是 `lg_effort_source` 已经格式化好的那句话
    /// (比如「订阅槽位强制」)——只负责套一层括号, 括号本身也是中文全角字符, 不能写死在调用点。
    pub lg_d_effort_source_suffix: fn(label: &str) -> String,
    pub lg_d_stop_reason: &'static str,
    pub lg_d_tools_offered: &'static str,
    pub lg_d_tool_results: &'static str,
    pub lg_d_tool_uses: &'static str,
    pub lg_d_tool_names: &'static str,
    pub lg_d_truncated: &'static str,
    pub lg_d_unnamed: &'static str,

    // ---------- 新建订阅向导 ----------
    pub wiz_title: &'static str,
    pub wiz_loading_providers: &'static str,
    pub wiz_load_failed: fn(reason: &str) -> String,

    // ---------- 向导第一步 (内置厂商: 选厂商 / 选接入点 / API Key / 备注名) ----------
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
    /// 5 个自定义协议条目的 label, 顺序与 `CustomProtocol::ALL` 一致——既用于厂商 picker 里的
    /// `custom:<protocol>` 条目, 也用于自定义表单自己的协议 picker (`wiz_pick_protocol`)。
    pub wiz_custom_labels: [&'static str; 5],
    pub wiz_err_api_key: &'static str,
    pub wiz_err_display_name: &'static str,
    pub wiz_err_provider: &'static str,
    pub wiz_err_endpoint: &'static str,
    /// 创建请求在飞时 (两条路径) 按钮的文案 (busy 态)。
    pub wiz_creating: &'static str,
    /// 向导完成时的 toast: 内置路径在保存槽位成功时弹, 自定义路径在创建成功时弹。
    pub wiz_created: fn(name: &str) -> String,
    /// `create_subscription` 失败时挂在表单顶部的说明行 (`FormRow::Note`)。
    pub wiz_create_failed: fn(reason: &str) -> String,

    // ---------- 向导第二步 (绑定模型) ----------
    /// 订阅已经建好、在等候选模型列表时第一步按钮的文案 (busy 态)。
    pub wiz_loading_models: &'static str,
    /// `update_subscription` (只带 `model_slots` 的 patch) 失败时挂在表单顶部的说明行, 与
    /// `wiz_models_manual` 共用同一个位置, 谁最后发生显示谁。
    pub wiz_save_failed: fn(reason: &str) -> String,
    /// 拉模型列表 / 探测模型返回 `ManualFallback` (或整个请求失败) 时挂在表单顶部的说明行, 参数
    /// 是后端给的原因。
    pub wiz_models_manual: fn(reason: &str) -> String,
    /// 槽位行 `⏎` 打开的模型 picker 标题, 参数是槽位显示名 (与 `pick_model_title` 同参数形状但
    /// 措辞不同——向导语境是"正在为这个槽位挑一个模型", 订阅详情页是"修改这个槽位的模型", 两处
    /// 不合并成同一个字段)。
    pub wiz_pick_model: fn(slot: &str) -> String,
    pub wiz_btn_save: &'static str,
    /// 四个核心槽位任一为空时的校验错误 (兜底槽不参与)。
    pub wiz_err_slot: &'static str,
    /// 订阅已经建好之后 `Esc` 的确认文案: 订阅已经建好了 (不是 `confirm_discard` 那种"放弃未保存
    /// 的编辑", 而是"这条订阅会带着 (pending) 槽位留在后端")。
    pub wiz_confirm_exit_pending: &'static str,
    /// 保存槽位在飞时的按钮文案 (busy 态)。
    pub wiz_saving: &'static str,

    // ---------- 向导自定义厂商单页 (协议 / Base URL / 鉴权 / 探测) ----------
    pub wiz_custom_title: &'static str,
    pub wiz_f_protocol: &'static str,
    pub wiz_f_provider_name: &'static str,
    pub wiz_f_base_url: &'static str,
    pub wiz_f_messages_path: &'static str,
    pub wiz_f_auth: &'static str,
    pub wiz_btn_probe: &'static str,
    pub wiz_btn_create: &'static str,
    pub wiz_pick_protocol: &'static str,
    /// 只有 Anthropic (未锁定鉴权头) 才会打开这个 picker。
    pub wiz_pick_auth: &'static str,
    /// 与 `ANTHROPIC_AUTH_PRESETS` 顺序一致的两项 label。
    pub wiz_auth_labels: [&'static str; 2],
    pub wiz_err_provider_name: &'static str,
    pub wiz_err_base_url_empty: &'static str,
    pub wiz_err_base_url_scheme: &'static str,
    /// 请求路径不以 `/` 开头 (所有协议共用)。
    pub wiz_err_messages_path: &'static str,
    /// 只有 `Gemini` (不含 `GeminiInteractions`) 要求请求路径含 `{model}`。
    pub wiz_err_gemini_placeholder: &'static str,
    /// 探测在飞时「获取模型列表」按钮的文案 (busy 态)。
    pub wiz_probing: &'static str,
    /// 自定义表单「协议」字段行自己的展示名 (顺序同 `CustomProtocol::ALL`)——**独立于**
    /// `wiz_custom_labels`(那份带 `自定义 · ` 前缀, 给厂商 picker 用), 不是从它派生出来的——
    /// 剥前缀在翻译或改文案后会静默失效, 两份译文各自独立维护。
    pub wiz_protocol_names: [&'static str; 5],

    /// 向导表单底栏: `↑↓` 在字段间移动。
    pub key_field: &'static str,
    /// 向导表单底栏 / 选择行右端 hint: `⏎` 打开选择弹窗。
    pub key_pick: &'static str,
    /// 向导表单底栏: 焦点在文本行时 `⏎` 的说明 (移到下一项, 不是提交, 也不是选择)。
    pub key_next_field: &'static str,
    /// 向导表单底栏 / API Key 行右端 hint: `Ctrl+R` 切换明文/掩码。
    pub key_reveal: &'static str,
    /// 表单内容超过可视高度、被截断时最后一行的提示。
    pub form_more: &'static str,

    // ---------- 错误文案 (`ClientError` / `DiscoveryError` → 用户文字, 见 `client_error`) ----------
    /// `ClientError::NotRunning`。
    pub err_not_running: &'static str,
    /// `ClientError::Disabled` (重读密钥重试后仍是 404 / 401)。
    pub err_disabled: &'static str,
    /// `ClientError::Transport`。
    pub err_network: fn(detail: &str) -> String,
    /// `ClientError::Decode`。
    pub err_bad_response: fn(detail: &str) -> String,
    /// `ClientError::ReadFile` (读本地 CA 证书失败)。`client_error` 会把它的结果再套一层
    /// `err_network`——用户看到的原文是「网络错误: 读取 …」, 这个字段只管后半句。
    pub err_read_file: fn(path: &str, detail: &str) -> String,
    /// `DiscoveryError::MissingEnv`。
    pub err_data_dir_env: fn(var: &str) -> String,
    /// `DiscoveryError::NoRuntimeFile`。
    pub err_file_missing: fn(path: &str) -> String,
    /// `DiscoveryError::Corrupt`。
    pub err_file_corrupt: fn(path: &str, detail: &str) -> String,
    /// `DiscoveryError::NoPort`。
    pub err_no_port: fn(path: &str) -> String,

    // ---------- 命令行 (`main.rs`: `--help` / 参数错误 / `--check` / 连接前提示) ----------
    pub cli_help: &'static str,
    pub cli_err_missing_data_dir_path: &'static str,
    pub cli_err_unknown_arg: fn(arg: &str) -> String,
    /// `explain()` 的 `ClientError::Discovery` 分支, 接在 `client_error` 的结果后面另起一行。
    pub cli_discovery_hint: &'static str,
    /// `explain()` 的 `ClientError::NotRunning` 分支, 接在 `client_error` 的结果后面 (含前导句号,
    /// 这样 `main.rs` 不用再拼一个字面的中文句号)。
    pub cli_not_running_hint: &'static str,
    /// `explain()` 的 `ClientError::Disabled` 分支, 同上 (含前导句号)。
    pub cli_disabled_hint: &'static str,
    pub cli_terminal_init_failed: fn(err: &str) -> String,
    pub cli_check_connected: fn(app_version: &str, pid: u32) -> String,
    pub cli_check_addr: fn(base_url: &str) -> String,
    pub cli_check_mode: fn(mode: &str, listen_all: bool) -> String,
    pub cli_check_subs: fn(total: usize, dispatchable: usize) -> String,
    pub cli_check_lang: fn(lang: &str) -> String,
    pub cli_check_events_ok: &'static str,
    pub cli_check_version_mismatch: fn(tui: &str, app: &str) -> String,
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

/// 唯一的「`ClientError` → 用户文字」入口: toast / `--check` 输出 / 连接前提示都必须经过这里,
/// 不允许在别处直接格式化 `ClientError` (它的 `Display` 现在是英文开发者文字, 只给日志 / `Debug`
/// 用)。
pub fn client_error(s: &Strings, e: &crate::client::ClientError) -> String {
    use crate::client::ClientError;
    match e {
        ClientError::Discovery(d) => discovery_error(s, d),
        ClientError::NotRunning => s.err_not_running.to_string(),
        ClientError::Disabled => s.err_disabled.to_string(),
        ClientError::Api { status, code, message } => format!("{message} ({code}, HTTP {status})"),
        ClientError::Transport(detail) => (s.err_network)(detail),
        ClientError::Decode(detail) => (s.err_bad_response)(detail),
        // 改前 (7a570bc 之前) 这类失败是手写 `ClientError::Transport(format!("读取 {path}: {e}"))`,
        // Display 再套一层「网络错误: {0}」——嵌两层格式化出「网络错误: 读取 {path}: {e}」。拆成
        // `ReadFile` 独立变体是为了能按语言重新格式化, 但用户看到的文字必须保持这句原文不变,
        // 所以这里手动把 `err_read_file` 的结果再套一层 `err_network`, 还原原来的嵌套。
        ClientError::ReadFile { path, message } => (s.err_network)(&(s.err_read_file)(path, message)),
    }
}

fn discovery_error(s: &Strings, e: &crate::client::discovery::DiscoveryError) -> String {
    use crate::client::discovery::DiscoveryError;
    match e {
        DiscoveryError::MissingEnv(var) => (s.err_data_dir_env)(var),
        DiscoveryError::NoRuntimeFile(path) => (s.err_file_missing)(&path.display().to_string()),
        DiscoveryError::Corrupt(path, detail) => (s.err_file_corrupt)(&path.display().to_string(), detail),
        DiscoveryError::NoPort(path) => (s.err_no_port)(&path.display().to_string()),
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
    key_delete: "删除",
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
    toast_busy: |name| format!("{name}：上一个操作还没完成,请稍候"),
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
        // 与「改当前槽位的模型」「改当前槽位的思考档位」两行合并——新增 n/d 两行后帮助弹窗刚好
        // 顶到最小终端高度, 合并这一对腾出一行 (`every_page_help_fits_the_minimum_terminal`)。
        ("⏎ / o (详情内)", "改模型 / 改思考档位"),
        ("s", "保存槽位修改"),
        ("e", "启用 / 停用"),
        ("t", "测试连接"),
        ("m", "刷新模型列表"),
        ("b", "刷新余额"),
        ("n", "新建订阅"),
        ("d", "删除订阅"),
    ],
    sub_busy_toggling: "正在切换…",
    sub_busy_testing: "正在测试连接…",
    sub_busy_models: "正在获取模型…",
    sub_busy_balance: "正在查询余额…",
    sub_busy_saving: "正在保存…",
    sub_busy_deleting: "正在删除…",
    sub_slot_modified: "已修改",
    sub_save_first: "先按 s 保存或 Esc 放弃当前修改",
    sub_effort_na_fallback: "兜底槽没有思考档位",
    sub_effort_na_kiro: "Kiro 不支持思考档位",
    sub_model_required: "模型不能为空",
    sub_gone: "这条订阅已不存在",
    saving_in_progress: "正在保存,请稍候",

    sub_confirm_delete: |name| format!("删除订阅「{name}」?"),
    sub_delete_refs: |n| format!("这条订阅被 {n} 个虚拟模型引用,删除后会自动解绑:"),
    sub_delete_refs_more: |n| format!("… 等 {n} 个"),
    toast_deleted: |name| format!("已删除「{name}」"),
    list_sep: "、",

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
    lg_d_effort_source_suffix: |label| format!("（{label}）"),
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

    wiz_loading_models: "正在获取模型列表…",
    wiz_save_failed: |reason| format!("保存失败: {reason}"),
    wiz_models_manual: |reason| format!("自动获取模型列表失败, 请手动填写: {reason}"),
    wiz_pick_model: |slot| format!("为 {slot} 选择模型"),
    wiz_btn_save: "保存",
    wiz_err_slot: "请填写模型",
    wiz_confirm_exit_pending: "订阅已经创建, 但模型槽位还是 (pending)。退出向导? (稍后可以在订阅页设置)",
    wiz_saving: "正在保存…",

    wiz_custom_title: "新建订阅 · 自定义",
    wiz_f_protocol: "协议",
    wiz_f_provider_name: "厂商名",
    wiz_f_base_url: "Base URL",
    wiz_f_messages_path: "请求路径",
    wiz_f_auth: "鉴权",
    wiz_btn_probe: "获取模型列表",
    wiz_btn_create: "创建",
    wiz_pick_protocol: "选择协议",
    wiz_pick_auth: "选择鉴权方式",
    wiz_auth_labels: ["Authorization: Bearer <key>", "x-api-key: <key>"],
    wiz_err_provider_name: "请填写厂商名",
    wiz_err_base_url_empty: "请填写 Base URL",
    wiz_err_base_url_scheme: "Base URL 必须以 http:// 或 https:// 开头",
    wiz_err_messages_path: "请求路径必须以 / 开头",
    wiz_err_gemini_placeholder: "Gemini 的请求路径必须包含 {model}",
    wiz_probing: "正在获取模型列表…",
    wiz_protocol_names: ["Anthropic 兼容", "Gemini", "OpenAI Responses", "OpenAI Chat Completions", "Gemini Interactions"],

    key_field: "字段",
    key_pick: "选择",
    key_next_field: "下一项",
    key_reveal: "显示 / 隐藏",
    form_more: "… 内容放不下",

    err_not_running: "cc-router 未在运行",
    err_disabled: "终端界面未启用",
    err_network: |detail| format!("网络错误: {detail}"),
    err_bad_response: |detail| format!("响应无法解析: {detail}"),
    err_read_file: |path, detail| format!("读取 {path}: {detail}"),
    err_data_dir_env: |var| format!("无法确定数据目录: 环境变量 {var} 未设置"),
    err_file_missing: |path| format!("未找到 {path}"),
    err_file_corrupt: |path, detail| format!("{path} 已损坏: {detail}"),
    err_no_port: |path| format!("{path} 里没有可用端口"),

    cli_help: "\
cc-router-tui — cc-router 的终端界面

用法: cc-router-tui [选项]
不带参数运行即进入界面 (需要 cc-router 桌面 app 正在运行, 且已在 设置 → 安全与访问 → 终端界面 打开开关)。

选项:
  --check            连接正在运行的 cc-router 并打印状态, 然后退出
  --data-dir <路径>  指定 cc-router 的数据目录 (默认按系统规则查找)
  --no-fx            关闭动效 (也可以设环境变量 CCR_TUI_NO_FX=1)
  -V, --version      打印版本
  -h, --help         打印本帮助
",
    cli_err_missing_data_dir_path: "--data-dir 需要一个路径",
    cli_err_unknown_arg: |arg| format!("未知参数: {arg}"),
    cli_discovery_hint: "请先启动 cc-router 桌面 app。",
    cli_not_running_hint: "。请先启动桌面 app。",
    cli_disabled_hint: "。请在桌面 app 的 设置 → 安全与访问 → 终端界面 打开开关。",
    cli_terminal_init_failed: |err| format!("无法初始化终端: {err}\n请在真正的终端窗口里运行 cc-router-tui。"),
    cli_check_connected: |app_version, pid| format!("已连接 cc-router {app_version} (pid {pid})"),
    cli_check_addr: |base_url| format!("  地址     {base_url}"),
    cli_check_mode: |mode, listen_all| format!("  模式     {mode}{}", if listen_all { " · 监听 0.0.0.0" } else { "" }),
    cli_check_subs: |total, dispatchable| format!("  订阅     {total} 个, {dispatchable} 个可调度"),
    cli_check_lang: |lang| format!("  语言     {lang}"),
    cli_check_events_ok: "  事件流   正常",
    cli_check_version_mismatch: |tui, app| format!("\n注意: TUI 版本 {tui} 与 app 版本 {app} 不一致。"),
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
        assert_eq!(Lang::resolve("ja", None, env(&[("LANG", "zh_CN.UTF-8")])), Lang::Ja);
        assert_eq!(Lang::resolve("en", Some("zh-CN"), env(&[])), Lang::En);
    }

    /// runtime.json 下发的系统标签必须先于环境变量被采信——桌面端与 TUI 进程的
    /// 环境变量不一定一致 (比如从 Finder / 快捷方式启动), 系统标签是桌面端自己探测到的事实。
    #[test]
    fn system_tag_from_the_desktop_app_beats_environment() {
        assert_eq!(Lang::resolve("system", Some("ja-JP"), env(&[("LANG", "zh_CN.UTF-8")])), Lang::Ja);
    }

    #[test]
    fn empty_or_missing_system_tag_falls_back_to_environment() {
        assert_eq!(Lang::resolve("system", None, env(&[("LANG", "zh_CN.UTF-8")])), Lang::Zh);
        assert_eq!(Lang::resolve("system", Some(""), env(&[("LANG", "ja_JP.UTF-8")])), Lang::Ja);
        assert_eq!(Lang::resolve("system", None, env(&[])), Lang::En);
    }

    #[test]
    fn lc_all_beats_lang_and_empty_values_are_skipped() {
        assert_eq!(Lang::resolve("system", None, env(&[("LC_ALL", "ja_JP"), ("LANG", "zh_CN")])), Lang::Ja);
        assert_eq!(Lang::resolve("system", None, env(&[("LC_ALL", ""), ("LANG", "zh_CN")])), Lang::Zh);
    }

    /// 未知偏好值 (既不是 zh/en/ja, 也不是 "system"/空串) 应该像 "system" 一样跟随系统——
    /// 与 `tray.rs::TrayLocale::resolve` 的行为一致, 不能悄悄退化成 en。
    #[test]
    fn unknown_preference_follows_the_system_like_the_tray() {
        assert_eq!(Lang::resolve("fr", Some("zh-Hans-CN"), env(&[])), Lang::Zh);
    }

    #[test]
    fn mapping_matches_the_desktop_rule_case_insensitively() {
        assert_eq!(Lang::resolve("system", Some("ZH-Hant"), env(&[])), Lang::Zh);
        assert_eq!(Lang::resolve("system", Some("Ja"), env(&[])), Lang::Ja);
        assert_eq!(Lang::resolve("system", Some("en-US"), env(&[])), Lang::En);
        assert_eq!(Lang::resolve("system", Some("pt-BR"), env(&[])), Lang::En);
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

    fn is_cjk(c: char) -> bool {
        matches!(c as u32,
            0x3000..=0x303F | 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xFF00..=0xFFEF
        )
    }

    /// 给整份源码打一对逐字符布尔掩码。`not_comment[i]`: 这个字符不在 `//`/`///`/`//!` 行注释
    /// 或 `/* */` 块注释 (支持嵌套) 里——字符串字面量的内容仍然算「不在注释里」, 因为扫描的目标
    /// 正是字符串字面量本身, 不能连它一起剥掉。`bare_code[i]`: 这个字符既不在注释里也不在字符串
    /// 字面量里——只给下面 `test_mod_ranges` 的花括号配对用, 字符串内容里出现的 `{`/`}`
    /// (比如断言里的 JSON 文本) 不能被当成语法花括号去配对。
    ///
    /// **已知简化 (不处理字符字面量 `'x'` 与原始字符串 `r"..."`/`r#"..."#`)**: 本仓库当前
    /// 「production 代码」范围内没有含花括号的字符字面量、也没有含 CJK 的原始字符串——这条简化
    /// 目前不影响结果。字符字面量与生命周期标注 (`'a`) 用同一个 `'` 前缀, 稳妥地区分两者需要
    /// 更多上下文, 不值得为了这条扫描测试引入; 如果哪天真的漏报/误报, 应该先来修这里, 不是绕过它。
    fn code_masks(chars: &[char]) -> (Vec<bool>, Vec<bool>) {
        #[derive(Clone, Copy)]
        enum State {
            Normal,
            LineComment,
            BlockComment(u32),
            Str,
        }
        let mut not_comment = vec![true; chars.len()];
        let mut bare_code = vec![true; chars.len()];
        let mut state = State::Normal;
        let mut i = 0;
        while i < chars.len() {
            state = match state {
                State::Normal => {
                    if chars[i] == '"' {
                        bare_code[i] = false;
                        i += 1;
                        State::Str
                    } else if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
                        not_comment[i] = false;
                        bare_code[i] = false;
                        i += 1;
                        State::LineComment
                    } else if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                        not_comment[i] = false;
                        bare_code[i] = false;
                        not_comment[i + 1] = false;
                        bare_code[i + 1] = false;
                        i += 2;
                        State::BlockComment(1)
                    } else {
                        i += 1;
                        State::Normal
                    }
                }
                State::LineComment => {
                    not_comment[i] = false;
                    bare_code[i] = false;
                    let at_newline = chars[i] == '\n';
                    i += 1;
                    if at_newline { State::Normal } else { State::LineComment }
                }
                State::BlockComment(depth) => {
                    not_comment[i] = false;
                    bare_code[i] = false;
                    if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                        not_comment[i + 1] = false;
                        bare_code[i + 1] = false;
                        i += 2;
                        State::BlockComment(depth + 1)
                    } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        not_comment[i + 1] = false;
                        bare_code[i + 1] = false;
                        i += 2;
                        if depth == 1 { State::Normal } else { State::BlockComment(depth - 1) }
                    } else {
                        i += 1;
                        State::BlockComment(depth)
                    }
                }
                State::Str => {
                    bare_code[i] = false;
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        bare_code[i + 1] = false;
                        i += 2;
                        State::Str
                    } else if chars[i] == '"' {
                        i += 1;
                        State::Normal
                    } else {
                        i += 1;
                        State::Str
                    }
                }
            };
        }
        (not_comment, bare_code)
    }

    /// 找出所有 `#[cfg(test)] [<可见性>] mod <ident> { ... }` 的整块范围 (半开区间
    /// `[start, end)`, `start` 是属性开头、`end` 是配对花括号之后一个字符), 花括号配对只数
    /// `bare_code` 为真的花括号。`<可见性>` 是可选的 `pub`/`pub(crate)`/`pub(super)`/
    /// `pub(in ...)`——`client/mod.rs` 的测试专用假后端就写成 `#[cfg(test)] pub(crate) mod
    /// test_support {`, 不认这个前缀会把它误判成生产代码。单个 `#[cfg(test)]` 挂在非 `mod` 项上
    /// (比如一个只在测试里用的小助手函数) 不处理——那种地方按裸代码扫描是刻意接受的简化 (见
    /// `no_cjk_string_literals_outside_i18n` 的文档), 真被抓到就直接挪进 `Strings`, 不值得为了
    /// 跳过它再实现一遍「找下一个语义单元的边界」。
    fn test_mod_ranges(chars: &[char], bare_code: &[bool]) -> Vec<(usize, usize)> {
        fn matches(chars: &[char], bare_code: &[bool], at: usize, needle: &[char]) -> bool {
            at + needle.len() <= chars.len() && (0..needle.len()).all(|k| bare_code[at + k] && chars[at + k] == needle[k])
        }
        fn skip_ws(chars: &[char], bare_code: &[bool], mut j: usize) -> usize {
            while j < chars.len() && bare_code[j] && chars[j].is_whitespace() {
                j += 1;
            }
            j
        }
        /// `pub`/`pub(...)` 可见性修饰符 (若存在) 之后的位置; 不存在原样返回 `j`。
        fn skip_optional_visibility(chars: &[char], bare_code: &[bool], j: usize) -> usize {
            let kw_pub: Vec<char> = "pub".chars().collect();
            if !matches(chars, bare_code, j, &kw_pub) {
                return j;
            }
            let after_pub = j + kw_pub.len();
            // `pub` 后紧跟标识符字符 (比如 `public_thing`) 说明这是另一个词, 不是可见性修饰符。
            if after_pub < chars.len() && bare_code[after_pub] && (chars[after_pub].is_alphanumeric() || chars[after_pub] == '_') {
                return j;
            }
            let after_ws = skip_ws(chars, bare_code, after_pub);
            if after_ws < chars.len() && bare_code[after_ws] && chars[after_ws] == '(' {
                let mut depth = 1i32;
                let mut p = after_ws + 1;
                while p < chars.len() && depth > 0 {
                    if bare_code[p] {
                        if chars[p] == '(' {
                            depth += 1;
                        } else if chars[p] == ')' {
                            depth -= 1;
                        }
                    }
                    p += 1;
                }
                return p;
            }
            after_ws
        }

        let attr: Vec<char> = "#[cfg(test)]".chars().collect();
        let kw_mod: Vec<char> = "mod".chars().collect();
        let mut ranges = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if matches(chars, bare_code, i, &attr) {
                let start = i;
                let mut j = skip_ws(chars, bare_code, i + attr.len());
                j = skip_optional_visibility(chars, bare_code, j);
                j = skip_ws(chars, bare_code, j);
                if matches(chars, bare_code, j, &kw_mod) {
                    j = skip_ws(chars, bare_code, j + kw_mod.len());
                    let ident_start = j;
                    while j < chars.len() && bare_code[j] && (chars[j].is_alphanumeric() || chars[j] == '_') {
                        j += 1;
                    }
                    if j > ident_start {
                        j = skip_ws(chars, bare_code, j);
                        if j < chars.len() && bare_code[j] && chars[j] == '{' {
                            let mut depth = 1u32;
                            let mut p = j + 1;
                            while p < chars.len() && depth > 0 {
                                if bare_code[p] {
                                    if chars[p] == '{' {
                                        depth += 1;
                                    } else if chars[p] == '}' {
                                        depth -= 1;
                                    }
                                }
                                p += 1;
                            }
                            if depth == 0 {
                                ranges.push((start, p));
                                i = p;
                                continue;
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        ranges
    }

    /// 扫一份源码, 返回含 CJK 字符的字符串字面量所在的行号 (1 起, 去重到每行至多一条)。跳过
    /// `#[cfg(test)] mod <ident> { ... }` 整块 (见 [`test_mod_ranges`]) 与全部注释, 字符串字面量
    /// 内容本身 (包括 `"http://127.0.0.1:{p}"` 这类含 `//` 的内容) 照常扫描。
    fn cjk_offenders(source: &str) -> Vec<usize> {
        let chars: Vec<char> = source.chars().collect();
        let (not_comment, bare_code) = code_masks(&chars);
        let excluded = test_mod_ranges(&chars, &bare_code);
        let in_excluded = |i: usize| excluded.iter().any(|&(s, e)| i >= s && i < e);

        let mut offenders = Vec::new();
        let mut line = 1usize;
        for (i, &c) in chars.iter().enumerate() {
            if c == '\n' {
                line += 1;
                continue;
            }
            if not_comment[i] && !in_excluded(i) && is_cjk(c) && offenders.last() != Some(&line) {
                offenders.push(line);
            }
        }
        offenders
    }

    /// **证明「只挖 `mod tests {}` 这一整块, 不是切到第一个 `#[cfg(test)]` 就不再看了」**:
    /// 单个 `#[cfg(test)]` 挂在小助手函数上 (不是 `mod`) 之后的生产代码仍然要被扫到——这正是
    /// 「找第一个 `#[cfg(test)]` 就截断」的旧实现会漏掉的场景 (`wizard/text.rs`/`app.rs` 等文件
    /// 前段就有这种孤立的 `#[cfg(test)]` 小函数, 之后还有几百行真正的生产代码)。
    #[test]
    fn cjk_after_an_early_cfg_test_helper_is_still_reported() {
        let src = "#[cfg(test)]\nfn helper() {}\n\nfn real() {\n    let x = \"中文\";\n}\n";
        assert_eq!(cjk_offenders(src), vec![5]);
    }

    /// `#[cfg(test)] mod tests { .. }` 整块 (含花括号跨行、块内随便写中文断言) 不应该被扫到。
    #[test]
    fn cjk_inside_the_test_mod_block_is_not_reported() {
        let src = "fn real() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        let x = \"中文\";\n        assert_eq!(x, \"中文\");\n    }\n}\n";
        assert_eq!(cjk_offenders(src), Vec::<usize>::new());
    }

    /// `#[cfg(test)] pub(crate) mod test_support { .. }`(`client/mod.rs` 假后端的真实写法)
    /// 中间那个可见性修饰符不能让匹配失败——回归用例, 改前的版本会把这种模块错判成生产代码。
    #[test]
    fn cjk_inside_a_pub_crate_cfg_test_mod_is_not_reported() {
        let src = "fn real() {}\n\n#[cfg(test)]\npub(crate) mod test_support {\n    fn helper() {\n        let x = \"绑定本地端口失败\";\n    }\n}\n";
        assert_eq!(cjk_offenders(src), Vec::<usize>::new());
    }

    /// `mod tests {}` 之后如果又出现生产代码 (不能假设「测试模块永远是文件最后一段」), 仍然要
    /// 被扫到——确认排除的是「这一块本身的范围」, 不是「从这里到文件结尾」。
    #[test]
    fn cjk_after_a_test_mod_block_is_still_reported() {
        let src = "#[cfg(test)]\nmod tests {\n    fn t() { let x = \"中文\"; }\n}\n\nfn real() {\n    let y = \"中文\";\n}\n";
        assert_eq!(cjk_offenders(src), vec![7]);
    }

    /// 字符串字面量里的 `//` (比如 URL) 不能被误判成注释开始 (否则会漏扫真正的字符串内容);
    /// 反过来注释里的中文不该被扫到。两条放一个测试里, 互相印证掩码逻辑没有做反。
    #[test]
    fn urls_in_strings_are_not_mistaken_for_comments_and_comments_are_ignored() {
        let src = "fn real() {\n    // 注释里的中文\n    let url = \"http://例子\";\n}\n";
        assert_eq!(cjk_offenders(src), vec![3]);
    }

    /// **锁住 P6 的收编成果**: `i18n.rs` 之外的非测试代码不许再冒出 CJK 字符串字面量——所有
    /// 用户可见文字都必须经 `Strings`。「非测试代码」精确到 `#[cfg(test)] mod <ident> { ... }`
    /// 这一整块本身 (见 [`cjk_offenders`]), 不是「文件里第一次出现 `#[cfg(test)]` 之前」。
    /// 读文件失败也算失败 (fail-closed), 不能让扫描本身的问题被静默放过。
    #[test]
    fn no_cjk_string_literals_outside_i18n() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("读不了目录 {dir:?}: {e}"));
            for entry in entries.filter_map(Result::ok) {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|e| e == "rs") {
                    out.push(p);
                }
            }
        }

        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src_dir, &mut files);

        let mut offenders = Vec::new();
        for path in files {
            if path.file_name().is_some_and(|n| n == "i18n.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不了 {path:?}: {e}"));
            let rel = path.strip_prefix(&src_dir).unwrap_or(&path).to_path_buf();
            for line in cjk_offenders(&text) {
                offenders.push(format!("{}:{}", rel.display(), line));
            }
        }
        assert!(offenders.is_empty(), "非 i18n.rs 的非测试代码里发现含 CJK 字符的字符串字面量: {offenders:?}");
    }

    /// 每个 `ClientError` 变体经 `client_error(&ZH, ..)` 得到的文字必须与旧版硬编码 `Display`
    /// (改英文之前) 逐字节一致——这就是「纯搬运, 中文显示不变」这条纪律的直接证据。
    #[test]
    fn client_errors_are_worded_by_strings() {
        use crate::client::discovery::DiscoveryError;
        use crate::client::ClientError;
        use std::path::PathBuf;

        assert_eq!(client_error(&ZH, &ClientError::NotRunning), "cc-router 未在运行");
        assert_eq!(client_error(&ZH, &ClientError::Disabled), "终端界面未启用");
        assert_eq!(client_error(&ZH, &ClientError::Transport("x".into())), "网络错误: x");
        assert_eq!(client_error(&ZH, &ClientError::Decode("x".into())), "响应无法解析: x");
        assert_eq!(
            // 改前 (7a570bc) 是 `ClientError::Transport(format!("读取 {path}: {e}"))`, Display 套一层
            // 「网络错误: {0}」——这句「网络错误: 读取 …」的原文必须保持逐字节不变。
            client_error(&ZH, &ClientError::ReadFile { path: "/a/b".into(), message: "denied".into() }),
            "网络错误: 读取 /a/b: denied"
        );
        assert_eq!(
            client_error(&ZH, &ClientError::Api { status: 400, code: "bad_request".into(), message: "无效 id".into() }),
            "无效 id (bad_request, HTTP 400)"
        );
        assert_eq!(
            client_error(&ZH, &ClientError::Discovery(DiscoveryError::MissingEnv("HOME"))),
            "无法确定数据目录: 环境变量 HOME 未设置"
        );
        assert_eq!(
            client_error(&ZH, &ClientError::Discovery(DiscoveryError::NoRuntimeFile(PathBuf::from("/x/runtime.json")))),
            "未找到 /x/runtime.json"
        );
        assert_eq!(
            client_error(&ZH, &ClientError::Discovery(DiscoveryError::Corrupt(PathBuf::from("/x/runtime.json"), "eof".into()))),
            "/x/runtime.json 已损坏: eof"
        );
        assert_eq!(
            client_error(&ZH, &ClientError::Discovery(DiscoveryError::NoPort(PathBuf::from("/x/runtime.json")))),
            "/x/runtime.json 里没有可用端口"
        );
    }
}
