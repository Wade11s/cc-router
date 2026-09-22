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
//! `Esc` 退出)。**Task 4 起加真正的表单**: 本文件当前实现了内置厂商路径的第一步
//! (`Stage::Basics` / `Creating`)——选厂商 → 选接入点 → 填 API Key → 备注名 → 下一步; 第二步
//! (绑定模型, `Stage::Slots` / `Saving`) 与自定义厂商的单页表单 (`Stage::Custom` / `Probing`)
//! 留给 Task 5/6。

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
use crate::client::dto::{CreateInput, CreateSource, CustomProtocol, ModelSlots, Provider};
use crate::i18n::Strings;
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
use fields::{api_key_display, default_display_name, validate_basics, BasicsDraft, BasicsField};

/// 向导走到哪一步了。P5 Task 2 只有前两个, Task 4 加了 `Basics`/`Creating` (穷尽 `match`, 加了不
/// 接住就编译失败), Task 5/6 各自继续往里加分支。
enum Stage {
    /// 正在拉厂商列表。
    Loading,
    /// 拉失败了, 表单画不出来, 只能 `Esc` 退出。
    LoadFailed(String),
    /// 内置路径第一步: 选厂商 / 选接入点 / 填 API Key / 备注名。
    Basics,
    /// `create_subscription` (以及紧随其后的 `refresh_model_list`) 在飞: 表单只读, 按钮转圈。
    /// 本 Task 收到 `Created` 结果时只弹一条 toast 并 `Action::CloseWizard`; Task 5 接手真正的
    /// 后续流转。
    Creating,
    // Task 5: Slots / Saving
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
    /// `create_subscription` 失败时的原因, 挂成表单顶部的说明行 (`FormRow::Note`, Task 5 保留
    /// 这个字段继续用)。
    create_error: Option<String>,
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
        }
    }

    /// 刚打开: 要发的请求 (拉厂商列表)。`App` 在创建它之后立刻调一次。
    pub fn on_open(&mut self) -> Vec<Cmd> {
        vec![Cmd::Wizard(Box::new(WizardCmd::LoadProviders))]
    }

    /// 除 `Ctrl+C` 外的全部按键。`None` = 吞掉 (或只改了向导自己的状态)。`Esc` 的处理跟阶段
    /// 无关 (`can_cancel()` 为假时连 `Esc` 也不接), 排在最前面统一判断; 其余按键只有
    /// `Stage::Basics` 才会真的处理——`Loading` / `LoadFailed` 没有字段可以接收输入, `Creating`
    /// 整张表单只读 (总规则: 有请求在飞时按键全部吞掉, 评审 M9 起连 `Esc` 也不例外)。
    pub fn handle_key(&mut self, key: KeyEvent, s: &'static Strings) -> Option<Action> {
        if key.code == KeyCode::Esc && self.can_cancel() {
            return Some(if self.has_input() {
                Action::OpenConfirm { prompt: s.confirm_discard.to_string(), on_yes: Box::new(Action::CloseWizard) }
            } else {
                Action::CloseWizard
            });
        }
        if matches!(self.stage, Stage::Basics) {
            self.handle_basics_key(key, s)
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

    /// `Action::PickerDone` 落地: 按 `tag` 分派给厂商 / 接入点两条分支; 跟向导无关的 tag (其它
    /// 页面自己的弹窗) 直接忽略——**穷尽 `match`**, 新增 `PickerTag` 变体时这里会编译失败, 逼着
    /// 显式决定向导要不要关心它。
    fn apply_picker_choice(&mut self, tag: &PickerTag, choice: &PickerChoice, store: &Store, s: &'static Strings) {
        match tag {
            PickerTag::WizardProvider => self.apply_provider_choice(choice, store, s),
            PickerTag::WizardEndpoint => self.apply_endpoint_choice(choice),
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

    /// 消费 `Action::WizardDone` (异步结果) 与 `Action::PickerDone` (选厂商/选接入点弹窗的结果);
    /// `Submit` 触发的请求走的是另一条路 (`Action::WizardRequest`, 由 `App::update` 直接转成
    /// `Cmd::Wizard`, 不经过这里——见该 action 的文档注释), 所以这个方法目前仍然不产出任何
    /// `Cmd`, 返回值恒为空。
    pub fn update(&mut self, action: &Action, store: &Store, s: &'static Strings) -> Vec<Cmd> {
        match action {
            Action::WizardDone(result) => self.apply_wizard_result(result, s),
            Action::PickerDone { tag, choice } => self.apply_picker_choice(tag, choice, store, s),
            _ => {}
        }
        Vec::new()
    }

    /// **刻意写成穷尽 `match`, 不用 `_` 兜底、也不用任何 `#[allow]`**: `WizardResult` 每加一个新
    /// 变体, 这里就必须显式接一条臂——哪怕暂时只是空臂 `=> {}`——否则编译期就 `E0004` 失败。
    /// `Providers` 成功后转进 `Stage::Basics`; `Created` 的处理是**临时的** (Task 4 只关掉向导 /
    /// 回填错误说明, Task 5 换成"进 Stage::Slots 接着拉模型列表")。`Models`/`Probed`/`SlotsSaved`
    /// 三个空臂留给 Task 5/6。
    fn apply_wizard_result(&mut self, result: &WizardResult, s: &'static Strings) {
        match result {
            WizardResult::Providers(Ok(list)) => {
                self.providers = list.clone();
                self.stage = Stage::Basics;
            }
            WizardResult::Providers(Err(reason)) => self.stage = Stage::LoadFailed(reason.clone()),
            WizardResult::Created(Ok(_)) => {
                self.notice = Some((ToastKind::Success, (s.wiz_created)(&self.draft.display_name)));
                self.close_request = true;
            }
            WizardResult::Created(Err(e)) => {
                self.stage = Stage::Basics;
                self.create_error = Some((s.wiz_create_failed)(e));
            }
            WizardResult::Models(_) => {}
            WizardResult::Probed(_) => {}
            WizardResult::SlotsSaved(_) => {}
        }
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
    /// `busy`、标签换成 `wiz_creating`——两个阶段的行结构完全一样, 拆成两份反而要重复一遍组装
    /// 逻辑。
    fn draw_basics(&self, frame: &mut Frame, area: Rect, theme: &Theme, s: &'static Strings, tick: u64, popup_open: bool) {
        let busy = matches!(self.stage, Stage::Creating);

        let provider_label = self.provider_label();
        let endpoint_label = self.endpoint_label();
        let api_key_text = api_key_display(&self.draft.api_key, self.reveal_api_key);
        let pick_hint = format!("⏎ {}", s.key_pick);
        let reveal_hint = format!("Ctrl+R {}", s.key_reveal);
        // M5: 弹窗盖在上面时两个文本行都不设光标 (`None`), 值本身照常显示——只是没有一个终端
        // 光标去闪它。
        let api_key_cursor = (!popup_open).then(|| self.api_key_input.visual_cursor());
        let display_name_cursor = (!popup_open).then(|| self.display_name_input.visual_cursor());
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
        rows.push(FormRow::Button { label: if busy { s.wiz_creating } else { s.wiz_btn_next }, busy });

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

        let view = FormView { title: s.wiz_title, steps: Some((0, s.wiz_steps.as_slice())), rows: &rows, focus, tick };
        // Task 8 会用这个返回值播 fx::field_err (校验失败时焦点一定落在出错的那一行)。
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
    /// 只读), 留空; `Basics` 按**当前聚焦行的类型**给提示 (评审 I2: 旧版三条提示写死不随焦点变,
    /// 在文本行上 `⏎` 实际是"下一项"却显示成"选择", 在按钮上 `⏎` 会真的调用后端却看不出来)——
    /// `↑↓ 字段` 常驻; 选择行 (`Provider`/`Endpoint`) 追加 `⏎ 选择`; 文本行 (`ApiKey`/
    /// `DisplayName`) 追加 `⏎ 下一项`, `ApiKey` 再多一条 `Ctrl+R 显示/隐藏` (只在这一行有效);
    /// 按钮行 (`Submit`) 追加 `⏎` + **按钮自己的标签**, 而不是一个通用词, 让用户一眼知道回车
    /// 会发生什么。行内的 hint (行右端「⏎ 选择」之类) 不受这条规则影响, 照旧固定。
    pub fn hints(&self, s: &'static Strings) -> Vec<Hint<'static>> {
        match &self.stage {
            Stage::Loading | Stage::LoadFailed(_) | Stage::Creating => Vec::new(),
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
        }
    }

    /// 请求在飞 (`Stage::Creating`) 时能不能按 `Esc` 退出——`App::draw` 据此决定要不要在底栏
    /// 右侧显示 `Esc 取消` (评审 M9: 在飞时连 `Esc` 都被 `handle_key` 吞掉, 继续显示这条提示
    /// 就是纯误导)。
    pub fn can_cancel(&self) -> bool {
        !matches!(self.stage, Stage::Creating)
    }

    /// 用户已经填过东西 / 已经创建过订阅 —— `Esc` 要不要先确认看这个。`Basics` 下「厂商已选」
    /// 或「API Key 非空」或「备注名非空」任一成立即为真 (用户填了一半按 `Esc` 不该直接丢掉);
    /// `Creating` 恒真 (订阅可能已经在飞行中创建)。
    pub fn has_input(&self) -> bool {
        match &self.stage {
            Stage::Loading | Stage::LoadFailed(_) => false,
            Stage::Basics => {
                !self.draft.provider_id.is_empty() || !self.draft.api_key.is_empty() || !self.draft.display_name.trim().is_empty()
            }
            Stage::Creating => true,
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
}
