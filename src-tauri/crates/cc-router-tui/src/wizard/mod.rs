//! 新建订阅向导。**不是标签页也不是弹窗**, 而是夹在弹窗与全局键之间的一层: 存在时内容区整个
//! 归它, 除 `Ctrl+C` 外所有按键归它 (所以 `q` / `r` / `1`-`5` 能当普通字符输入)。它之上仍然可以
//! 叠**一个**弹窗 (选厂商 / 选模型 / 退出确认), 所以不需要弹窗栈。
//!
//! 它刻意不实现 `Component` (`crate::pages::Component`): 那个 trait 的一半方法
//! (`on_subscriptions_changed` / `on_mutation_*` / `on_event`) 对向导没有意义, 而向导需要的
//! `on_open` 又不在里面。方法签名仍然照着 `Component` 写, 调用约定一致 (`App` 那一侧的
//! `update_wizard` helper与 `update_page` 一一对应)。
//!
//! P5 Task 2 只搭这一层的架子: 拉厂商列表、画一个加载中/失败的空容器、`Esc` 退出。表单本身 (选
//! 厂商 / 填 API Key / 绑定模型) 从 Task 4 起分阶段加进 [`Stage`]。

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ratatui::Frame;
use throbber_widgets_tui::{Throbber, BRAILLE_SIX};

use crate::action::{Action, Cmd, WizardCmd, WizardResult};
use crate::client::dto::Provider;
use crate::i18n::Strings;
use crate::pages::DrawCtx;
use crate::store::Store;
use crate::widgets::keybar::Hint;
use crate::widgets::toast::ToastKind;
use crate::widgets::spinner_state;

/// 向导走到哪一步了。P5 Task 2 只有前两个, Task 4–6 各自往里加分支 (穷尽 `match`, 加了不接住
/// 就编译失败)。
enum Stage {
    /// 正在拉厂商列表。
    Loading,
    /// 拉失败了, 表单画不出来, 只能 `Esc` 退出。
    LoadFailed(String),
    // Task 4: Basics / Creating
    // Task 5: Slots / Saving
    // Task 6: Custom / Probing
}

pub struct Wizard {
    stage: Stage,
    /// `list_providers` 拉到的厂商列表。P5 Task 2 只负责存 (`Stage::Loading` 的画面不随它变化),
    /// 展示交给 Task 4 起的表单——`Stage` 那时候会从 `Loading` 换成真正的 `Basics` 之类的分支。
    #[allow(dead_code)] // 读取点在 Task 4
    providers: Vec<Provider>,
    /// 与页面的 `pending_notice` 同一套约定, 见 `take_notice`。
    notice: Option<(ToastKind, String)>,
    /// `take_close_request()` 的待办标记。P5 Task 2 生产代码里还没有任何写入点 (Task 4 起「创建
    /// 成功」/「保存成功」才会真的置位); `App` 那一侧转发这个标记的逻辑必须现在就测到, 所以留了
    /// `request_close_for_test` 这个测试专用入口 (与 `pages::logs::Logs::set_force_dirty` 同一套
    /// 「只在编译本 crate 单测时存在」的做法)。
    close_request: bool,
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
        Self { stage: Stage::Loading, providers: Vec::new(), notice: None, close_request: false }
    }

    /// 刚打开: 要发的请求 (拉厂商列表)。`App` 在创建它之后立刻调一次。
    pub fn on_open(&mut self) -> Vec<Cmd> {
        vec![Cmd::Wizard(Box::new(WizardCmd::LoadProviders))]
    }

    /// 除 `Ctrl+C` 外的全部按键。`None` = 吞掉 (或只改了向导自己的状态)。P5 Task 2 只认识
    /// `Esc`——`has_input()` 恒 `false`, 所以直接关闭, 不弹确认; Task 4 起 `has_input()` 会随表单
    /// 填写变化, 这条判断到时候自然生效, 不用改这个方法本身。
    pub fn handle_key(&mut self, key: KeyEvent, s: &'static Strings) -> Option<Action> {
        match key.code {
            KeyCode::Esc if self.has_input() => {
                Some(Action::OpenConfirm { prompt: s.confirm_discard.to_string(), on_yes: Box::new(Action::CloseWizard) })
            }
            KeyCode::Esc => Some(Action::CloseWizard),
            _ => None,
        }
    }

    /// 目前只消费 `Action::WizardDone`; `Action::PickerDone` 留给 Task 4 起的选厂商/选模型弹窗,
    /// 现在没有任何 picker 会以向导为目标, 其余 action 一律忽略。
    pub fn update(&mut self, action: &Action, _store: &Store, _s: &'static Strings) -> Vec<Cmd> {
        if let Action::WizardDone(result) = action {
            self.apply_wizard_result(result);
        }
        Vec::new()
    }

    /// **刻意写成穷尽 `match`, 不用 `_` 兜底、也不用任何 `#[allow]`**: `WizardResult` 每加一个新
    /// 变体, 这里就必须显式接一条臂——哪怕暂时只是空臂 `=> {}`——否则编译期就 `E0004` 失败。这是
    /// 上一轮评审专门要的保护: 不这样做的话,「表单填完按了创建, 结果被静默吞掉, 向导永远转圈」这种
    /// 事只会在运行时才暴露 (Review round 1)。`Created`/`Models`/`Probed`/`SlotsSaved` 四个空臂由
    /// Task 3 加入, 真实处理 (创建成功后进 Basics→Slots / 探测结果写进表单 / 保存成功后关闭向导) 留给
    /// Task 4–6, 不是这里的改动范围。
    fn apply_wizard_result(&mut self, result: &WizardResult) {
        match result {
            WizardResult::Providers(Ok(list)) => self.providers = list.clone(),
            WizardResult::Providers(Err(reason)) => self.stage = Stage::LoadFailed(reason.clone()),
            WizardResult::Created(_) => {}
            WizardResult::Models(_) => {}
            WizardResult::Probed(_) => {}
            WizardResult::SlotsSaved(_) => {}
        }
    }

    /// P5 Task 2 只画一个带边框的空容器: 标题 `s.wiz_title`, 正文居中一行——加载中带 throbber
    /// (与总览页「重连中」同一套 `spinner_state` + `to_symbol_span`), 失败时改成一行错误文案。
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &mut DrawCtx) {
        let theme = ctx.theme;
        let s = ctx.s;
        let block = Block::bordered().border_type(BorderType::Rounded).border_style(theme.border_style()).title_top(format!(" {} ", s.wiz_title));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let line = match &self.stage {
            Stage::Loading => {
                let state = spinner_state(ctx.tick);
                // `to_symbol_span` 自己已经在符号后面带一个空格 (throbber-widgets-tui 的实现),
                // 这里不用再手动加一个前导空格——否则会跟总览页「重连中」那行 (`draw_tabs`) 的间距
                // 对不上, 平白多出一格。
                let spinner = Throbber::default().throbber_set(BRAILLE_SIX).to_symbol_span(&state);
                Line::from(vec![spinner, Span::raw(s.wiz_loading_providers)]).centered()
            }
            Stage::LoadFailed(reason) => Line::styled((s.wiz_load_failed)(reason), Style::new().fg(theme.err)).centered(),
        };
        frame.render_widget(line, inner.centered_vertically(Constraint::Length(1)));
    }

    /// 底栏左侧。P5 Task 2 没有任何可操作的字段, 唯一的键 (`Esc`) 由 `App::draw` 固定画在右侧
    /// (`s.key_cancel`), 这里留空。
    pub fn hints(&self, _s: &'static Strings) -> Vec<Hint<'static>> {
        Vec::new()
    }

    /// 用户已经填过东西 / 已经创建过订阅 —— `Esc` 要不要先确认看这个。P5 Task 2 恒 `false`
    /// (还没有任何字段), Task 4 起填真实实现。
    pub fn has_input(&self) -> bool {
        false
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

    /// 测试专用: 直接置位「向导想关闭自己」。P5 Task 2 的生产代码里没有任何路径会走到这里 (那要
    /// 等 Task 4 的「创建成功」), 但 `App::update_wizard` 转发 `take_close_request()` 的逻辑必须
    /// 现在就有测试盯着——写法照抄 `pages::logs::Logs::set_force_dirty`。
    #[cfg(test)]
    pub fn request_close_for_test(&mut self) {
        self.close_request = true;
    }

    /// 测试专用: 直接塞一条待发的 notice。P5 Task 2 的生产代码里同样没有任何路径会写
    /// `self.notice` (那要等 Task 4 起「校验失败」之类的场景), 与 `request_close_for_test` 同一
    /// 条理由——`App::update_wizard` 转发 `take_notice()` 的逻辑 (Review round 1) 需要一个能从
    /// 外面戳进 notice 的入口。
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

    /// 除 `Esc` 外的按键在这个骨架阶段一律被吞掉——没有任何字段可以接收字符输入。
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
        // 仍然按「没有输入」处理 (骨架阶段 has_input 恒 false), Esc 照常直接关闭。
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
}
