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
//! - `Esc` = 退出向导 (`has_input()` 为真时先弹确认)——**除非请求正在飞** (`can_cancel()` 为
//!   假): 那种情况下连 `Esc` 也吞掉, 整张表单只读直到结果回来 (评审 M9 裁决; `Ctrl+C` 仍能强退,
//!   它在 `App` 层, 向导拦不住), 按钮行显示 throbber。
//!
//! P5 Task 2 只搭了骨架 (`Stage::Loading` / `LoadFailed`, 拉厂商列表、画一个加载中/失败的空容器、
//! `Esc` 退出)。**Task 4 起加真正的表单**: 内置厂商路径的第一步 (`Stage::Basics` / `Creating`)——
//! 选厂商 → 选接入点 → 填 API Key → 备注名 → 下一步。**Task 5 加第二步** (绑定模型,
//! `Stage::Slots` / `Saving`)——创建成功后拉候选模型 → 五个槽位选模型 → 保存, 向导里**不**设置
//! reasoning effort (与 spec §5.4 的偏离, 见 `WizardCmd::SaveSlots` 的文档注释: 四个槽全 auto,
//! 用户创建完在订阅详情页按 `o` 就能改)。自定义厂商的单页表单 (`Stage::Custom` / `Probing`)
//! 留给 Task 6。

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ratatui::Frame;
use throbber_widgets_tui::{Throbber, BRAILLE_SIX};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::action::{Action, Cmd, WizardCmd, WizardResult};
use crate::client::dto::{CreateInput, CreateSource, CustomProtocol, ModelSlots, Provider, RefreshModelsResult, Slot};
use crate::i18n::Strings;
use crate::pages::subscriptions::slot_label;
use crate::pages::DrawCtx;
use crate::secret::Secret;
use crate::store::Store;
use crate::theme::Theme;
use crate::widgets::form::{self, FormRow, FormView};
use crate::widgets::keybar::Hint;
use crate::widgets::picker::{PickerChoice, PickerItem, PickerSpec, PickerTag};
use crate::widgets::spinner_state;
use crate::widgets::toast::ToastKind;

mod fields;
use fields::{api_key_display, default_display_name, validate_basics, validate_slots, BasicsDraft, BasicsField, SlotsDraft, SlotsField};

/// 向导走到哪一步了。P5 Task 2 只有前两个, Task 4 加了 `Basics`/`Creating`, Task 5 加了
/// `Slots`/`Saving` (穷尽 `match`, 加了不接住就编译失败), Task 6 继续往里加 `Custom`/`Probing`。
enum Stage {
    /// 正在拉厂商列表。
    Loading,
    /// 拉失败了, 表单画不出来, 只能 `Esc` 退出。
    LoadFailed(String),
    /// 内置路径第一步: 选厂商 / 选接入点 / 填 API Key / 备注名。
    Basics,
    /// `create_subscription` 在飞: 表单只读, 按钮转圈。`Created(Ok)` 落地后不切换 stage (订阅
    /// 已经建好了, 但还在等 `refresh_model_list` 回来), 只是按钮文案从 `wiz_creating` 换成
    /// `wiz_loading_models`——不新增一个变体表示这个过渡态, 复用同一条"表单只读、按钮转圈"逻辑。
    Creating,
    /// 第二步: 五个槽位选模型, `Save` 触发 `SaveSlots`。
    Slots,
    /// `SaveSlots` (只带 `model_slots` 的 patch, 向导不设置 effort) 在飞: 表单只读, 按钮转圈。
    Saving,
    // Task 6: Custom / Probing
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
    /// `Stage::Basics` 的草稿。选中自定义厂商条目 / OAuth 厂商都不会碰这个字段——只有选中一个
    /// 可用的内置厂商才会写 `provider_id`/`endpoint_id` (P5 Task 4)。
    draft: BasicsDraft,
    /// 当前聚焦的字段, `Wizard::new()` 从 `BasicsField::Provider` 起步。
    focus: BasicsField,
    /// API Key 输入框的编辑状态 (光标位置等); `draft.api_key` 是它的 `Secret` 投影, 每次编辑
    /// (`handle_event` 认为值真的变了) 之后同步——与 `display_name_input` 同一条道理, 见
    /// `BasicsDraft` 的文档注释 (`tui_input::Input` 不参与 `PartialEq`, 草稿只存文本)。
    api_key_input: Input,
    /// 备注名输入框的编辑状态; `draft.display_name` 是它的 `String` 投影。
    display_name_input: Input,
    /// API Key 行是否显示明文 (`Ctrl+R` 切换, 只影响显示, 不影响 `draft.api_key` 本身)。
    reveal_api_key: bool,
    /// 上一次 `apply_provider_choice` 自动算出来的备注名——据此判断用户是不是已经手动改过它
    /// (改过就不再跟着厂商切换重算), 见 `fields::default_display_name` 的调用点。
    last_auto_display_name: Option<String>,
    /// 校验失败的字段与原因; `Submit` 按下但没通过时写入, 画成那一行下面的 `⚠` 提示。
    field_error: Option<(BasicsField, &'static str)>,
    /// `create_subscription` 失败时的原因, 挂成表单顶部的说明行 (`FormRow::Note`)。
    create_error: Option<String>,
    /// `Created(Ok(id))` 落地时记下的订阅 id, 供 `LoadModels`/`SaveSlots` 使用 (Task 5)。
    /// `Stage::Slots`/`Saving` 期间恒为 `Some`——由 `apply_wizard_result` 保证。
    created_id: Option<String>,
    /// `Stage::Slots` 的草稿: 五个槽位的值 + 拉到的候选模型 + 说明行。
    slots_draft: SlotsDraft,
    /// `Stage::Slots` 当前聚焦的字段, 从 `SlotsField::Row(Slot::Fable)` 起步。
    slots_focus: SlotsField,
    /// 槽位校验失败的字段与原因; `Save` 按下但没通过时写入, 与 `field_error` 同一条道理。
    slot_error: Option<(Slot, &'static str)>,
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
            focus: BasicsField::Provider,
            api_key_input: Input::default(),
            display_name_input: Input::default(),
            reveal_api_key: false,
            last_auto_display_name: None,
            field_error: None,
            create_error: None,
            created_id: None,
            slots_draft: SlotsDraft::default(),
            slots_focus: SlotsField::Row(Slot::Fable),
            slot_error: None,
        }
    }

    /// 刚打开: 要发的请求 (拉厂商列表)。`App` 在创建它之后立刻调一次。
    pub fn on_open(&mut self) -> Vec<Cmd> {
        vec![Cmd::Wizard(Box::new(WizardCmd::LoadProviders))]
    }

    /// 除 `Ctrl+C` 外的全部按键。`None` = 吞掉 (或只改了向导自己的状态)。`Esc` 的处理跟阶段
    /// 无关 (`can_cancel()` 为假时连 `Esc` 也不接), 排在最前面统一判断; 其余按键只有
    /// `Stage::Basics`/`Slots` 才会真的处理——`Loading` / `LoadFailed` 没有字段可以接收输入,
    /// `Creating`/`Saving` 整张表单只读 (总规则: 有请求在飞时按键全部吞掉, 评审 M9 起连 `Esc`
    /// 也不例外)。**两种文案**: 还没创建 (`Basics`) 复用既有的 `confirm_discard`; 订阅已经建好、
    /// 停在 `Slots` 时换成 `wiz_confirm_exit_pending` (退出会留下带 (pending) 槽位的订阅, 跟
    /// "放弃未保存的编辑" 不是同一件事)。
    pub fn handle_key(&mut self, key: KeyEvent, s: &'static Strings) -> Option<Action> {
        if key.code == KeyCode::Esc && self.can_cancel() {
            if !self.has_input() {
                return Some(Action::CloseWizard);
            }
            let prompt = if matches!(self.stage, Stage::Slots) { s.wiz_confirm_exit_pending } else { s.confirm_discard };
            return Some(Action::OpenConfirm { prompt: prompt.to_string(), on_yes: Box::new(Action::CloseWizard) });
        }
        if matches!(self.stage, Stage::Basics) {
            self.handle_basics_key(key, s)
        } else if matches!(self.stage, Stage::Slots) {
            self.handle_slots_key(key, s)
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
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.move_focus(-1);
                None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.move_focus(1);
                None
            }
            KeyCode::Enter if self.focus == BasicsField::Provider => Some(self.open_provider_picker(s)),
            KeyCode::Enter if self.focus == BasicsField::Endpoint => Some(self.open_endpoint_picker(s)),
            KeyCode::Enter if matches!(self.focus, BasicsField::ApiKey | BasicsField::DisplayName) => {
                self.move_focus(1);
                None
            }
            KeyCode::Enter if self.focus == BasicsField::Submit => self.submit(s),
            // 必须排在下面两条打字分支之前: 否则 Ctrl+R 会被 `ApiKey` 的编辑分支当成字符 'r' 吃掉。
            KeyCode::Char('r') if ctrl && self.focus == BasicsField::ApiKey => {
                self.reveal_api_key = !self.reveal_api_key;
                None
            }
            _ if self.focus == BasicsField::ApiKey => {
                self.edit_api_key(key);
                None
            }
            _ if self.focus == BasicsField::DisplayName => {
                self.edit_display_name(key);
                None
            }
            // 选择行 (`Provider`/`Endpoint`) 上除了上面已经接住的 `⏎`, 其余按键 (含字符键) 一律
            // 吞掉——"选择行不能直接打字"。
            _ => None,
        }
    }

    fn move_focus(&mut self, delta: isize) {
        let idx = BasicsField::ALL.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = (idx as isize + delta).clamp(0, BasicsField::ALL.len() as isize - 1) as usize;
        self.focus = BasicsField::ALL[next];
    }

    /// `Stage::Slots` 的按键表 (总规则同 `Basics`): `↑`/`↓`/`Tab`/`BackTab` 在 6 个可聚焦字段
    /// 之间移动 (5 个槽位行 + `Save` 按钮); 槽位行是选择行, `⏎` 开模型 picker; `Save` 是按钮行,
    /// `⏎` 触发校验 + 提交——与 `Basics` 不同的是这一步**没有文本行**, 全部字段都要么是选择行
    /// 要么是按钮行, 所以不需要区分"打字"分支。
    fn handle_slots_key(&mut self, key: KeyEvent, s: &'static Strings) -> Option<Action> {
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.move_slots_focus(-1);
                None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.move_slots_focus(1);
                None
            }
            KeyCode::Enter => match self.slots_focus {
                SlotsField::Row(slot) => Some(self.open_slot_picker(slot, s)),
                SlotsField::Save => self.submit_slots(s),
            },
            _ => None,
        }
    }

    fn move_slots_focus(&mut self, delta: isize) {
        let idx = SlotsField::ALL.iter().position(|f| *f == self.slots_focus).unwrap_or(0);
        let next = (idx as isize + delta).clamp(0, SlotsField::ALL.len() as isize - 1) as usize;
        self.slots_focus = SlotsField::ALL[next];
    }

    /// 编辑成功 (值真的变了) 就清掉**这个字段自己**的校验错误 (评审 M3)——别的字段的错误不动,
    /// 不然改一个字段会把提交时挂在另一个字段上的提示也一并抹掉, 反而让用户以为它也修好了。
    fn clear_field_error(&mut self, field: BasicsField) {
        if self.field_error.is_some_and(|(f, _)| f == field) {
            self.field_error = None;
        }
    }

    fn edit_api_key(&mut self, key: KeyEvent) {
        if self.api_key_input.handle_event(&Event::Key(key)).is_some_and(|changed| changed.value) {
            self.draft.api_key = Secret::new(self.api_key_input.value());
            self.clear_field_error(BasicsField::ApiKey);
        }
    }

    fn edit_display_name(&mut self, key: KeyEvent) {
        if self.display_name_input.handle_event(&Event::Key(key)).is_some_and(|changed| changed.value) {
            self.draft.display_name = self.display_name_input.value().to_string();
            self.clear_field_error(BasicsField::DisplayName);
        }
    }

    /// 与 `clear_field_error` 同一条道理, 只是作用于 `Stage::Slots` 自己的槽位错误。
    fn clear_slot_error(&mut self, slot: Slot) {
        if self.slot_error.is_some_and(|(sl, _)| sl == slot) {
            self.slot_error = None;
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

    /// `Action::PickerDone` 落地: 按 `tag` 分派给厂商 / 接入点 / 槽位三条分支; 跟向导无关的 tag
    /// (其它页面自己的弹窗) 直接忽略——**穷尽 `match`**, 新增 `PickerTag` 变体时这里会编译失败,
    /// 逼着显式决定向导要不要关心它。
    fn apply_picker_choice(&mut self, tag: &PickerTag, choice: &PickerChoice, store: &Store, s: &'static Strings) {
        match tag {
            PickerTag::WizardProvider => self.apply_provider_choice(choice, store, s),
            PickerTag::WizardEndpoint => self.apply_endpoint_choice(choice),
            PickerTag::WizardSlot { slot } => self.apply_slot_choice(*slot, choice),
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
    /// 察觉。选中自定义条目 → 进 Task 6 的 `Stage::Custom` (本 Task 先弹一条 `wiz_custom_todo`
    /// 的 Info toast 占位, Task 6 替换掉这一行并删掉这个字段)。选中 OAuth 厂商 → 不设值, 只弹
    /// `wiz_desktop_only`。
    fn apply_provider_choice(&mut self, choice: &PickerChoice, store: &Store, s: &'static Strings) {
        // `allow_custom: false`: picker 理论上不会产出 `Custom`, 防御性地忽略。
        let PickerChoice::Item(id) = choice else { return };
        if let Some(wire) = id.strip_prefix("custom:") {
            let _ = wire; // Task 6 会解析回 `CustomProtocol` 并进 `Stage::Custom`; 本 Task 只占位。
            self.notice = Some((ToastKind::Info, s.wiz_custom_todo.to_string()));
            return;
        }
        let Some(provider) = self.providers.iter().find(|p| &p.id == id) else { return };
        if provider.is_oauth() {
            self.notice = Some((ToastKind::Info, s.wiz_desktop_only.to_string()));
            return;
        }
        // 把后面要用的字段先拷成拥有所有权的值——`provider` 借用着 `self.providers`, 下面几行都
        // 要调 `&mut self` 的方法 (`clear_field_error` 等), 两者不能同时活着。
        let provider_id = provider.id.clone();
        let provider_display_name = provider.display_name.clone();
        let default_endpoint_id = provider.default_endpoint().map(|e| e.id.clone()).unwrap_or_default();

        // 选中了一个合法的内置厂商 (不管是不是重选同一个), 这个字段本身就算通过了, 先清错误
        // (评审 M3)。
        self.clear_field_error(BasicsField::Provider);
        if provider_id == self.draft.provider_id {
            return; // M2: 重选同一个厂商, 接入点/备注名/焦点都不动。
        }
        self.draft.provider_id = provider_id;
        self.draft.endpoint_id = default_endpoint_id;
        self.clear_field_error(BasicsField::Endpoint);
        let still_auto =
            self.draft.display_name.is_empty() || self.last_auto_display_name.as_deref() == Some(self.draft.display_name.as_str());
        if still_auto {
            let name = default_display_name(&provider_display_name, store);
            self.draft.display_name = name.clone();
            self.display_name_input = Input::new(name.clone());
            self.last_auto_display_name = Some(name);
            self.clear_field_error(BasicsField::DisplayName);
        }
        self.focus = BasicsField::ApiKey;
    }

    fn apply_endpoint_choice(&mut self, choice: &PickerChoice) {
        if let PickerChoice::Item(id) = choice {
            self.draft.endpoint_id = id.clone();
            self.clear_field_error(BasicsField::Endpoint);
        }
    }

    /// 兜底槽的「清空」项 (`id: ""`) 与其它槽位的正常选值走同一条路径: 写回空串本来就是它的语义
    /// (未配置), 不需要特殊分支。自定义输入 (`Custom`) `trim` 一下, 与订阅详情页 `open_model_picker`
    /// 那条路径同规则。
    fn apply_slot_choice(&mut self, slot: Slot, choice: &PickerChoice) {
        let value = match choice {
            PickerChoice::Item(id) => id.clone(),
            PickerChoice::Custom(text) => text.trim().to_string(),
        };
        self.slots_draft.slots.set(slot, value);
        self.clear_slot_error(slot);
    }

    /// `Submit` 行 `⏎`: 先 `validate_basics`, 失败则把焦点移到那个字段、把 `error` 挂上去 (Task 8
    /// 会在这里播 `fx::field_err`——焦点此刻已经落在出错的那一行, 用 `form::draw` 返回的聚焦行
    /// 矩形就够); 通过则打包 `WizardCmd::Create` 并进 `Stage::Creating`。
    fn submit(&mut self, s: &'static Strings) -> Option<Action> {
        match validate_basics(&self.draft, s) {
            Some((field, message)) => {
                self.focus = field;
                self.field_error = Some((field, message));
                None
            }
            None => {
                self.field_error = None;
                self.create_error = None;
                let cmd = WizardCmd::Create(CreateInput {
                    display_name: self.draft.display_name.clone(),
                    api_key: self.draft.api_key.clone(),
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
                self.slots_focus = SlotsField::Row(slot);
                self.slot_error = Some((slot, message));
                None
            }
            None => {
                self.slot_error = None;
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
    /// `Providers` 成功后转进 `Stage::Basics`; `Created(Ok)` 记下 id 并发 `LoadModels` (stage 仍是
    /// `Creating`, 只是文案换了——见 `Stage::Creating` 的文档注释); `Created(Err)` 回 `Basics` 并
    /// 挂说明行; `Models` 的两个 `Ok` 分支 (`Auto`/`ManualFallback`) 与 `Err` 都进 `Stage::Slots`,
    /// 区别只在有没有候选、有没有说明行; `SlotsSaved(Ok)` 关掉向导 + Success toast,
    /// `SlotsSaved(Err)` 回 `Slots` 并挂说明行。`Probed` 留给 Task 6, 保持空臂。
    fn apply_wizard_result(&mut self, result: &WizardResult, s: &'static Strings) -> Vec<Cmd> {
        match result {
            WizardResult::Providers(Ok(list)) => {
                self.providers = list.clone();
                self.stage = Stage::Basics;
                Vec::new()
            }
            WizardResult::Providers(Err(reason)) => {
                self.stage = Stage::LoadFailed(reason.clone());
                Vec::new()
            }
            WizardResult::Created(Ok(created)) => {
                self.created_id = Some(created.id.clone());
                vec![Cmd::Wizard(Box::new(WizardCmd::LoadModels { id: created.id.clone() }))]
            }
            WizardResult::Created(Err(e)) => {
                self.stage = Stage::Basics;
                self.create_error = Some((s.wiz_create_failed)(e));
                Vec::new()
            }
            WizardResult::Models(Ok(RefreshModelsResult::Auto { models, .. })) => {
                let mut slots = ModelSlots::default();
                // 桌面端同规则: 有候选就预填四个核心槽为第一项, 用户不用把五次 ⏎ 都点一遍才能
                // 保存——空候选 (理论上不该发生, `Auto` 变体本该非空, 防御性地) 就留空, 交给
                // `validate_slots` 在保存时拦住。
                if let Some(first) = models.first() {
                    for slot in [Slot::Fable, Slot::Opus, Slot::Sonnet, Slot::Haiku] {
                        slots.set(slot, first.id.clone());
                    }
                }
                self.slots_draft = SlotsDraft { slots, models: models.clone(), note: None };
                self.slots_focus = SlotsField::Row(Slot::Fable);
                self.stage = Stage::Slots;
                Vec::new()
            }
            WizardResult::Models(Ok(RefreshModelsResult::ManualFallback { reason })) => self.enter_slots_with_note((s.wiz_models_manual)(reason)),
            WizardResult::Models(Err(e)) => self.enter_slots_with_note((s.wiz_models_manual)(e)),
            WizardResult::Probed(_) => Vec::new(),
            WizardResult::SlotsSaved(Ok(())) => {
                self.notice = Some((ToastKind::Success, (s.wiz_created)(&self.draft.display_name)));
                self.close_request = true;
                Vec::new()
            }
            WizardResult::SlotsSaved(Err(e)) => {
                self.stage = Stage::Slots;
                self.slots_draft.note = Some((s.wiz_save_failed)(e));
                Vec::new()
            }
        }
    }

    /// `Models(Ok(ManualFallback))` 与 `Models(Err)` 共用: 候选清空、槽位留空、挂一条说明行、
    /// 进 `Stage::Slots`, 只是说明文案的措辞由调用方算好传进来。
    fn enter_slots_with_note(&mut self, note: String) -> Vec<Cmd> {
        self.slots_draft = SlotsDraft { note: Some(note), ..SlotsDraft::default() };
        self.slots_focus = SlotsField::Row(Slot::Fable);
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
            Stage::Creating => self.draw_basics(frame, area, theme, s, ctx.tick, popup_open),
            Stage::Slots => self.draw_slots(frame, area, theme, s, ctx.tick, popup_open),
            Stage::Saving => self.draw_slots(frame, area, theme, s, ctx.tick, popup_open),
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

    /// `Stage::Basics` / `Stage::Creating` 共用: 后者只是把全部字段画成 `locked`、按钮画成
    /// `busy`、标签换成 `wiz_creating`/`wiz_loading_models` (`Created(Ok)` 落地前后两个子阶段,
    /// 见 `Stage::Creating` 的文档注释)——两个阶段的行结构完全一样, 拆成两份反而要重复一遍组装
    /// 逻辑。
    fn draw_basics(&self, frame: &mut Frame, area: Rect, theme: &Theme, s: &'static Strings, tick: u64, popup_open: bool) {
        let busy = matches!(self.stage, Stage::Creating);

        let provider_label = self.provider_label();
        let endpoint_label = self.endpoint_label();
        let api_key_text = api_key_display(&self.draft.api_key, self.reveal_api_key);
        let pick_hint = format!("⏎ {}", s.key_pick);
        let reveal_hint = format!("Ctrl+R {}", s.key_reveal);
        // 弹窗叠在表单上面时终端光标该不该显示由 `FormView::show_cursor` 统一把关 (Task 5 起收在
        // `form::draw` 这一个关口), 这里恢复成无条件给值。
        let api_key_cursor = Some(self.api_key_input.visual_cursor());
        let display_name_cursor = Some(self.display_name_input.visual_cursor());
        let field_error = |field: BasicsField| self.field_error.and_then(|(f, msg)| (f == field).then_some(msg));

        let mut rows: Vec<FormRow> = Vec::new();
        if let Some(err) = &self.create_error {
            rows.push(FormRow::Note { text: err });
            rows.push(FormRow::Spacer);
        }
        rows.push(FormRow::Field {
            label: s.wiz_f_provider,
            value: &provider_label,
            placeholder: "",
            hint: Some(&pick_hint),
            cursor: None,
            error: field_error(BasicsField::Provider),
            locked: busy,
        });
        rows.push(FormRow::Field {
            label: s.wiz_f_endpoint,
            value: &endpoint_label,
            placeholder: "",
            hint: Some(&pick_hint),
            cursor: None,
            error: field_error(BasicsField::Endpoint),
            locked: busy,
        });
        rows.push(FormRow::Field {
            label: s.wiz_f_api_key,
            value: &api_key_text,
            placeholder: "",
            hint: Some(&reveal_hint),
            cursor: api_key_cursor,
            error: field_error(BasicsField::ApiKey),
            locked: busy,
        });
        rows.push(FormRow::Field {
            label: s.wiz_f_display_name,
            value: &self.draft.display_name,
            placeholder: "",
            hint: None,
            cursor: display_name_cursor,
            error: field_error(BasicsField::DisplayName),
            locked: busy,
        });
        rows.push(FormRow::Spacer);
        // `busy` 覆盖 `Created(Ok)` 落地前后两个子阶段 (仍然是同一个 `Stage::Creating`, 见它的
        // 文档注释): 还没拿到 id 时文案是 `wiz_creating`, 拿到 id 之后 (在等 `refresh_model_list`)
        // 换成 `wiz_loading_models`。
        let creating_label = if self.created_id.is_some() { s.wiz_loading_models } else { s.wiz_creating };
        rows.push(FormRow::Button { label: if busy { creating_label } else { s.wiz_btn_next }, busy });

        // 有说明行时整体后移 2 行 (Note + Spacer); Submit 前面还有一个 Spacer (下标 4), 按钮排在
        // 它后面 (下标 5)。
        let offset = if self.create_error.is_some() { 2 } else { 0 };
        let focus = offset
            + match self.focus {
                BasicsField::Provider => 0,
                BasicsField::Endpoint => 1,
                BasicsField::ApiKey => 2,
                BasicsField::DisplayName => 3,
                BasicsField::Submit => 5,
            };

        let view =
            FormView { title: s.wiz_title, steps: Some((0, s.wiz_steps.as_slice())), rows: &rows, focus, tick, show_cursor: !popup_open };
        // Task 8 会用这个返回值播 fx::field_err (校验失败时焦点一定落在出错的那一行)。
        let _focus_rect = form::draw(frame, area, &view, theme, s);
    }

    /// `Stage::Slots` / `Stage::Saving` 共用, 与 `draw_basics` 处理 `Basics`/`Creating` 同一个
    /// 套路: 后者只是把全部行画成 `locked`、按钮画成 `busy`、标签换成 `wiz_saving`。这一步**没有
    /// 文本行**——五个槽位都是选择行, `cursor` 恒 `None`。
    fn draw_slots(&self, frame: &mut Frame, area: Rect, theme: &Theme, s: &'static Strings, tick: u64, popup_open: bool) {
        let busy = matches!(self.stage, Stage::Saving);
        let pick_hint = format!("⏎ {}", s.key_pick);
        let slot_error = |slot: Slot| self.slot_error.and_then(|(sl, msg)| (sl == slot).then_some(msg));

        let mut rows: Vec<FormRow> = Vec::new();
        if let Some(note) = &self.slots_draft.note {
            rows.push(FormRow::Note { text: note });
            rows.push(FormRow::Spacer);
        }
        for slot in [Slot::Fable, Slot::Opus, Slot::Sonnet, Slot::Haiku, Slot::Fallback] {
            // 兜底槽留空时画成灰字「(未配置)」(`s.sub_slot_unset`, 复用订阅详情页的字段); 四个
            // 核心槽留空时没有专门的占位提示——没填就是没填, `validate_slots` 在保存时会拦住并
            // 指到这一行。
            let placeholder = if slot == Slot::Fallback { s.sub_slot_unset } else { "" };
            rows.push(FormRow::Field {
                label: slot_label(slot, s),
                value: self.slots_draft.slots.get(slot),
                placeholder,
                hint: Some(&pick_hint),
                cursor: None,
                error: slot_error(slot),
                locked: busy,
            });
        }
        rows.push(FormRow::Spacer);
        rows.push(FormRow::Button { label: if busy { s.wiz_saving } else { s.wiz_btn_save }, busy });

        // 说明行同 `draw_basics` 的 `create_error`: 有就整体后移 2 行。5 个槽位行占下标 0..4,
        // Spacer 占 5, 按钮占 6。
        let offset = if self.slots_draft.note.is_some() { 2 } else { 0 };
        let focus = offset
            + match self.slots_focus {
                SlotsField::Row(Slot::Fable) => 0,
                SlotsField::Row(Slot::Opus) => 1,
                SlotsField::Row(Slot::Sonnet) => 2,
                SlotsField::Row(Slot::Haiku) => 3,
                SlotsField::Row(Slot::Fallback) => 4,
                SlotsField::Save => 6,
            };

        let view =
            FormView { title: s.wiz_title, steps: Some((1, s.wiz_steps.as_slice())), rows: &rows, focus, tick, show_cursor: !popup_open };
        let _focus_rect = form::draw(frame, area, &view, theme, s);
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

    /// 底栏左侧。`Loading`/`LoadFailed`/`Creating` 没有任何可操作的字段 (`Creating` 整张表单
    /// 只读), 留空; `Basics`/`Slots` 按**当前聚焦行的类型**给提示 (评审 I2: 旧版三条提示写死不
    /// 随焦点变, 在文本行上 `⏎` 实际是"下一项"却显示成"选择", 在按钮上 `⏎` 会真的调用后端却看
    /// 不出来)——`↑↓ 字段` 常驻; 选择行 (`Provider`/`Endpoint`/`Slots` 的槽位行) 追加 `⏎ 选择`;
    /// 文本行 (`ApiKey`/`DisplayName`) 追加 `⏎ 下一项`, `ApiKey` 再多一条 `Ctrl+R 显示/隐藏`
    /// (只在这一行有效); 按钮行 (`Submit`/`Save`) 追加 `⏎` + **按钮自己的标签**, 而不是一个通用
    /// 词, 让用户一眼知道回车会发生什么。行内的 hint (行右端「⏎ 选择」之类) 不受这条规则影响,
    /// 照旧固定。
    pub fn hints(&self, s: &'static Strings) -> Vec<Hint<'static>> {
        match &self.stage {
            Stage::Loading | Stage::LoadFailed(_) | Stage::Creating | Stage::Saving => Vec::new(),
            Stage::Basics => {
                let mut hints = vec![("↑↓", s.key_field)];
                match self.focus {
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
                match self.slots_focus {
                    SlotsField::Row(_) => hints.push(("⏎", s.key_pick)),
                    SlotsField::Save => hints.push(("⏎", s.wiz_btn_save)),
                }
                hints
            }
        }
    }

    /// 请求在飞 (`Stage::Creating`/`Saving`) 时能不能按 `Esc` 退出——`App::draw` 据此决定要不要
    /// 在底栏右侧显示 `Esc 取消` (评审 M9: 在飞时连 `Esc` 都被 `handle_key` 吞掉, 继续显示这条
    /// 提示就是纯误导)。
    pub fn can_cancel(&self) -> bool {
        !matches!(self.stage, Stage::Creating | Stage::Saving)
    }

    /// 用户已经填过东西 / 已经创建过订阅 —— `Esc` 要不要先确认看这个。`Basics` 下「厂商已选」
    /// 或「API Key 非空」或「备注名非空」任一成立即为真 (用户填了一半按 `Esc` 不该直接丢掉);
    /// `Creating`/`Slots`/`Saving` 恒真 (订阅已经在飞行中创建, 或者已经建好了)。
    pub fn has_input(&self) -> bool {
        match &self.stage {
            Stage::Loading | Stage::LoadFailed(_) => false,
            Stage::Basics => {
                !self.draft.provider_id.is_empty() || !self.draft.api_key.is_empty() || !self.draft.display_name.trim().is_empty()
            }
            Stage::Creating | Stage::Slots | Stage::Saving => true,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(w.handle_key(key(KeyCode::Esc), &crate::i18n::ZH), Some(Action::CloseWizard));
    }

    /// 除 `Esc` 外的按键在 `Stage::Loading` 一律被吞掉——没有任何字段可以接收字符输入 (`Basics`
    /// 起才有, 见 `wizard_basics_snapshot_80x24` 等 `tests/ui.rs` 里的用例)。
    #[test]
    fn other_keys_are_swallowed() {
        let mut w = Wizard::new();
        for code in [KeyCode::Char('q'), KeyCode::Char('r'), KeyCode::Char('1'), KeyCode::Tab, KeyCode::Enter] {
            assert_eq!(w.handle_key(key(code), &crate::i18n::ZH), None, "{code:?}");
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
        assert_eq!(w.handle_key(key(KeyCode::Esc), &crate::i18n::ZH), Some(Action::CloseWizard));
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
        assert_eq!(w.handle_key(key(KeyCode::Esc), &crate::i18n::ZH), None, "在飞时 Esc 应该被吞掉");
    }

    /// 同上, `Stage::Saving` 是新加的另一个"在飞"阶段, 应该同样吞掉 `Esc`。
    #[test]
    fn escape_is_swallowed_while_saving() {
        let mut w = Wizard::new();
        w.stage = Stage::Saving;
        assert!(!w.can_cancel(), "保存在飞时不该能取消");
        assert_eq!(w.handle_key(key(KeyCode::Esc), &crate::i18n::ZH), None, "保存在飞时 Esc 应该被吞掉");
    }

    /// P5 Task 5 前置项 2: 底栏提示按焦点行的类型变化, 四个分支 (选择行 / API Key 文本行 / 备注名
    /// 文本行 / 按钮行) 都要断言到——之前只有 API Key 那一支靠 `wizard_basics_80x24` 快照间接钉住。
    /// 按钮行要断言出现的是**按钮自己的标签**, 不是一个通用词。
    #[test]
    fn hints_follow_focus_on_every_basics_row_type() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Basics;

        w.focus = BasicsField::Provider;
        assert_eq!(w.hints(s), vec![("↑↓", s.key_field), ("⏎", s.key_pick)], "选择行应该提示 ⏎ 选择");

        w.focus = BasicsField::ApiKey;
        assert_eq!(
            w.hints(s),
            vec![("↑↓", s.key_field), ("⏎", s.key_next_field), ("Ctrl+R", s.key_reveal)],
            "API Key 行额外带 Ctrl+R 提示"
        );

        w.focus = BasicsField::DisplayName;
        let hints = w.hints(s);
        assert_eq!(hints, vec![("↑↓", s.key_field), ("⏎", s.key_next_field)], "备注名行是文本行, 但不该有 Ctrl+R 提示");
        assert!(!hints.iter().any(|(k, _)| *k == "Ctrl+R"), "备注名行不该出现 Ctrl+R\n{hints:?}");

        w.focus = BasicsField::Submit;
        assert_eq!(w.hints(s), vec![("↑↓", s.key_field), ("⏎", s.wiz_btn_next)], "按钮行应该显示按钮自己的标签, 不是通用词");
    }

    /// 同上, 覆盖 `Stage::Slots` 新增的两支: 槽位行 (选择行) 与 `Save` 按钮行。
    #[test]
    fn hints_follow_focus_on_every_slots_row_type() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Slots;

        w.slots_focus = SlotsField::Row(Slot::Fable);
        assert_eq!(w.hints(s), vec![("↑↓", s.key_field), ("⏎", s.key_pick)], "槽位行应该提示 ⏎ 选择");

        w.slots_focus = SlotsField::Save;
        assert_eq!(w.hints(s), vec![("↑↓", s.key_field), ("⏎", s.wiz_btn_save)], "保存按钮行应该显示它自己的标签");
    }

    /// P5 Task 5 前置项 3: 编辑字段只清自己的错误——负向用例 (Task 4 只有正向: 编辑同一个字段清
    /// 掉自己的错误; `field_error` 是单个 `Option`, 退化成"任何编辑都清"时那条正向测试照样能过,
    /// 必须补一条编辑别的字段、断言原字段错误还在的用例才咬得住这个回归)。
    #[test]
    fn editing_one_field_does_not_clear_another_fields_error() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Basics;
        w.field_error = Some((BasicsField::Provider, s.wiz_err_provider));
        w.focus = BasicsField::DisplayName;

        w.handle_key(key(KeyCode::Char('a')), s);

        assert_eq!(w.field_error, Some((BasicsField::Provider, s.wiz_err_provider)), "编辑备注名不该清掉厂商行的错误");
    }

    /// 同上, `Stage::Slots` 里槽位错误的负向用例: 给 `Fable` 挂错误, 选 `Opus` 的模型, `Fable`
    /// 的错误应该原封不动。
    #[test]
    fn selecting_one_slot_does_not_clear_another_slots_error() {
        let s = &crate::i18n::ZH;
        let mut w = Wizard::new();
        w.stage = Stage::Slots;
        w.slot_error = Some((Slot::Fable, s.wiz_err_slot));

        w.apply_slot_choice(Slot::Opus, &PickerChoice::Item("glm-4.6".into()));

        assert_eq!(w.slot_error, Some((Slot::Fable, s.wiz_err_slot)), "选定 Opus 的模型不该清掉 Fable 行的错误");
    }
}
