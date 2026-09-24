//! 新建订阅向导。**不是标签页也不是弹窗**, 而是夹在弹窗与全局键之间的一层: 存在时内容区整个
//! 归它, 除 `Ctrl+C` 外所有按键归它 (所以 `q` / `r` / `1`-`5` 能当普通字符输入)。它之上仍然可以
//! 叠**一个**弹窗 (选厂商 / 选模型 / 退出确认), 所以不需要弹窗栈。
//!
//! 它刻意不实现 `Component` (`crate::pages::Component`): 那个 trait 的一半方法
//! (`on_subscriptions_changed` / `on_mutation_*` / `on_event`) 对向导没有意义, 而向导需要的
//! `on_open` 又不在里面。方法签名仍然照着 `Component` 写, 调用约定一致 (`App` 那一侧的
//! `update_wizard` helper与 `update_page` 一一对应)。
//!
//! **表单交互的总规则** (`Basics` / Task 5 的 `Slots` / Task 6 的 `Custom` 三个阶段共用):
//! - 表单是一列「行」, `↑` / `↓` 在**可聚焦**的行之间移动 (说明行与空行跳过), 不绕回;
//!   `Tab` 与 `↓` 同义、`BackTab` (Shift+Tab) 与 `↑` 同义, 在**所有**行类型上都有效 (评审
//!   M8——不再只在文本行才认 `Tab`)。
//! - **文本行**: 直接打字 (不用先进入编辑模式); `⏎` = 移到下一个可聚焦行。
//! - **选择行**: `⏎` = 打开选择弹窗; 不能直接打字。
//! - **按钮行**: `⏎` = 执行。所以「下一步」「保存」「获取模型列表」**都不占用任何字符键**——
//!   这是表单吞掉全部按键之后唯一安全的做法。
//! - `Esc` = 退出向导 (`has_input()` 为真时先弹确认); 请求在飞时整张表单只读 (按钮显示
//!   throbber), 但 `Esc` 能不能用要看这个请求**会不会落库** (评审 M9 + Task 5 评审收窄):
//!   `Create`/`SaveSlots` 在飞时连 `Esc` 也吞 (`can_cancel()` 为假, 撤不回后端落库); 只读的
//!   `LoadProviders`/`LoadModels`/`Probe` 在飞时 `Esc` 可用——`Stage::LoadingModels` 就是这种
//!   (等 `refresh_model_list`, 订阅已经建好了, 退出跟在 `Slots` 退出是一回事)。
//! - **每个异步结果只在发起它的那个阶段被接受, 其余一律丢弃** (Task 5 评审): 向导同一时刻最多
//!   一个请求在飞, 按 `Stage` 判就够, 不需要 `Fetch` 那套 `issued` 序号——`apply_wizard_result`
//!   开头的守卫见那个函数的文档注释。
//!
//! P5 Task 2 只搭了骨架 (`Stage::Loading` / `LoadFailed`, 拉厂商列表、画一个加载中/失败的空容器、
//! `Esc` 退出)。**Task 4 起加真正的表单**: 内置厂商路径的第一步 (`Stage::Basics` / `Creating`)——
//! 选厂商 → 选接入点 → 填 API Key → 备注名 → 下一步。**Task 5 加第二步** (绑定模型,
//! `Stage::LoadingModels` / `Slots` / `Saving`)——创建成功后拉候选模型 → 五个槽位选模型 → 保存,
//! 向导里**不**设置 reasoning effort (与 spec §5.4 的偏离, 见 `WizardCmd::SaveSlots` 的文档
//! 注释: 四个槽全 auto, 用户创建完在订阅详情页按 `o` 就能改)。**Task 6 加自定义厂商的单页表单**
//! (`Stage::Custom` / `Probing`): 协议 / 厂商名 / Base URL / 请求路径 / 鉴权 / API Key / 备注名 /
//! 五个槽位挤在同一屏, `Probe` 按钮探测模型 (不落库)、`Submit` 按钮一次创建 (槽位已经是真值,
//! 不需要 Task 5 那样的第二步 `SaveSlots`)。`Stage::Creating` 被两条路径共用, `custom_draft` 是不是
//! `Some` 就是分流判据 (两条路径互斥)。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ratatui::Frame;
use throbber_widgets_tui::{Throbber, BRAILLE_SIX};

use crate::action::{Action, Cmd, WizardCmd, WizardResult};
use crate::client::dto::{
    AuthHeaderFormat, CreateInput, CreateSource, CustomProtocol, CustomSource, ModelSlots, ProbeInput, ProbeModelsResult, Provider,
    RefreshModelsResult, Slot, ANTHROPIC_AUTH_PRESETS, CUSTOM_BASE_URL_PLACEHOLDER,
};
use crate::i18n::Strings;
use crate::pages::subscriptions::slot_label;
use crate::pages::DrawCtx;
use crate::store::Store;
use crate::theme::Theme;
use crate::widgets::form::{self, FormBuilder, FormRow, FormView};
use crate::widgets::keybar::Hint;
use crate::widgets::picker::{PickerChoice, PickerItem, PickerSpec, PickerTag};
use crate::widgets::spinner_state;
use crate::widgets::toast::ToastKind;

mod fields;
mod form_state;
mod text;
use fields::{
    default_display_name, validate_basics, validate_custom, validate_slots, BasicsDraft, BasicsField, CustomDraft, CustomField, ProbedModels,
    SlotsDraft, SlotsField,
};
use form_state::FormState;
use text::{TextField, TextInput};

/// 向导走到哪一步了。P5 Task 2 只有前两个, Task 4 加了 `Basics`/`Creating`, Task 5 加了
/// `LoadingModels`/`Slots`/`Saving` (穷尽 `match`, 加了不接住就编译失败), Task 6 继续往里加
/// `Custom`/`Probing`。
enum Stage {
    /// 正在拉厂商列表。
    Loading,
    /// 拉失败了, 表单画不出来, 只能 `Esc` 退出。
    LoadFailed(String),
    /// 内置路径第一步: 选厂商 / 选接入点 / 填 API Key / 备注名。
    Basics,
    /// `create_subscription` 在飞: 表单只读, 按钮转圈, **连 `Esc` 也吞掉**——这个请求会落库,
    /// 退出撤不回 (评审 M9)。
    Creating,
    /// `create_subscription` 已经成功 (`created_id` 是 `Some`), 在等 `refresh_model_list`
    /// 回来: 表单同样只读、按钮转圈 (文案 `wiz_loading_models`), 但这是一个**只读**请求, 不
    /// 落库——评审收窄 M9: `Esc` 可用 (`can_cancel()` 为真), 走 `wiz_confirm_exit_pending`
    /// 确认 (订阅已经建好了, 此刻退出与在 `Slots` 退出是一回事)。与旧版"停在 `Creating` 只是
    /// 换按钮文案"的做法不同, 这里拆成独立的 `Stage` 变体, 好让 `can_cancel()` 对它单独判定。
    LoadingModels,
    /// 第二步: 五个槽位选模型, `Save` 触发 `SaveSlots`。
    Slots,
    /// `SaveSlots` (只带 `model_slots` 的 patch, 向导不设置 effort) 在飞: 表单只读, 按钮转圈,
    /// 连 `Esc` 也吞掉 (同 `Creating`, 这个请求会落库)。
    Saving,
    /// 自定义路径单页表单: 协议 / 厂商名 / Base URL / 请求路径 / 鉴权 / API Key / 备注名 / 五个
    /// 槽位。`custom_draft` 恒为 `Some` (由 `apply_provider_choice` 保证)。`Stage::Creating` 与
    /// 内置路径共用 (`custom_draft.is_some()` 是分流判据), 但这一步与下面的 `Probing` 只属于这
    /// 条路径。
    Custom,
    /// `probe_custom_models` 在飞: 表单只读, `Probe` 按钮转圈。这是**只读请求**——`Esc` 可用
    /// (`can_cancel()` 天然为真, 它不在 `Creating | Saving` 集合里), 按 `has_input()`(自定义
    /// 路径恒真) 弹 `confirm_discard`(不是 `wiz_confirm_exit_pending`——创建之前什么都没落库,
    /// 见评审裁决第 3 条)。
    Probing,
}

pub struct Wizard {
    stage: Stage,
    /// `list_providers` 拉到的厂商列表。
    providers: Vec<Provider>,
    /// 与页面的 `pending_notice` 同一套约定, 见 `take_notice`。
    notice: Option<(ToastKind, String)>,
    /// `take_close_request()` 的待办标记。`App` 那一侧转发这个标记的逻辑在 `update()` 返回之后
    /// 轮询一次。
    close_request: bool,
    /// `Stage::Basics` 的草稿 (含 API Key / 备注名两个输入框本身)。选中自定义厂商条目 / OAuth
    /// 厂商都不会碰这个字段——只有选中一个可用的内置厂商才会写 `provider_id`/`endpoint_id` (P5 Task 4)。
    draft: BasicsDraft,
    /// `Stage::Basics` 的焦点与校验错误, 从 `BasicsField::Provider` 起步。
    basics_form: FormState<BasicsField>,
    /// 上一次自动算出来的备注名——据此判断用户是不是已经手动改过它 (改过就不再跟着厂商切换 /
    /// 厂商名编辑重算), 见 `follow_display_name`。两条路径互斥, 共用这一个字段。
    last_auto_display_name: Option<String>,
    /// `create_subscription` 失败时的原因, 挂成表单顶部的说明行 (`FormRow::Note`)。
    create_error: Option<String>,
    /// `Created(Ok(id))` 落地时记下的订阅 id, 供 `LoadModels`/`SaveSlots` 使用 (Task 5)。
    /// `Stage::LoadingModels`/`Slots`/`Saving` 期间恒为 `Some`——由 `apply_wizard_result` 保证。
    created_id: Option<String>,
    /// `Stage::Slots` 的草稿: 五个槽位的值 + 拉到的候选模型 + 说明行。
    slots_draft: SlotsDraft,
    /// `Stage::Slots` 的焦点与校验错误, 从 `SlotsField::Row(Slot::Fable)` 起步。
    slots_form: FormState<SlotsField>,
    /// `Stage::Custom` 的草稿 (含五个输入框本身); `None` 直到用户从厂商 picker 选中一个
    /// `custom:<protocol>` 条目。**是否处于自定义路径由它是不是 `Some` 判定**(`Stage::Creating` 的
    /// 分流依据同样是这个, `apply_slot_choice` 该写回哪一份草稿也是这个)。
    custom_draft: Option<CustomDraft>,
    /// `Stage::Custom` 的焦点与校验错误; 只在 `custom_draft` 为 `Some` 时有意义,
    /// `apply_provider_choice` 进 `Stage::Custom` 时重置成焦点在 `CustomField::ProviderName`。
    custom_form: FormState<CustomField>,
}

// clippy::new_without_default: `App`/页面构造函数都带参数, 没有这条先例——`Wizard::new()` 恰好是
// 这个 crate 第一个零参数的 `new()`, 照 clippy 的建议直接转发。
impl Default for Wizard {
    fn default() -> Self {
        Self::new()
    }
}

impl Wizard {
    pub fn new() -> Self {
        Self {
            stage: Stage::Loading,
            providers: Vec::new(),
            notice: None,
            close_request: false,
            draft: BasicsDraft::default(),
            basics_form: FormState::new(BasicsField::Provider),
            last_auto_display_name: None,
            create_error: None,
            created_id: None,
            slots_draft: SlotsDraft::default(),
            slots_form: FormState::new(SlotsField::Row(Slot::Fable)),
            custom_draft: None,
            custom_form: FormState::new(CustomField::ProviderName),
        }
    }

    /// 刚打开: 要发的请求 (拉厂商列表)。`App` 在创建它之后立刻调一次。
    pub fn on_open(&mut self) -> Vec<Cmd> {
        vec![Cmd::Wizard(Box::new(WizardCmd::LoadProviders))]
    }

    /// 除 `Ctrl+C` 外的全部按键。`None` = 吞掉 (或只改了向导自己的状态)。`Esc` 的处理跟阶段
    /// 无关 (`can_cancel()` 为假时连 `Esc` 也不接), 排在最前面统一判断; 其余按键只有
    /// `Stage::Basics`/`Slots`/`Custom` 才会真的处理——`Loading`/`LoadFailed` 没有字段可以接收
    /// 输入, `Creating`/`LoadingModels`/`Saving`/`Probing` 整张表单只读 (请求在飞, 见文件顶部
    /// "表单交互的总规则")。**两种文案**: 还没创建 (`Basics`/`Custom`/`Probing`) 复用既有的
    /// `confirm_discard`; 订阅已经建好 (`Slots`, 以及只读等待中的 `LoadingModels`) 时换成
    /// `wiz_confirm_exit_pending` (退出会留下带 (pending) 槽位的订阅, 跟"放弃未保存的编辑"不是
    /// 同一件事)。`store` 只有 `Stage::Custom` 编辑厂商名时会用到 (评审 7: 备注名自动跟随厂商名,
    /// 判重名要查 `Store`), 与 `apply_picker_choice`/`apply_provider_choice` 已经在用的 `store`
    /// 是同一个。
    pub fn handle_key(&mut self, key: KeyEvent, store: &Store, s: &'static Strings) -> Option<Action> {
        if key.code == KeyCode::Esc && self.can_cancel() {
            if !self.has_input() {
                return Some(Action::CloseWizard);
            }
            let prompt = if matches!(self.stage, Stage::Slots | Stage::LoadingModels) { s.wiz_confirm_exit_pending } else { s.confirm_discard };
            return Some(Action::OpenConfirm { prompt: prompt.to_string(), on_yes: Box::new(Action::CloseWizard) });
        }
        if matches!(self.stage, Stage::Basics) {
            self.handle_basics_key(key, s)
        } else if matches!(self.stage, Stage::Slots) {
            self.handle_slots_key(key, s)
        } else if matches!(self.stage, Stage::Custom) {
            self.handle_custom_key(key, store, s)
        } else {
            None
        }
    }

    /// `Stage::Basics` 的按键表 (总规则见文件顶部文档): `↑`/`↓`/`Tab`/`BackTab` 在 5 个可聚焦
    /// 字段之间移动 (评审 M8: `Tab` 与 `↓` 同义、`BackTab` 与 `↑` 同义, 在**所有**行类型上都
    /// 有效, 不再只在文本行才认); `Provider`/`Endpoint` 是选择行, `⏎` 开对应的 picker;
    /// `ApiKey`/`DisplayName` 是文本行, 直接打字、`⏎` 移到下一行, `ApiKey` 额外认 `Ctrl+R`
    /// 切换明文/掩码; `Submit` 是按钮行, `⏎` 触发校验 + 提交。
    fn handle_basics_key(&mut self, key: KeyEvent, s: &'static Strings) -> Option<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let focus = self.basics_form.focus;
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.basics_form.step(-1, &BasicsField::ALL);
                None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.basics_form.step(1, &BasicsField::ALL);
                None
            }
            KeyCode::Enter if focus == BasicsField::Provider => Some(self.open_provider_picker(s)),
            KeyCode::Enter if focus == BasicsField::Endpoint => Some(self.open_endpoint_picker(s)),
            KeyCode::Enter if matches!(focus, BasicsField::ApiKey | BasicsField::DisplayName) => {
                self.basics_form.step(1, &BasicsField::ALL);
                None
            }
            KeyCode::Enter if focus == BasicsField::Submit => self.submit(s),
            // 必须排在下面的打字分支之前: 否则 Ctrl+R 会被 `ApiKey` 的编辑分支当成字符 'r' 吃掉。
            KeyCode::Char('r') if ctrl && focus == BasicsField::ApiKey => {
                self.draft.api_key.toggle_reveal();
                None
            }
            // 文本行交给统一的编辑路径; 选择行 (`Provider`/`Endpoint`) 没有文本字段, 其余按键
            // (含字符键) 一律吞掉——"选择行不能直接打字"。
            _ => {
                edit_focused_text(self.draft.text_field(focus), &mut self.basics_form, key);
                None
            }
        }
    }

    /// `Stage::Slots` 的按键表 (总规则同 `Basics`): `↑`/`↓`/`Tab`/`BackTab` 在 6 个可聚焦字段
    /// 之间移动 (5 个槽位行 + `Save` 按钮); 槽位行是选择行, `⏎` 开模型 picker; `Save` 是按钮行,
    /// `⏎` 触发校验 + 提交——与 `Basics` 不同的是这一步**没有文本行**, 全部字段都要么是选择行
    /// 要么是按钮行, 所以不需要区分"打字"分支。
    fn handle_slots_key(&mut self, key: KeyEvent, s: &'static Strings) -> Option<Action> {
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.slots_form.step(-1, &SlotsField::ALL);
                None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.slots_form.step(1, &SlotsField::ALL);
                None
            }
            KeyCode::Enter => match self.slots_form.focus {
                SlotsField::Row(slot) => Some(self.open_slot_picker(slot, s)),
                SlotsField::Save => self.submit_slots(s),
            },
            _ => None,
        }
    }

    /// `Stage::Custom` 的按键表 (总规则同 `Basics`/`Slots`): `↑`/`↓`/`Tab`/`BackTab` 在
    /// [`CustomField::all`] 给出的可聚焦字段间移动 (`Auth` 锁定时被排除); `Protocol`/未锁定时的
    /// `Auth` 是选择行, `⏎` 开对应 picker; `ProviderName`/`BaseUrl`/`MessagesPath`/`ApiKey`/
    /// `DisplayName` 是文本行, 直接打字、`⏎` 移到下一行, `ApiKey` 额外认 `Ctrl+R`; `Probe`/
    /// `Submit` 是按钮行, `⏎` 触发对应请求; 槽位行 `⏎` 开模型 picker。`custom_draft` 是 `None`
    /// 时 (理论不该发生, `Stage::Custom` 只由 `apply_provider_choice` 设置且同时写好
    /// `custom_draft`) 防御性地什么都不做。
    fn handle_custom_key(&mut self, key: KeyEvent, store: &Store, s: &'static Strings) -> Option<Action> {
        let draft = self.custom_draft.as_mut()?;
        let locked = draft.protocol.auth_locked();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let focus = self.custom_form.focus;
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.custom_form.step(-1, &CustomField::all(locked));
                None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.custom_form.step(1, &CustomField::all(locked));
                None
            }
            KeyCode::Enter => match focus {
                CustomField::Protocol => Some(self.open_protocol_picker(s)),
                CustomField::Auth if !locked => Some(self.open_auth_picker(s)),
                // 锁定态理论上不会被聚焦到 (`CustomField::all` 已经把它排除在导航列表之外), 但
                // `⏎` 落在这里防御性地吞掉, 不做任何事。
                CustomField::Auth => None,
                CustomField::ProviderName | CustomField::BaseUrl | CustomField::MessagesPath | CustomField::ApiKey | CustomField::DisplayName => {
                    self.custom_form.step(1, &CustomField::all(locked));
                    None
                }
                CustomField::Probe => self.submit_probe(s),
                CustomField::Slot(slot) => Some(self.open_custom_slot_picker(slot, s)),
                CustomField::Submit => self.submit_custom(s),
            },
            // 必须排在下面的打字分支之前, 同 `handle_basics_key` 的 Ctrl+R 分支——否则会被
            // `ApiKey` 的编辑分支当成字符 'r' 吃掉。
            KeyCode::Char('r') if ctrl && focus == CustomField::ApiKey => {
                draft.api_key.toggle_reveal();
                None
            }
            // 文本行交给统一的编辑路径; 选择行 (`Protocol`/`Auth`) 与按钮行 (`Probe`/`Submit`/
            // 槽位行) 没有文本字段, 其余按键一律吞掉——"选择行不能直接打字"。**编辑 Base URL 不清
            // `probe`**(评审裁决第 7 条)——`CustomDraft::models_url()` 自己按值比对, 编辑时提前
            // 清空反而会丢掉"改回去又生效"这条桌面端行为; 只有换协议 (`apply_protocol_choice`) 才清。
            _ => {
                let changed = edit_focused_text(draft.text_field(focus), &mut self.custom_form, key);
                // 评审 7: 备注名跟着厂商名自动生成, 与内置路径选厂商时同一个函数; 厂商名 trim 后为空
                // 时不自动填 (刚进表单、还没打字时不该凭空冒出一个基于空字符串的默认值)。
                if changed && focus == CustomField::ProviderName {
                    let name = draft.provider_display_name.value().trim().to_string();
                    if !name.is_empty() && follow_display_name(&mut draft.display_name, &mut self.last_auto_display_name, &name, store) {
                        self.custom_form.clear(CustomField::DisplayName);
                    }
                }
                None
            }
        }
    }

    /// `Protocol` 行 `⏎`: 5 项, label 复用厂商 picker 里那份 `wiz_custom_labels`(与 Step 1 选择
    /// `custom:<protocol>` 条目时看到的文案一致)。
    fn open_protocol_picker(&self, s: &'static Strings) -> Action {
        let current = self.custom_draft.as_ref().map(|d| d.protocol.as_wire().to_string()).unwrap_or_default();
        let items = CustomProtocol::ALL
            .iter()
            .zip(s.wiz_custom_labels.iter())
            .map(|(p, label)| PickerItem { id: p.as_wire().to_string(), label: (*label).to_string(), hint: None })
            .collect();
        Action::OpenPicker(PickerSpec {
            tag: PickerTag::WizardProtocol,
            title: s.wiz_pick_protocol.to_string(),
            items,
            allow_custom: false,
            initial: current,
        })
    }

    /// `Auth` 行 `⏎` (只有 Anthropic 未锁定鉴权头时才会被派发到这里): `ANTHROPIC_AUTH_PRESETS`
    /// 两项, id 用头名本身当唯一标识 (两个预设的头名不同, 天然唯一)。
    fn open_auth_picker(&self, s: &'static Strings) -> Action {
        let current = self.custom_draft.as_ref().map(|d| d.auth_header_name.clone()).unwrap_or_default();
        let items = ANTHROPIC_AUTH_PRESETS
            .iter()
            .zip(s.wiz_auth_labels.iter())
            .map(|((header, _format), label)| PickerItem { id: (*header).to_string(), label: (*label).to_string(), hint: None })
            .collect();
        Action::OpenPicker(PickerSpec { tag: PickerTag::WizardAuth, title: s.wiz_pick_auth.to_string(), items, allow_custom: false, initial: current })
    }

    /// 槽位行 `⏎`: 与 Task 5 完全同一套 `PickerTag::WizardSlot`(`apply_slot_choice` 按
    /// `custom_draft` 是不是 `Some` 分流写回哪一份草稿), 候选来自 `custom_draft.slots.models`
    /// (探测到的模型; 探测前 / 探测失败时为空, 引导手输)——自定义厂商没有 `example_models` 这个
    /// 概念, 候选为空时不像 `open_slot_picker` 那样还有厂商兜底可退。
    fn open_custom_slot_picker(&self, slot: Slot, s: &'static Strings) -> Action {
        let (models, initial) = match &self.custom_draft {
            Some(draft) => (draft.slots.models.clone(), draft.slots.slots.get(slot).to_string()),
            None => (Vec::new(), String::new()),
        };
        let mut items = Vec::new();
        if slot == Slot::Fallback {
            items.push(PickerItem { id: String::new(), label: s.pick_clear_fallback.to_string(), hint: None });
        }
        items.extend(models.iter().map(|m| PickerItem { id: m.id.clone(), label: m.id.clone(), hint: m.display_name.clone() }));
        Action::OpenPicker(PickerSpec {
            tag: PickerTag::WizardSlot { slot },
            title: (s.wiz_pick_model)(slot_label(slot, s)),
            items,
            allow_custom: true,
            initial,
        })
    }

    /// 换协议: 重置连接相关字段的草稿 (`CustomDraft::apply_protocol`, 输入框就在草稿里, 不用另外
    /// 同步), 清掉这三个字段自己的校验错误 (评审 M3 同一条道理——被 `apply_protocol`
    /// 重写的字段等同于被"编辑"过), 再把停在刚锁定的 `Auth` 行上的焦点挪开防止悬空。**重选同一个协议
    /// (评审 1, 仿 Task 4 评审 M2 在厂商行修过的同一类问题) 什么都不重算, 连 `probe` 都不清**——
    /// 否则用户在 Anthropic 下手动把 Base URL / 请求路径 / 鉴权都改成中转站的真实值之后, 回到
    /// 协议行看一眼、又按了一次 `⏎`(picker 默认高亮当前项, 很容易无意中确认同一项), 这些手填的
    /// 值会被悄悄弹回协议预设 (Anthropic 的预设 Base URL 是空串, 用户完全看不出发生了什么——
    /// 只会看到灰字占位符, 误以为那就是真值), 建出的订阅连不上上游。
    fn apply_protocol_choice(&mut self, choice: &PickerChoice) {
        let PickerChoice::Item(id) = choice else { return };
        let Some(protocol) = CustomProtocol::ALL.iter().find(|p| p.as_wire() == id).copied() else { return };
        let Some(draft) = &mut self.custom_draft else { return };
        if protocol == draft.protocol {
            return;
        }
        draft.apply_protocol(protocol);
        self.custom_form.clear(CustomField::BaseUrl);
        self.custom_form.clear(CustomField::MessagesPath);
        self.custom_form.clear(CustomField::Auth);
        // 焦点恰好停在刚被锁定的 `Auth` 行时 (少见: 焦点停在 Auth 行时重新打开协议 picker 并选中
        // 一个锁定协议), 挪到相邻的 `ApiKey` 行——避免焦点停在一个再也进不了导航列表的字段上。
        if self.custom_form.focus == CustomField::Auth && protocol.auth_locked() {
            self.custom_form.focus = CustomField::ApiKey;
        }
    }

    fn apply_auth_choice(&mut self, choice: &PickerChoice) {
        let PickerChoice::Item(id) = choice else { return };
        let Some((header, format)) = ANTHROPIC_AUTH_PRESETS.iter().find(|entry| entry.0 == id.as_str()) else { return };
        if let Some(draft) = &mut self.custom_draft {
            draft.auth_header_name = (*header).to_string();
            draft.auth_header_format = *format;
        }
        self.custom_form.clear(CustomField::Auth);
    }

    /// `Probe` 行 `⏎`: 只校验 `base_url` 非空 + API Key 非空 (与桌面端一致, 其余字段这一步不
    /// 校验), 通过则打包 `WizardCmd::Probe` 并进 `Stage::Probing`。**发起时清掉 `slots.note`**
    /// (评审 6, 仿 `submit`/`submit_slots` 对 `create_error`/`note` 的处理)——否则重试在飞期间,
    /// 屏幕上会同时显示"上一次探测失败的原因"和"正在获取模型列表…"两条互相矛盾的文案。
    fn submit_probe(&mut self, s: &'static Strings) -> Option<Action> {
        let draft = self.custom_draft.as_ref()?;
        if draft.base_url.value().trim().is_empty() {
            self.custom_form.reject(CustomField::BaseUrl, s.wiz_err_base_url_empty);
            return None;
        }
        if draft.api_key.is_empty() {
            self.custom_form.reject(CustomField::ApiKey, s.wiz_err_api_key);
            return None;
        }
        let cmd = WizardCmd::Probe(ProbeInput {
            base_url: draft.base_url.value().trim().to_string(),
            auth_header_name: draft.auth_header_name.clone(),
            auth_header_format: draft.auth_header_format,
            api_key: draft.api_key.secret(),
            protocol: draft.protocol,
        });
        self.custom_form.clear_all();
        if let Some(draft) = &mut self.custom_draft {
            draft.slots.note = None;
        }
        self.stage = Stage::Probing;
        Some(Action::WizardRequest(Box::new(cmd)))
    }

    /// `Submit`(「创建」) 行 `⏎`: `validate_custom` 失败则把焦点移到那个字段并挂错误 (与
    /// `submit`/`submit_slots` 同一条道理); 通过则打包 `WizardCmd::Create`——`model_slots` 是
    /// 真实选值 (不是 `ModelSlots::pending()`, `validate_custom` 已经保证四个核心槽非空),
    /// `models_url` 由 `CustomDraft::models_url()` 按"探测后 base_url 没再改过"这条规则算。
    /// **发起时清掉 `slots.note`**(评审 6), 理由同 `submit_probe`。
    fn submit_custom(&mut self, s: &'static Strings) -> Option<Action> {
        let draft = self.custom_draft.as_ref()?;
        match validate_custom(draft, s) {
            Some((field, message)) => {
                self.custom_form.reject(field, message);
                None
            }
            None => {
                let cmd = WizardCmd::Create(CreateInput {
                    display_name: draft.display_name.value().to_string(),
                    api_key: draft.api_key.secret(),
                    model_slots: draft.slots.slots.clone(),
                    source: CreateSource::Custom(Box::new(CustomSource {
                        provider_display_name: draft.provider_display_name.value().to_string(),
                        base_url: draft.base_url.value().trim().to_string(),
                        messages_path: draft.messages_path.value().trim().to_string(),
                        auth_header_name: draft.auth_header_name.clone(),
                        auth_header_format: draft.auth_header_format,
                        protocol: draft.protocol,
                        models_url: draft.models_url().map(str::to_string),
                    })),
                });
                self.custom_form.clear_all();
                if let Some(draft) = &mut self.custom_draft {
                    draft.slots.note = None;
                }
                self.stage = Stage::Creating;
                Some(Action::WizardRequest(Box::new(cmd)))
            }
        }
    }

    /// `Provider` 行 `⏎`: 条目是全部厂商 + 5 个自定义协议。OAuth 类厂商 (TUI 不做设备码流程)
    /// 的 label 后面追加提示语, picker 本身没有"置灰不可选"的能力, 选中时用「可选但选了只给
    /// 提示」等效 (`apply_provider_choice` 里判断)。
    fn open_provider_picker(&self, s: &'static Strings) -> Action {
        let mut items: Vec<PickerItem> = self
            .providers
            .iter()
            .map(|p| {
                let label = if p.is_oauth() { format!("{} · {}", p.display_name, s.wiz_desktop_only) } else { p.display_name.clone() };
                PickerItem { id: p.id.clone(), label, hint: p.description.clone() }
            })
            .collect();
        for (protocol, label) in CustomProtocol::ALL.iter().zip(s.wiz_custom_labels.iter()) {
            items.push(PickerItem { id: format!("custom:{}", protocol.as_wire()), label: (*label).to_string(), hint: None });
        }
        Action::OpenPicker(PickerSpec {
            tag: PickerTag::WizardProvider,
            title: s.wiz_pick_provider.to_string(),
            items,
            allow_custom: false,
            initial: self.draft.provider_id.clone(),
        })
    }

    /// `Endpoint` 行 `⏎`: 厂商还没选时 (或者选中的 id 在厂商列表里找不到, 防御性地) 就地拒绝,
    /// 不开弹窗。
    fn open_endpoint_picker(&self, s: &'static Strings) -> Action {
        let Some(provider) = self.providers.iter().find(|p| p.id == self.draft.provider_id) else {
            return Action::Notify { kind: ToastKind::Info, text: s.wiz_pick_provider_first.to_string() };
        };
        let items =
            provider.endpoints.iter().map(|e| PickerItem { id: e.id.clone(), label: e.label.clone(), hint: Some(e.base_url.clone()) }).collect();
        Action::OpenPicker(PickerSpec {
            tag: PickerTag::WizardEndpoint,
            title: s.wiz_pick_endpoint.to_string(),
            items,
            allow_custom: false,
            initial: self.draft.endpoint_id.clone(),
        })
    }

    /// 槽位行 `⏎`: 候选是 `slots_draft.models` (拉到的真实候选); 拉不到 (`ManualFallback` / 请求
    /// 失败, `models` 空) 时退回当前厂商的 `example_models` (只有 id, 没有 hint), 引导手输——
    /// `allow_custom: true` 让两种情况下都能直接打字。兜底槽额外在最前面放一项「清空」(复用 P3b
    /// 订阅详情页那个「清空」项的文案字段 `pick_clear_fallback`; 简报写的字段名是 `s.slot_clear`,
    /// 但代码库里从来没有这个名字——按"复用既有字段"的裁决取实际存在的那个)。
    fn open_slot_picker(&self, slot: Slot, s: &'static Strings) -> Action {
        let initial = self.slots_draft.slots.get(slot).to_string();
        let mut items = Vec::new();
        if slot == Slot::Fallback {
            items.push(PickerItem { id: String::new(), label: s.pick_clear_fallback.to_string(), hint: None });
        }
        if self.slots_draft.models.is_empty() {
            let examples = self
                .providers
                .iter()
                .find(|p| p.id == self.draft.provider_id)
                .map(|p| p.model_discovery.example_models.as_slice())
                .unwrap_or(&[]);
            items.extend(examples.iter().map(|id| PickerItem { id: id.clone(), label: id.clone(), hint: None }));
        } else {
            items.extend(self.slots_draft.models.iter().map(|m| PickerItem { id: m.id.clone(), label: m.id.clone(), hint: m.display_name.clone() }));
        }
        Action::OpenPicker(PickerSpec {
            tag: PickerTag::WizardSlot { slot },
            title: (s.wiz_pick_model)(slot_label(slot, s)),
            items,
            allow_custom: true,
            initial,
        })
    }

    /// `Action::PickerDone` 落地: 按 `tag` 分派给厂商 / 接入点 / 槽位 / 协议 / 鉴权方式五条分支;
    /// 跟向导无关的 tag (其它页面自己的弹窗) 直接忽略——**穷尽 `match`**, 新增 `PickerTag` 变体时
    /// 这里会编译失败, 逼着显式决定向导要不要关心它。
    fn apply_picker_choice(&mut self, tag: &PickerTag, choice: &PickerChoice, store: &Store, s: &'static Strings) {
        match tag {
            PickerTag::WizardProvider => self.apply_provider_choice(choice, store, s),
            PickerTag::WizardEndpoint => self.apply_endpoint_choice(choice),
            PickerTag::WizardSlot { slot } => self.apply_slot_choice(*slot, choice),
            PickerTag::WizardProtocol => self.apply_protocol_choice(choice),
            PickerTag::WizardAuth => self.apply_auth_choice(choice),
            PickerTag::SlotModel { .. }
            | PickerTag::SlotEffort { .. }
            | PickerTag::VmAddSubscription { .. }
            | PickerTag::LiveFilter
            | PickerTag::LogsFilter => {}
        }
    }

    /// 选中内置厂商 → 记 `provider_id`, `endpoint_id` 置为 `provider.default_endpoint()`,
    /// `display_name` 若为空**或**等于上一次自动生成的值则重算, 焦点移到 `ApiKey`。**重选同一个
    /// 厂商 (评审 M2) 什么都不重算**——接入点/备注名/焦点原样保留, 否则用户手动把接入点从默认值
    /// 改成别的、又不小心在厂商行按了 `⏎` 确认同一个厂商, 接入点会被悄悄弹回默认值, 用户毫无
    /// 察觉。选中自定义条目 (P5 Task 6) → 构造一份全新 `CustomDraft`(`CustomDraft::new` 已经按
    /// 协议预设填好连接字段, 输入框就在草稿里), 焦点落在 `ProviderName`、错误清空, 进 `Stage::Custom`。
    /// 选中 OAuth 厂商 → 不设值, 只弹 `wiz_desktop_only`。
    fn apply_provider_choice(&mut self, choice: &PickerChoice, store: &Store, s: &'static Strings) {
        // `allow_custom: false`: picker 理论上不会产出 `Custom`, 防御性地忽略。
        let PickerChoice::Item(id) = choice else { return };
        if let Some(wire) = id.strip_prefix("custom:") {
            let Some(protocol) = CustomProtocol::ALL.iter().find(|p| p.as_wire() == wire).copied() else { return };
            // 全新的自定义草稿: 上一次 (可能是内置路径留下的) 自动生成备注名的记录不该带过来,
            // 否则编辑厂商名时第一次判断"是否还处于自动跟随"可能被 stale 值干扰 (评审 7)。
            self.last_auto_display_name = None;
            self.custom_draft = Some(CustomDraft::new(protocol));
            self.custom_form = FormState::new(CustomField::ProviderName);
            self.stage = Stage::Custom;
            return;
        }
        let Some(provider) = self.providers.iter().find(|p| &p.id == id) else { return };
        if provider.is_oauth() {
            self.notice = Some((ToastKind::Info, s.wiz_desktop_only.to_string()));
            return;
        }
        // 把后面要用的字段先拷成拥有所有权的值——`provider` 借用着 `self.providers`, 下面几行都
        // 要改 `self` 的其它字段, 两者不能同时活着。
        let provider_id = provider.id.clone();
        let provider_display_name = provider.display_name.clone();
        let default_endpoint_id = provider.default_endpoint().map(|e| e.id.clone()).unwrap_or_default();

        // 选中了一个合法的内置厂商 (不管是不是重选同一个), 这个字段本身就算通过了, 先清错误
        // (评审 M3)。
        self.basics_form.clear(BasicsField::Provider);
        if provider_id == self.draft.provider_id {
            return; // M2: 重选同一个厂商, 接入点/备注名/焦点都不动。
        }
        self.draft.provider_id = provider_id;
        self.draft.endpoint_id = default_endpoint_id;
        self.basics_form.clear(BasicsField::Endpoint);
        if follow_display_name(&mut self.draft.display_name, &mut self.last_auto_display_name, &provider_display_name, store) {
            self.basics_form.clear(BasicsField::DisplayName);
        }
        self.basics_form.focus = BasicsField::ApiKey;
    }

    fn apply_endpoint_choice(&mut self, choice: &PickerChoice) {
        if let PickerChoice::Item(id) = choice {
            self.draft.endpoint_id = id.clone();
            self.basics_form.clear(BasicsField::Endpoint);
        }
    }

    /// 兜底槽的「清空」项 (`id: ""`) 与其它槽位的正常选值走同一条路径: 写回空串本来就是它的语义
    /// (未配置), 不需要特殊分支。自定义输入 (`Custom`) `trim` 一下, 与订阅详情页 `open_model_picker`
    /// 那条路径同规则。**P5 Task 6 起按 `custom_draft` 是不是 `Some` 分流写回哪一份草稿**——
    /// `Stage::Slots`(内置路径) 与 `Stage::Custom`(自定义路径) 共用同一个 `PickerTag::WizardSlot`
    /// (见 `open_custom_slot_picker` 的文档注释), 但落地的存储位置不同 (`self.slots_draft` vs
    /// `self.custom_draft.slots`); 两条路径互斥 (`custom_draft` 只在自定义路径下是 `Some`), 这个
    /// 判据是安全的。
    fn apply_slot_choice(&mut self, slot: Slot, choice: &PickerChoice) {
        let value = match choice {
            PickerChoice::Item(id) => id.clone(),
            PickerChoice::Custom(text) => text.trim().to_string(),
        };
        if let Some(draft) = &mut self.custom_draft {
            draft.slots.slots.set(slot, value);
            self.custom_form.clear(CustomField::Slot(slot));
        } else {
            self.slots_draft.slots.set(slot, value);
            self.slots_form.clear(SlotsField::Row(slot));
        }
    }

    /// `Submit` 行 `⏎`: 先 `validate_basics`, 失败则把焦点移到那个字段、把 `error` 挂上去 (Task 8
    /// 会在这里播 `fx::field_err`——焦点此刻已经落在出错的那一行, 用 `form::draw` 返回的聚焦行
    /// 矩形就够); 通过则打包 `WizardCmd::Create` 并进 `Stage::Creating`。
    fn submit(&mut self, s: &'static Strings) -> Option<Action> {
        match validate_basics(&self.draft, s) {
            Some((field, message)) => {
                self.basics_form.reject(field, message);
                None
            }
            None => {
                self.basics_form.clear_all();
                self.create_error = None;
                let cmd = WizardCmd::Create(CreateInput {
                    display_name: self.draft.display_name.value().to_string(),
                    api_key: self.draft.api_key.secret(),
                    model_slots: ModelSlots::pending(),
                    source: CreateSource::Builtin { provider_id: self.draft.provider_id.clone(), endpoint_id: self.draft.endpoint_id.clone() },
                });
                self.stage = Stage::Creating;
                Some(Action::WizardRequest(Box::new(cmd)))
            }
        }
    }

    /// `Save` 行 `⏎`: 先 `validate_slots`, 失败则把焦点移到那个槽位、把 `error` 挂上去 (与
    /// `submit` 同一条道理); 通过则打包 `WizardCmd::SaveSlots`——**只带 `model_slots`**(Task 5
    /// 的裁决: 向导不设置 effort, 少发一个字段就不会把后端默认值清掉)——并进 `Stage::Saving`。
    fn submit_slots(&mut self, s: &'static Strings) -> Option<Action> {
        match validate_slots(&self.slots_draft, s) {
            Some((slot, message)) => {
                self.slots_form.reject(SlotsField::Row(slot), message);
                None
            }
            None => {
                self.slots_form.clear_all();
                self.slots_draft.note = None;
                // `created_id` 在 `Stage::Slots` 期间恒为 `Some` (由 `apply_wizard_result` 的
                // `Created(Ok)` 分支保证); `None` 时防御性地什么都不做, 不 panic。
                let id = self.created_id.clone()?;
                let cmd = WizardCmd::SaveSlots { id, model_slots: self.slots_draft.slots.clone() };
                self.stage = Stage::Saving;
                Some(Action::WizardRequest(Box::new(cmd)))
            }
        }
    }

    /// 消费 `Action::WizardDone` (异步结果) 与 `Action::PickerDone` (选厂商/选接入点/选槽位模型
    /// 弹窗的结果)。`Submit`/`Save` 触发的请求走的是另一条路 (`Action::WizardRequest`, 由
    /// `App::update` 直接转成 `Cmd::Wizard`, 不经过这里——见该 action 的文档注释)。**这个方法本身
    /// 现在可能产出 `Cmd`**: `apply_wizard_result` 处理 `Created(Ok)` 时要紧接着发一次
    /// `LoadModels` (创建成功后自动拉候选模型, 不是按键触发的, 所以走这条"结果"路径而不是
    /// `WizardRequest`)。
    pub fn update(&mut self, action: &Action, store: &Store, s: &'static Strings) -> Vec<Cmd> {
        match action {
            Action::WizardDone(result) => self.apply_wizard_result(result, s),
            Action::PickerDone { tag, choice } => {
                self.apply_picker_choice(tag, choice, store, s);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// **刻意写成穷尽 `match`, 不用 `_` 兜底、也不用任何 `#[allow]`**: `WizardResult` 每加一个新
    /// 变体, 这里就必须显式接一条臂——哪怕暂时只是空臂 `=> {}`——否则编译期就 `E0004` 失败。
    ///
    /// **每个结果只在发起它的那个阶段被接受, 其余一律丢弃** (Task 5 评审, Task 6 给 `Probed` 补上
    /// 同一条守卫): 向导同一时刻最多一个请求在飞, 按 `self.stage` 判就够, 不需要 `Fetch` 那套
    /// `issued` 序号。`Providers` 只认 `Stage::Loading`; `Created` 只认 `Stage::Creating`;
    /// `Models` 只认 `Stage::LoadingModels`; `SlotsSaved` 只认 `Stage::Saving`; `Probed` 只认
    /// `Stage::Probing`。**不这样做的失败场景**: 按 `n` 打开向导 → 厂商列表还没回来就 `Esc` →
    /// 再按 `n` 重开; 旧的 `Providers` 结果晚到时, 如果不管阶段直接接受, 会把已经走到 `Slots` 的
    /// 表单打回 `Basics`——`self.providers` 也被换成旧的那一份, 草稿里的 `provider_id` 可能已经
    /// 不在这份新列表里。`Created`/`SlotsSaved` 同理: 晚到的 `Created` 会覆盖已经在用的
    /// `created_id` 并再发一次 `LoadModels`; 晚到的 `SlotsSaved` 会在错误的阶段关掉向导或弹一条
    /// 不该出现的 toast。
    ///
    /// **`Models`/`Probed` 除了阶段守卫, 还要额外核对结果自带的身份 (评审 4)**: 这两个是
    /// `WizardResult` 上方文档注释里说的"只读请求"——在飞时 `Esc` 可用, 关闭向导不会取消请求
    /// (最长等到 30 秒超时), 所以用户能退出向导 A、马上开向导 B、B 也走到同一个 `Stage`, A 的
    /// 结果这时晚到, 单靠阶段守卫拦不住。`Models { id, .. }` 与 `self.created_id` 不一致、
    /// `Probed { base_url, .. }` 与草稿当前 (trim 后) 的 `base_url` 不一致, 都整个丢弃——
    /// `LoadingModels`/`Probing` 期间表单只读, 草稿里的值就是这次请求发出时的值, 不一致必然是
    /// 另一个向导实例的结果。不这样做: 晚到的 `Probed` 会把中转 X 的候选模型记成"当前 Base URL
    /// 是 Y", 按创建时把 X 的 `models_url` 与 Y 的 base_url 一起落库; 晚到的 `Models` 会把另一家
    /// 厂商的模型名预填进当前的四个核心槽。
    ///
    /// `Providers` 成功后转进 `Stage::Basics`; `Created(Ok)` 按 `custom_draft` 是不是 `Some`
    /// 分流 (P5 Task 6, `Stage::Creating` 被两条路径共用)——内置路径记下 id、发 `LoadModels`、进
    /// `Stage::LoadingModels` (不再是"停在 `Creating` 换按钮文案"); 自定义路径槽位此刻已经是
    /// 真值, 直接关向导 + Success toast, **不**发 `LoadModels`/`SaveSlots`。`Created(Err)` 同样
    /// 按路径分流: 内置回 `Basics`, 自定义回 `Stage::Custom`, 都挂 `wiz_create_failed` 说明行。
    /// `Models` 的两个 `Ok` 分支 (`Auto`/`ManualFallback`) 与 `Err` 都进 `Stage::Slots`, 区别只在
    /// 有没有候选、有没有说明行; `Probed` 三个分支都回 `Stage::Custom`、焦点落在 `Slot(Fable)`,
    /// 区别同样只在有没有候选、有没有说明行 (`Auto` 额外记下 `ProbedModels` 供 `models_url()`
    /// 使用); `SlotsSaved(Ok)` 关掉向导 + Success toast, `SlotsSaved(Err)` 回 `Slots` 并挂说明行。
    fn apply_wizard_result(&mut self, result: &WizardResult, s: &'static Strings) -> Vec<Cmd> {
        match result {
            WizardResult::Providers(inner) => {
                if !matches!(self.stage, Stage::Loading) {
                    return Vec::new();
                }
                match inner {
                    Ok(list) => {
                        self.providers = list.clone();
                        self.stage = Stage::Basics;
                    }
                    Err(reason) => self.stage = Stage::LoadFailed(reason.clone()),
                }
                Vec::new()
            }
            WizardResult::Created(inner) => {
                if !matches!(self.stage, Stage::Creating) {
                    return Vec::new();
                }
                // P5 Task 6: `Stage::Creating` 被两条路径共用, 用 `custom_draft` 是不是 `Some`
                // 分流——只有自定义路径会设置它 (`apply_provider_choice` 进 `Stage::Custom` 时
                // 写, 全程不清空, `Stage::Creating` 期间照样是 `Some`)。
                if self.custom_draft.is_some() {
                    match inner {
                        Ok(_created) => {
                            let name = self.custom_draft.as_ref().map(|d| d.display_name.value().to_string()).unwrap_or_default();
                            self.notice = Some((ToastKind::Success, (s.wiz_created)(&name)));
                            self.close_request = true;
                        }
                        Err(e) => {
                            self.stage = Stage::Custom;
                            if let Some(draft) = &mut self.custom_draft {
                                draft.slots.note = Some((s.wiz_create_failed)(e));
                            }
                        }
                    }
                    return Vec::new();
                }
                match inner {
                    Ok(created) => {
                        self.created_id = Some(created.id.clone());
                        self.stage = Stage::LoadingModels;
                        vec![Cmd::Wizard(Box::new(WizardCmd::LoadModels { id: created.id.clone() }))]
                    }
                    Err(e) => {
                        self.stage = Stage::Basics;
                        self.create_error = Some((s.wiz_create_failed)(e));
                        Vec::new()
                    }
                }
            }
            WizardResult::Models { id, result: inner } => {
                if !matches!(self.stage, Stage::LoadingModels) {
                    return Vec::new();
                }
                // 评审 4: 只读请求自证身份, 与 `self.created_id`(发起 `LoadModels` 时用的那个)
                // 不一致就是另一个向导实例的晚到结果, 整个丢弃。
                if self.created_id.as_deref() != Some(id.as_str()) {
                    return Vec::new();
                }
                match inner {
                    Ok(RefreshModelsResult::Auto { models, .. }) => {
                        let mut slots = ModelSlots::default();
                        // 桌面端同规则: 有候选就预填四个核心槽为第一项, 用户不用把五次 ⏎ 都点
                        // 一遍才能保存——空候选就留空, 交给 `validate_slots` 在保存时拦住。
                        if let Some(first) = models.first() {
                            for slot in [Slot::Fable, Slot::Opus, Slot::Sonnet, Slot::Haiku] {
                                slots.set(slot, first.id.clone());
                            }
                        }
                        self.slots_draft = SlotsDraft { slots, models: models.clone(), note: None };
                        self.slots_form.focus = SlotsField::Row(Slot::Fable);
                        self.stage = Stage::Slots;
                        Vec::new()
                    }
                    Ok(RefreshModelsResult::ManualFallback { reason }) => self.enter_slots_with_note((s.wiz_models_manual)(reason)),
                    Err(e) => self.enter_slots_with_note((s.wiz_models_manual)(e)),
                }
            }
            WizardResult::Probed { base_url, result: inner } => {
                if !matches!(self.stage, Stage::Probing) {
                    return Vec::new();
                }
                let Some(draft) = &mut self.custom_draft else { return Vec::new() };
                // 评审 4: 只读请求自证身份——`Probing` 期间表单只读, 草稿当前 (trim 后) 的
                // `base_url` 就是这次请求发出时的值; 与结果带回来的 `base_url` 不一致, 必然是
                // 另一个向导实例的晚到探测结果, 整个丢弃。
                if draft.base_url.value().trim() != base_url.as_str() {
                    return Vec::new();
                }
                match inner {
                    Ok(ProbeModelsResult::Auto { models, models_url }) => {
                        draft.slots.models = models.clone();
                        draft.slots.note = None;
                        // 不自动预填槽位 (与桌面端一致): 自定义中转的模型名千差万别, 猜错不如
                        // 留空, 与 Task 5 内置路径"有候选就预填四个核心槽"故意不同。`base_url`
                        // 用结果里带的这个值 (已经与上面的一致性检查对齐过), 不用再重新 trim
                        // 一遍草稿。
                        draft.probe = Some(ProbedModels { base_url: base_url.clone(), models_url: models_url.clone() });
                    }
                    Ok(ProbeModelsResult::ManualFallback { reason }) => {
                        draft.slots.models = Vec::new();
                        draft.probe = None;
                        draft.slots.note = Some((s.wiz_models_manual)(reason));
                    }
                    Err(e) => {
                        draft.slots.models = Vec::new();
                        draft.probe = None;
                        draft.slots.note = Some((s.wiz_models_manual)(e));
                    }
                }
                self.custom_form.focus = CustomField::Slot(Slot::Fable);
                self.stage = Stage::Custom;
                Vec::new()
            }
            WizardResult::SlotsSaved(inner) => {
                if !matches!(self.stage, Stage::Saving) {
                    return Vec::new();
                }
                match inner {
                    Ok(()) => {
                        self.notice = Some((ToastKind::Success, (s.wiz_created)(self.draft.display_name.value())));
                        self.close_request = true;
                    }
                    Err(e) => {
                        self.stage = Stage::Slots;
                        self.slots_draft.note = Some((s.wiz_save_failed)(e));
                    }
                }
                Vec::new()
            }
        }
    }

    /// `Models(Ok(ManualFallback))` 与 `Models(Err)` 共用: 候选清空、槽位留空、挂一条说明行、
    /// 进 `Stage::Slots`, 只是说明文案的措辞由调用方算好传进来。
    fn enter_slots_with_note(&mut self, note: String) -> Vec<Cmd> {
        self.slots_draft = SlotsDraft { note: Some(note), ..SlotsDraft::default() };
        self.slots_form.focus = SlotsField::Row(Slot::Fable);
        self.stage = Stage::Slots;
        Vec::new()
    }

    /// `popup_open`: 是否有弹窗叠在向导上面 (`self.popup.is_some()`, `App::draw` 传进来)。
    /// 评审 M5: 有弹窗时向导不该再设终端光标——`Frame::set_cursor_position` 一帧只记一个坐标,
    /// 最后一次调用生效; 向导先画、弹窗后画, 如果向导设了光标而弹窗 (Confirm/Help/Detail) 自己
    /// 不设, 那个坐标就会原样留到帧尾, 光标停在被压暗的输入框里闪烁。Picker 弹窗自己会设光标,
    /// 这次调用即便被跳过也无妨 (它的设置发生在向导之后, 天然覆盖)。
    ///
    /// **刻意写成对 `stage` 的单个穷尽 `match`, 不用"提前 return + 再 match 一次"**——旧版那样写
    /// 需要在第二个 `match` 里给已经处理过的分支塞 `unreachable!()`, 是这个 crate 非测试代码
    /// 唯一的 panic 点; 以后往 `Stage` 加变体时很容易只改第一个 `matches!()` 忘了改第二个
    /// `match`, 那就会在画面上真的 panic (评审 M7)。现在每个变体各自一条臂, 加新变体编译期就会
    /// 报 E0004, 没有能被漏掉的分支。
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &mut DrawCtx, popup_open: bool) {
        let theme = ctx.theme;
        let s = ctx.s;
        match &self.stage {
            Stage::Loading => {
                let state = spinner_state(ctx.tick);
                // `to_symbol_span` 自己已经在符号后面带一个空格 (throbber-widgets-tui 的实现),
                // 这里不用再手动加一个前导空格——否则会跟总览页「重连中」那行 (`draw_tabs`) 的
                // 间距对不上, 平白多出一格。
                let spinner = Throbber::default().throbber_set(BRAILLE_SIX).to_symbol_span(&state);
                let line = Line::from(vec![spinner, Span::raw(s.wiz_loading_providers)]).centered();
                Self::draw_placeholder(frame, area, theme, s, line);
            }
            Stage::LoadFailed(reason) => {
                let line = Line::styled((s.wiz_load_failed)(reason), Style::new().fg(theme.err)).centered();
                Self::draw_placeholder(frame, area, theme, s, line);
            }
            Stage::Basics => self.draw_basics(frame, area, theme, s, ctx.tick, popup_open),
            // `Stage::Creating` 被两条路径共用 (P5 Task 6): `custom_draft` 是不是 `Some` 决定画
            // 哪张表单——与 `apply_wizard_result` 的 `Created` 分流用的是同一个判据。
            Stage::Creating => {
                if self.custom_draft.is_some() {
                    self.draw_custom(frame, area, theme, s, ctx.tick, popup_open);
                } else {
                    self.draw_basics(frame, area, theme, s, ctx.tick, popup_open);
                }
            }
            Stage::LoadingModels => self.draw_basics(frame, area, theme, s, ctx.tick, popup_open),
            Stage::Slots => self.draw_slots(frame, area, theme, s, ctx.tick, popup_open),
            Stage::Saving => self.draw_slots(frame, area, theme, s, ctx.tick, popup_open),
            Stage::Custom => self.draw_custom(frame, area, theme, s, ctx.tick, popup_open),
            Stage::Probing => self.draw_custom(frame, area, theme, s, ctx.tick, popup_open),
        }
    }

    /// `Loading` / `LoadFailed` 共用的画法 (P5 Task 2 原样保留): 带边框的空容器 + 居中一行, 由
    /// 调用方按阶段自己拼好 `line`。
    fn draw_placeholder(frame: &mut Frame, area: Rect, theme: &Theme, s: &'static Strings, line: Line<'static>) {
        let block = Block::bordered().border_type(BorderType::Rounded).border_style(theme.border_style()).title_top(format!(" {} ", s.wiz_title));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(line, inner.centered_vertically(Constraint::Length(1)));
    }

    /// `Stage::Basics` / `Stage::Creating` / `Stage::LoadingModels` 共用: 后两者只是把全部字段
    /// 画成 `locked`、按钮画成 `busy`、标签换成 `wiz_creating`/`wiz_loading_models`——三个阶段
    /// 的行结构完全一样, 拆成三份反而要重复一遍组装逻辑。
    fn draw_basics(&self, frame: &mut Frame, area: Rect, theme: &Theme, s: &'static Strings, tick: u64, popup_open: bool) {
        let busy = matches!(self.stage, Stage::Creating | Stage::LoadingModels);

        let provider_label = self.provider_label();
        let endpoint_label = self.endpoint_label();
        let api_key_text = self.draft.api_key.display();
        let pick_hint = format!("⏎ {}", s.key_pick);
        let reveal_hint = format!("Ctrl+R {}", s.key_reveal);
        // 弹窗叠在表单上面时终端光标该不该显示由 `FormView::show_cursor` 统一把关 (Task 5 起收在
        // `form::draw` 这一个关口), 这里恢复成无条件给值。
        let api_key_cursor = Some(self.draft.api_key.visual_cursor());
        let display_name_cursor = Some(self.draft.display_name.visual_cursor());
        let field = |label, value, hint, cursor, field: BasicsField| FormRow::Field {
            label,
            value,
            placeholder: "",
            hint,
            cursor,
            error: self.basics_form.error_for(field),
            locked: busy,
        };

        let mut b = FormBuilder::new();
        if let Some(err) = &self.create_error {
            b.push(FormRow::Note { text: err }, false);
            b.push(FormRow::Spacer, false);
        }
        for (label, value, hint, cursor, f) in [
            (s.wiz_f_provider, provider_label.as_str(), Some(pick_hint.as_str()), None, BasicsField::Provider),
            (s.wiz_f_endpoint, endpoint_label.as_str(), Some(pick_hint.as_str()), None, BasicsField::Endpoint),
            (s.wiz_f_api_key, api_key_text.as_str(), Some(reveal_hint.as_str()), api_key_cursor, BasicsField::ApiKey),
            (s.wiz_f_display_name, self.draft.display_name.value(), None, display_name_cursor, BasicsField::DisplayName),
        ] {
            b.push(field(label, value, hint, cursor, f), self.basics_form.focus == f);
        }
        b.push(FormRow::Spacer, false);
        // `Creating` 文案是 `wiz_creating`; `LoadingModels` (已经拿到 id, 在等
        // `refresh_model_list`) 换成 `wiz_loading_models`。
        let creating_label = if matches!(self.stage, Stage::LoadingModels) { s.wiz_loading_models } else { s.wiz_creating };
        b.push(FormRow::Button { label: if busy { creating_label } else { s.wiz_btn_next }, busy }, self.basics_form.focus == BasicsField::Submit);
        let built = b.finish();

        let view = FormView {
            title: s.wiz_title,
            steps: Some((0, s.wiz_steps.as_slice())),
            rows: &built.rows,
            focus: built.focus,
            tick,
            show_cursor: !popup_open,
        };
        // Task 8 会用这个返回值播 fx::field_err (校验失败时焦点一定落在出错的那一行)。
        let _focus_rect = form::draw(frame, area, &view, theme, s);
    }

    /// `Stage::Slots` / `Stage::Saving` 共用, 与 `draw_basics` 处理 `Basics`/`Creating` 同一个
    /// 套路: 后者只是把全部行画成 `locked`、按钮画成 `busy`、标签换成 `wiz_saving`。这一步**没有
    /// 文本行**——五个槽位都是选择行, `cursor` 恒 `None`。
    fn draw_slots(&self, frame: &mut Frame, area: Rect, theme: &Theme, s: &'static Strings, tick: u64, popup_open: bool) {
        let busy = matches!(self.stage, Stage::Saving);
        let pick_hint = format!("⏎ {}", s.key_pick);

        let mut b = FormBuilder::new();
        if let Some(note) = &self.slots_draft.note {
            b.push(FormRow::Note { text: note }, false);
            b.push(FormRow::Spacer, false);
        }
        for slot in [Slot::Fable, Slot::Opus, Slot::Sonnet, Slot::Haiku, Slot::Fallback] {
            // 兜底槽留空时画成灰字「(未配置)」(`s.sub_slot_unset`, 复用订阅详情页的字段); 四个
            // 核心槽留空时没有专门的占位提示——没填就是没填, `validate_slots` 在保存时会拦住并
            // 指到这一行。
            let placeholder = if slot == Slot::Fallback { s.sub_slot_unset } else { "" };
            b.push(
                FormRow::Field {
                    label: slot_label(slot, s),
                    value: self.slots_draft.slots.get(slot),
                    placeholder,
                    hint: Some(&pick_hint),
                    cursor: None,
                    error: self.slots_form.error_for(SlotsField::Row(slot)),
                    locked: busy,
                },
                self.slots_form.focus == SlotsField::Row(slot),
            );
        }
        b.push(FormRow::Spacer, false);
        b.push(FormRow::Button { label: if busy { s.wiz_saving } else { s.wiz_btn_save }, busy }, self.slots_form.focus == SlotsField::Save);
        let built = b.finish();

        let view = FormView {
            title: s.wiz_title,
            steps: Some((1, s.wiz_steps.as_slice())),
            rows: &built.rows,
            focus: built.focus,
            tick,
            show_cursor: !popup_open,
        };
        let _focus_rect = form::draw(frame, area, &view, theme, s);
    }

    /// `Stage::Custom` / `Stage::Creating`(自定义路径) / `Stage::Probing` 共用: 14 行固定内容
    /// (协议 / 厂商名 / Base URL / 请求路径 / 鉴权 / API Key / 备注名 / [获取模型列表] / 5 个
    /// 槽位 / [创建]) —— 与两步向导的 `draw_basics`/`draw_slots` 不同, 这里**没有 Spacer**(简报
    /// 80×24 布局逐行紧跟, 14 行内容在 20 行内容区还有富余, 不需要用空行分组)。`Auth` 行**总是
    /// 画出来**(哪怕锁定), 只是这时 `locked: true`——锁定与否不影响行数, 只影响
    /// `CustomField::all` 的焦点导航列表 (见 `handle_custom_key`)。`custom_draft` 是 `None` 时
    /// (不该发生) 什么都不画。
    fn draw_custom(&self, frame: &mut Frame, area: Rect, theme: &Theme, s: &'static Strings, tick: u64, popup_open: bool) {
        let Some(draft) = &self.custom_draft else { return };
        let probing = matches!(self.stage, Stage::Probing);
        let creating = matches!(self.stage, Stage::Creating);
        let locked_form = probing || creating;
        let locked_auth = draft.protocol.auth_locked();

        let protocol_label = Self::custom_protocol_label(draft.protocol, s);
        let auth_label = format!("{} · {}", draft.auth_header_name, Self::auth_format_label(draft.auth_header_format));
        let api_key_text = draft.api_key.display();
        let pick_hint = format!("⏎ {}", s.key_pick);
        let reveal_hint = format!("Ctrl+R {}", s.key_reveal);
        // 弹窗叠在表单上面时终端光标该不该显示由 `FormView::show_cursor` 统一把关, 这里恢复成
        // 无条件给值 (同 `draw_basics`)。
        let provider_name_cursor = Some(draft.provider_display_name.visual_cursor());
        let base_url_cursor = Some(draft.base_url.visual_cursor());
        let messages_path_cursor = Some(draft.messages_path.visual_cursor());
        let api_key_cursor = Some(draft.api_key.visual_cursor());
        let display_name_cursor = Some(draft.display_name.visual_cursor());
        let field_error = |field: CustomField| self.custom_form.error_for(field);
        let focused = |field: CustomField| self.custom_form.focus == field;
        // 除 `Auth` 外全部字段行的共同形状 (`Auth` 的 `hint`/`locked` 另外看锁定态, 单独拼)。
        let field = |label, value, placeholder, hint, cursor, field: CustomField| FormRow::Field {
            label,
            value,
            placeholder,
            hint,
            cursor,
            error: field_error(field),
            locked: locked_form,
        };

        let mut b = FormBuilder::new();
        if let Some(note) = &draft.slots.note {
            b.push(FormRow::Note { text: note }, false);
            b.push(FormRow::Spacer, false);
        }
        b.push(field(s.wiz_f_protocol, protocol_label, "", Some(&pick_hint), None, CustomField::Protocol), focused(CustomField::Protocol));
        b.push(
            field(s.wiz_f_provider_name, draft.provider_display_name.value(), "", None, provider_name_cursor, CustomField::ProviderName),
            focused(CustomField::ProviderName),
        );
        b.push(
            field(s.wiz_f_base_url, draft.base_url.value(), CUSTOM_BASE_URL_PLACEHOLDER, None, base_url_cursor, CustomField::BaseUrl),
            focused(CustomField::BaseUrl),
        );
        b.push(
            field(s.wiz_f_messages_path, draft.messages_path.value(), "", None, messages_path_cursor, CustomField::MessagesPath),
            focused(CustomField::MessagesPath),
        );
        b.push(
            FormRow::Field {
                label: s.wiz_f_auth,
                value: &auth_label,
                placeholder: "",
                hint: (!locked_auth).then_some(pick_hint.as_str()),
                cursor: None,
                error: field_error(CustomField::Auth),
                locked: locked_form || locked_auth,
            },
            focused(CustomField::Auth),
        );
        b.push(field(s.wiz_f_api_key, &api_key_text, "", Some(&reveal_hint), api_key_cursor, CustomField::ApiKey), focused(CustomField::ApiKey));
        b.push(
            field(s.wiz_f_display_name, draft.display_name.value(), "", None, display_name_cursor, CustomField::DisplayName),
            focused(CustomField::DisplayName),
        );
        b.push(FormRow::Button { label: if probing { s.wiz_probing } else { s.wiz_btn_probe }, busy: probing }, focused(CustomField::Probe));
        for slot in [Slot::Fable, Slot::Opus, Slot::Sonnet, Slot::Haiku, Slot::Fallback] {
            // 兜底槽留空时画成灰字「(未配置)」, 与 `draw_slots` 同规则。
            let placeholder = if slot == Slot::Fallback { s.sub_slot_unset } else { "" };
            b.push(
                field(slot_label(slot, s), draft.slots.slots.get(slot), placeholder, Some(&pick_hint), None, CustomField::Slot(slot)),
                focused(CustomField::Slot(slot)),
            );
        }
        b.push(FormRow::Button { label: if creating { s.wiz_creating } else { s.wiz_btn_create }, busy: creating }, focused(CustomField::Submit));
        let built = b.finish();

        // 自定义单页没有 Task 4/5 那样的两步步骤条 (`steps: None`, `FormView.steps` 的文档注释里
        // "自定义单页" 说的就是这里), 标题换成专属的 `wiz_custom_title`。
        let view =
            FormView { title: s.wiz_custom_title, steps: None, rows: &built.rows, focus: built.focus, tick, show_cursor: !popup_open };
        let _focus_rect = form::draw(frame, area, &view, theme, s);
    }

    /// 协议行自己的展示名, 与厂商 picker 里带 `自定义 · ` 前缀的 `wiz_custom_labels`(标题已经
    /// 写明「新建订阅 · 自定义」, 字段行没必要重复这个前缀) 分开成独立的 `wiz_protocol_names`
    /// 字段, **不靠剥字符串前缀算**(评审 3): 剥前缀是非测试代码里出现中文字面量, 且 P6 把
    /// `wiz_custom_labels` 翻成英文之后前缀不再匹配, 会静默显示带冗余前缀的全称——哪怕今天只是
    /// 改一下中文 label 里 `·` 两边的空格也会同样静默失效。两个数组顺序都与 `CustomProtocol::ALL`
    /// 一致, 但是两份独立的文案, 不是派生关系。
    fn custom_protocol_label(protocol: CustomProtocol, s: &'static Strings) -> &'static str {
        let idx = CustomProtocol::ALL.iter().position(|p| *p == protocol).unwrap_or(0);
        s.wiz_protocol_names[idx]
    }

    /// 鉴权格式的展示名——英文技术词汇 (与请求头名 "Authorization"/"x-api-key" 同类), 不进
    /// `Strings`。
    fn auth_format_label(format: AuthHeaderFormat) -> &'static str {
        match format {
            AuthHeaderFormat::Bearer => "Bearer",
            AuthHeaderFormat::Raw => "Raw",
        }
    }

    fn provider_label(&self) -> String {
        self.providers.iter().find(|p| p.id == self.draft.provider_id).map(|p| p.display_name.clone()).unwrap_or_default()
    }

    fn endpoint_label(&self) -> String {
        self.providers
            .iter()
            .find(|p| p.id == self.draft.provider_id)
            .and_then(|p| p.endpoints.iter().find(|e| e.id == self.draft.endpoint_id))
            .map(|e| e.label.clone())
            .unwrap_or_default()
    }

    /// 底栏左侧。`Loading`/`LoadFailed`/`Creating`/`Probing` 没有任何可操作的字段 (整张表单
    /// 只读), 留空; `Basics`/`Slots`/`Custom` 按**当前聚焦行的类型**给提示 (评审 I2: 旧版三条
    /// 提示写死不随焦点变, 在文本行上 `⏎` 实际是"下一项"却显示成"选择", 在按钮上 `⏎` 会真的调用
    /// 后端却看不出来)——`↑↓ 字段` 常驻; 选择行 (`Provider`/`Endpoint`/`Slots` 的槽位行/
    /// `Custom` 的 `Protocol`/未锁定的 `Auth`/槽位行) 追加 `⏎ 选择`; 文本行 (`ApiKey`/
    /// `DisplayName`/`Custom` 的厂商名/Base URL/请求路径/API Key/备注名) 追加 `⏎ 下一项`,
    /// `ApiKey` 再多一条 `Ctrl+R 显示/隐藏` (只在这一行有效); 按钮行 (`Submit`/`Save`/`Custom`
    /// 的 `Probe`/`Submit`) 追加 `⏎` + **按钮自己的标签**, 而不是一个通用词, 让用户一眼知道回车
    /// 会发生什么。行内的 hint (行右端「⏎ 选择」之类) 不受这条规则影响, 照旧固定。
    pub fn hints(&self, s: &'static Strings) -> Vec<Hint<'static>> {
        match &self.stage {
            Stage::Loading | Stage::LoadFailed(_) | Stage::Creating | Stage::LoadingModels | Stage::Saving | Stage::Probing => Vec::new(),
            Stage::Basics => {
                let mut hints = vec![("↑↓", s.key_field)];
                match self.basics_form.focus {
                    BasicsField::Provider | BasicsField::Endpoint => hints.push(("⏎", s.key_pick)),
                    BasicsField::ApiKey => {
                        hints.push(("⏎", s.key_next_field));
                        hints.push(("Ctrl+R", s.key_reveal));
                    }
                    BasicsField::DisplayName => hints.push(("⏎", s.key_next_field)),
                    BasicsField::Submit => hints.push(("⏎", s.wiz_btn_next)),
                }
                hints
            }
            Stage::Slots => {
                let mut hints = vec![("↑↓", s.key_field)];
                match self.slots_form.focus {
                    SlotsField::Row(_) => hints.push(("⏎", s.key_pick)),
                    SlotsField::Save => hints.push(("⏎", s.wiz_btn_save)),
                }
                hints
            }
            Stage::Custom => {
                let mut hints = vec![("↑↓", s.key_field)];
                let locked = self.custom_draft.as_ref().map(|d| d.protocol.auth_locked()).unwrap_or(false);
                match self.custom_form.focus {
                    CustomField::Protocol => hints.push(("⏎", s.key_pick)),
                    CustomField::Auth if !locked => hints.push(("⏎", s.key_pick)),
                    CustomField::Auth => {} // 锁定态理论不会被聚焦到, 防御性地不给提示。
                    CustomField::ProviderName | CustomField::BaseUrl | CustomField::MessagesPath | CustomField::DisplayName => {
                        hints.push(("⏎", s.key_next_field));
                    }
                    CustomField::ApiKey => {
                        hints.push(("⏎", s.key_next_field));
                        hints.push(("Ctrl+R", s.key_reveal));
                    }
                    CustomField::Probe => hints.push(("⏎", s.wiz_btn_probe)),
                    CustomField::Slot(_) => hints.push(("⏎", s.key_pick)),
                    CustomField::Submit => hints.push(("⏎", s.wiz_btn_create)),
                }
                hints
            }
        }
    }

    /// 请求在飞时能不能按 `Esc` 退出——`App::draw` 据此决定要不要在底栏右侧显示 `Esc 取消`。
    /// 只有**会落库**的请求 (`Creating`/`Saving`) 在飞时才吞 `Esc` (评审 M9: 撤不回后端落库,
    /// 继续显示这条提示就是纯误导); `LoadingModels`/`Probing` 等的是只读请求 (`refresh_model_list`
    /// / `probe_custom_models`), 不落库, 所以能取消 (评审收窄, 见 `Stage::LoadingModels` 的文档
    /// 注释——`Probing` 是 P5 Task 6 同理的第二个只读"在飞"阶段)。
    pub fn can_cancel(&self) -> bool {
        !matches!(self.stage, Stage::Creating | Stage::Saving)
    }

    /// 用户已经填过东西 / 已经创建过订阅 —— `Esc` 要不要先确认看这个。`Basics` 下「厂商已选」
    /// 或「API Key 非空」或「备注名非空」任一成立即为真 (用户填了一半按 `Esc` 不该直接丢掉);
    /// `Creating`/`LoadingModels`/`Slots`/`Saving` 恒真 (订阅已经在飞行中创建, 或者已经建好了);
    /// `Custom`/`Probing` 同样恒真——能进这两个阶段本身就意味着用户已经从厂商 picker 里选中了
    /// 一个 `custom:<protocol>` 条目 (与 `Basics` 里「厂商已选」是同一件事), 不存在"刚进来什么
    /// 都没做"的状态。
    pub fn has_input(&self) -> bool {
        match &self.stage {
            Stage::Loading | Stage::LoadFailed(_) => false,
            Stage::Basics => {
                !self.draft.provider_id.is_empty() || !self.draft.api_key.is_empty() || !self.draft.display_name.value().trim().is_empty()
            }
            Stage::Creating | Stage::LoadingModels | Stage::Slots | Stage::Saving | Stage::Custom | Stage::Probing => true,
        }
    }

    /// 与页面同一套: `update()` 内部想弹 toast 就存这里, `App` 调用后轮询取走。
    pub fn take_notice(&mut self) -> Option<(ToastKind, String)> {
        self.notice.take()
    }

    /// 向导在处理**结果**时想关掉自己 (保存成功 / 创建成功)。`update()` 的签名只能返回
    /// `Vec<Cmd>`, 塞不进一个 `Action::CloseWizard` —— 与 `take_notice` 同一条出路: 存在这里,
    /// `App` 调用 `update()` 之后轮询一次。`true` 表示"关掉我", 取走即清零。
    pub fn take_close_request(&mut self) -> bool {
        std::mem::take(&mut self.close_request)
    }

    /// 测试专用: 直接置位「向导想关闭自己」。写法照抄 `pages::logs::Logs::set_force_dirty`。
    #[cfg(test)]
    pub fn request_close_for_test(&mut self) {
        self.close_request = true;
    }

    /// 测试专用: 直接塞一条待发的 notice。`App::update_wizard` 转发 `take_notice()` 的逻辑
    /// (Review round 1) 需要一个能从外面戳进 notice 的入口。
    #[cfg(test)]
    pub fn request_notice_for_test(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.notice = Some((kind, text.into()));
    }
}

/// 文本行按键的**唯一**编辑路径 (内置 / 自定义两张表单共用): `field` 是按焦点找到的那一格
/// (`BasicsDraft::text_field` / `CustomDraft::text_field`, 焦点不在文本行上时是 `None`, 按键被吞掉),
/// 交给它处理; 值真的变了就清掉**这个字段自己**的校验错误 (评审 M3, 见 `FormState::clear`)。返回
/// 值是否改变——自定义表单改了厂商名还要让备注名跟随。
fn edit_focused_text<F: Copy + Eq>(field: Option<&mut dyn TextInput>, form: &mut FormState<F>, key: KeyEvent) -> bool {
    let Some(field) = field else { return false };
    if !field.handle(key) {
        return false;
    }
    form.clear(form.focus);
    true
}

/// 备注名跟着厂商名自动生成——内置路径 (`apply_provider_choice` 选中厂商) 与自定义路径 (编辑厂商
/// 名, 评审 7 拍板的一致性改动) 共用这一个函数: 备注名为空、**或**仍等于上一次自动生成的值时,
/// 重算成 `default_display_name(source)` 并记下来; 用户手改过之后不再跟随。返回是否真的重算了
/// (调用方据此清掉备注名行自己的错误)。
fn follow_display_name(display_name: &mut TextField, last_auto: &mut Option<String>, source: &str, store: &Store) -> bool {
    let still_auto = display_name.value().is_empty() || last_auto.as_deref() == Some(display_name.value());
    if !still_auto {
        return false;
    }
    let generated = default_display_name(source, store);
    display_name.set(generated.clone());
    *last_auto = Some(generated);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::dto::{CreatedSubscription, ModelInfo};

    fn provider(id: &str) -> Provider {
        Provider {
            id: id.to_string(),
            display_name: id.to_string(),
            description: None,
            endpoints: vec![],
            default_endpoint: None,
            auth: crate::client::dto::ProviderAuth { auth_type: "api_key".into() },
            model_discovery: crate::client::dto::ModelDiscovery { enabled: true, example_models: vec![] },
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, ratatui::crossterm::event::KeyModifiers::NONE)
    }

    #[test]
    fn on_open_requests_the_provider_list() {
        let mut w = Wizard::new();
        assert_eq!(w.on_open(), vec![Cmd::Wizard(Box::new(WizardCmd::LoadProviders))]);
    }

    #[test]
    fn has_no_input_yet_so_escape_closes_without_confirming() {
        let mut w = Wizard::new();
        assert!(!w.has_input());
        assert_eq!(w.handle_key(key(KeyCode::Esc), &Store::default(), &crate::i18n::ZH), Some(Action::CloseWizard));
    }

    /// 除 `Esc` 外的按键在 `Stage::Loading` 一律被吞掉——没有任何字段可以接收字符输入 (`Basics`
    /// 起才有, 见 `wizard_basics_snapshot_80x24` 等 `tests/ui.rs` 里的用例)。
    #[test]
    fn other_keys_are_swallowed() {
        let mut w = Wizard::new();
        for code in [KeyCode::Char('q'), KeyCode::Char('r'), KeyCode::Char('1'), KeyCode::Tab, KeyCode::Enter] {
            assert_eq!(w.handle_key(key(code), &Store::default(), &crate::i18n::ZH), None, "{code:?}");
        }
    }

    #[test]
    fn a_successful_provider_list_is_recorded_quietly() {
        let mut w = Wizard::new();
        let action = Action::WizardDone(Box::new(WizardResult::Providers(Ok(vec![provider("zhipu")]))));
        let cmds = w.update(&action, &Store::default(), &crate::i18n::ZH);
        assert!(cmds.is_empty());
        assert!(w.take_notice().is_none());
        assert!(!w.take_close_request());
        // 进了 `Stage::Basics`, 但草稿还是空的 (`has_input()` 仍然为假), Esc 照常直接关闭。
        assert_eq!(w.handle_key(key(KeyCode::Esc), &Store::default(), &crate::i18n::ZH), Some(Action::CloseWizard));
    }

    #[test]
    fn a_failed_provider_list_does_not_produce_a_command_notice_or_close_request() {
        let mut w = Wizard::new();
        let action = Action::WizardDone(Box::new(WizardResult::Providers(Err("network".into()))));
        let cmds = w.update(&action, &Store::default(), &crate::i18n::ZH);
        assert!(cmds.is_empty());
        assert!(w.take_notice().is_none());
        assert!(!w.take_close_request());
    }

    #[test]
    fn request_close_for_test_is_taken_exactly_once() {
        let mut w = Wizard::new();
        assert!(!w.take_close_request());
        w.request_close_for_test();
        assert!(w.take_close_request());
        assert!(!w.take_close_request(), "取走之后应该清零");
    }

    /// 评审 M9: 请求在飞 (`Stage::Creating`) 时连 `Esc` 也该被吞掉, 不弹确认——直接摆 `stage`
    /// (私有字段, 与本文件同一个 `mod`) 比走完整个选厂商流程更直接, 只验证这一条单点行为。
    #[test]
    fn escape_is_swallowed_while_a_request_is_in_flight() {
        let mut w = Wizard::new();
        w.stage = Stage::Creating;
        assert!(!w.can_cancel(), "在飞时不该能取消");
        assert_eq!(w.handle_key(key(KeyCode::Esc), &Store::default(), &crate::i18n::ZH), None, "在飞时 Esc 应该被吞掉");
    }

    /// 同上, `Stage::Saving` 是新加的另一个"在飞"阶段, 应该同样吞掉 `Esc`。
    #[test]
    fn escape_is_swallowed_while_saving() {
        let mut w = Wizard::new();
        w.stage = Stage::Saving;
        assert!(!w.can_cancel(), "保存在飞时不该能取消");
        assert_eq!(w.handle_key(key(KeyCode::Esc), &Store::default(), &crate::i18n::ZH), None, "保存在飞时 Esc 应该被吞掉");
    }

    /// P5 Task 5 前置项 2: 底栏提示按焦点行的类型变化, 四个分支 (选择行 / API Key 文本行 / 备注名
    /// 文本行 / 按钮行) 都要断言到——之前只有 API Key 那一支靠 `wizard_basics_80x24` 快照间接钉住。
    /// 按钮行要断言出现的是**按钮自己的标签**, 不是一个通用词。
    #[test]
    fn hints_follow_focus_on_every_basics_row_type() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Basics;

        w.basics_form.focus = BasicsField::Provider;
        assert_eq!(w.hints(s), vec![("↑↓", s.key_field), ("⏎", s.key_pick)], "选择行应该提示 ⏎ 选择");

        w.basics_form.focus = BasicsField::ApiKey;
        assert_eq!(
            w.hints(s),
            vec![("↑↓", s.key_field), ("⏎", s.key_next_field), ("Ctrl+R", s.key_reveal)],
            "API Key 行额外带 Ctrl+R 提示"
        );

        w.basics_form.focus = BasicsField::DisplayName;
        let hints = w.hints(s);
        assert_eq!(hints, vec![("↑↓", s.key_field), ("⏎", s.key_next_field)], "备注名行是文本行, 但不该有 Ctrl+R 提示");
        assert!(!hints.iter().any(|(k, _)| *k == "Ctrl+R"), "备注名行不该出现 Ctrl+R\n{hints:?}");

        w.basics_form.focus = BasicsField::Submit;
        assert_eq!(w.hints(s), vec![("↑↓", s.key_field), ("⏎", s.wiz_btn_next)], "按钮行应该显示按钮自己的标签, 不是通用词");
    }

    /// 同上, 覆盖 `Stage::Slots` 新增的两支: 槽位行 (选择行) 与 `Save` 按钮行。
    #[test]
    fn hints_follow_focus_on_every_slots_row_type() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Slots;

        w.slots_form.focus = SlotsField::Row(Slot::Fable);
        assert_eq!(w.hints(s), vec![("↑↓", s.key_field), ("⏎", s.key_pick)], "槽位行应该提示 ⏎ 选择");

        w.slots_form.focus = SlotsField::Save;
        assert_eq!(w.hints(s), vec![("↑↓", s.key_field), ("⏎", s.wiz_btn_save)], "保存按钮行应该显示它自己的标签");
    }

    /// P5 Task 5 前置项 3: 编辑字段只清自己的错误——负向用例 (Task 4 只有正向: 编辑同一个字段清
    /// 掉自己的错误; 错误是单个 `Option`, 退化成"任何编辑都清"时那条正向测试照样能过,
    /// 必须补一条编辑别的字段、断言原字段错误还在的用例才咬得住这个回归)。
    #[test]
    fn editing_one_field_does_not_clear_another_fields_error() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Basics;
        w.basics_form.error = Some((BasicsField::Provider, s.wiz_err_provider));
        w.basics_form.focus = BasicsField::DisplayName;

        w.handle_key(key(KeyCode::Char('a')), &Store::default(), s);

        assert_eq!(w.basics_form.error, Some((BasicsField::Provider, s.wiz_err_provider)), "编辑备注名不该清掉厂商行的错误");
    }

    /// 7R-a: 统一编辑路径的正向用例 (两张表单各一条)——只移动光标不算编辑, 错误留着; 值真的变了
    /// 才清掉这个字段自己的错误。
    #[test]
    fn editing_a_text_field_clears_its_own_error_only_when_the_value_changes() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Basics;
        w.basics_form.reject(BasicsField::ApiKey, s.wiz_err_api_key);
        w.handle_key(key(KeyCode::Left), &Store::default(), s);
        assert_eq!(w.basics_form.error, Some((BasicsField::ApiKey, s.wiz_err_api_key)), "只移动光标不该清错误");
        w.handle_key(key(KeyCode::Char('x')), &Store::default(), s);
        assert_eq!(w.basics_form.error, None, "给 API Key 打字应该清掉它自己的错误");

        let mut c = Wizard::new();
        c.stage = Stage::Custom;
        c.custom_draft = Some(CustomDraft::new(CustomProtocol::Anthropic));
        c.custom_form.reject(CustomField::BaseUrl, s.wiz_err_base_url_empty);
        c.handle_key(key(KeyCode::Left), &Store::default(), s);
        assert_eq!(c.custom_form.error, Some((CustomField::BaseUrl, s.wiz_err_base_url_empty)), "只移动光标不该清错误");
        c.handle_key(key(KeyCode::Char('h')), &Store::default(), s);
        assert_eq!(c.custom_form.error, None, "给 Base URL 打字应该清掉它自己的错误");
    }

    /// 同上, `Stage::Slots` 里槽位错误的负向用例: 给 `Fable` 挂错误, 选 `Opus` 的模型, `Fable`
    /// 的错误应该原封不动。
    #[test]
    fn selecting_one_slot_does_not_clear_another_slots_error() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Slots;
        w.slots_form.error = Some((SlotsField::Row(Slot::Fable), s.wiz_err_slot));

        w.apply_slot_choice(Slot::Opus, &PickerChoice::Item("glm-4.6".into()));

        assert_eq!(w.slots_form.error, Some((SlotsField::Row(Slot::Fable), s.wiz_err_slot)), "选定 Opus 的模型不该清掉 Fable 行的错误");
    }

    /// Item 3: `Stage::Basics` 下 `Esc` (有输入时) 用的是 `confirm_discard`, 不是
    /// `Stage::Slots`/`LoadingModels` 专用的 `wiz_confirm_exit_pending`——两者语义不同 (第一步
    /// 订阅还没建, "放弃" 只是丢草稿; 用错文案会让用户以为已经建出了一条订阅)。
    #[test]
    fn basics_escape_uses_the_discard_prompt_not_the_pending_one() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Basics;
        w.draft.provider_id = "zhipu".into(); // has_input() 为真
        assert_eq!(
            w.handle_key(key(KeyCode::Esc), &Store::default(), s),
            Some(Action::OpenConfirm { prompt: s.confirm_discard.to_string(), on_yes: Box::new(Action::CloseWizard) })
        );
    }

    /// 评审收窄 M9: `LoadingModels` 等的是只读请求 (`refresh_model_list`, 不落库), `Esc` 应该
    /// 可用, 且文案是 `wiz_confirm_exit_pending` (订阅已经建好了, 与在 `Slots` 退出是一回事)。
    #[test]
    fn escape_is_available_while_loading_models() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::LoadingModels;
        w.created_id = Some("sub-1".into());
        assert!(w.can_cancel(), "等模型列表时应该能取消");
        assert_eq!(
            w.handle_key(key(KeyCode::Esc), &Store::default(), s),
            Some(Action::OpenConfirm { prompt: s.wiz_confirm_exit_pending.to_string(), on_yes: Box::new(Action::CloseWizard) })
        );
    }

    /// 评审: 每个异步结果只在发起它的那个阶段被接受, 其余一律丢弃——4 个变体各一条用例, 构造
    /// 一个"晚到"的结果喂进一个不对的阶段, 断言阶段/字段/`created_id` 都不变, 也不产出 `Cmd`。
    ///
    /// `Providers` 晚到 (向导已经走到 `Slots`): 不该把表单打回 `Basics`, 也不该采纳这份厂商
    /// 列表——它可能是过期的, `self.providers` 应该原样保留。
    #[test]
    fn a_stale_provider_list_is_discarded_outside_loading() {
        let mut w = Wizard::new();
        w.stage = Stage::Slots;
        w.slots_form.focus = SlotsField::Save;
        let action = Action::WizardDone(Box::new(WizardResult::Providers(Ok(vec![provider("late")]))));
        let cmds = w.update(&action, &Store::default(), &crate::i18n::ZH);
        assert!(cmds.is_empty());
        assert!(w.providers.is_empty(), "过期的厂商列表不该被采纳\n{:?}", w.providers);
        assert!(matches!(w.stage, Stage::Slots), "阶段不该被晚到的厂商列表打回 Basics");
    }

    /// `Created` 晚到 (已经在 `Slots`, `created_id` 已经是真实值): 不该覆盖 `created_id`, 也
    /// 不该再发一次 `LoadModels`。
    #[test]
    fn a_stale_created_result_is_discarded_outside_creating() {
        let mut w = Wizard::new();
        w.stage = Stage::Slots;
        w.created_id = Some("real-id".into());
        let action = Action::WizardDone(Box::new(WizardResult::Created(Ok(CreatedSubscription { id: "stale-id".into() }))));
        let cmds = w.update(&action, &Store::default(), &crate::i18n::ZH);
        assert!(cmds.is_empty(), "不该再发一次 LoadModels");
        assert_eq!(w.created_id, Some("real-id".into()), "created_id 不该被晚到的结果覆盖");
        assert!(matches!(w.stage, Stage::Slots));
    }

    /// `Models` 晚到 (`created_id` 还是 `None`, 阶段还停在 `Creating`, 不是 `LoadingModels`):
    /// 不该凭空进 `Slots`——那样保存时 `submit_slots` 拿不到 `created_id`, 会变成无声的空操作。
    #[test]
    fn a_stale_models_result_is_discarded_outside_loading_models() {
        let mut w = Wizard::new();
        w.stage = Stage::Creating;
        let action = Action::WizardDone(Box::new(WizardResult::Models {
            id: "sub-1".into(),
            result: Ok(RefreshModelsResult::Auto { models: vec![ModelInfo { id: "glm-4.6".into(), display_name: None }], fetched_at: 0 }),
        }));
        let cmds = w.update(&action, &Store::default(), &crate::i18n::ZH);
        assert!(cmds.is_empty());
        assert!(matches!(w.stage, Stage::Creating), "阶段不该被晚到的 Models 结果打进 Slots");
        assert_eq!(w.slots_draft, SlotsDraft::default(), "槽位草稿不该被写入");
    }

    /// `SlotsSaved` 晚到 (阶段已经不是 `Saving`): 不该关向导、不该弹 toast。
    #[test]
    fn a_stale_slots_saved_result_is_discarded_outside_saving() {
        let mut w = Wizard::new();
        w.stage = Stage::Slots;
        let action = Action::WizardDone(Box::new(WizardResult::SlotsSaved(Ok(()))));
        let cmds = w.update(&action, &Store::default(), &crate::i18n::ZH);
        assert!(cmds.is_empty());
        assert!(!w.take_close_request(), "不该关向导");
        assert!(w.take_notice().is_none(), "不该弹 toast");
        assert!(matches!(w.stage, Stage::Slots));
    }
}
