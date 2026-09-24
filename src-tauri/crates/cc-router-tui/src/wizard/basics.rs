//! 内置厂商路径的第一步: 选厂商 / 选接入点 / 填 API Key / 备注名 → 下一步。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Frame;

use super::fields::{validate_basics, BasicsDraft, BasicsField};
use super::form_state::FormState;
use super::{edit_focused_text, follow_display_name, BasicsPhase, Paint};
use crate::action::{Action, WizardCmd};
use crate::client::dto::{CreateInput, CreateSource, CustomProtocol, ModelSlots, Provider};
use crate::i18n::Strings;
use crate::store::Store;
use crate::widgets::form::{self, FormBuilder, FormRow, FormView};
use crate::widgets::keybar::Hint;
use crate::widgets::picker::{PickerChoice, PickerItem, PickerSpec, PickerTag};
use crate::widgets::toast::ToastKind;

pub(super) struct BasicsForm {
    pub(super) draft: BasicsDraft,
    pub(super) state: FormState<BasicsField>,
    /// 上一次自动算出来的备注名, 见 `follow_display_name`。
    pub(super) last_auto_name: Option<String>,
    /// `create_subscription` 失败时的原因, 挂成表单顶部的说明行。
    pub(super) create_error: Option<String>,
}

impl Default for BasicsForm {
    fn default() -> Self {
        Self { draft: BasicsDraft::default(), state: FormState::new(BasicsField::Provider), last_auto_name: None, create_error: None }
    }
}

impl BasicsForm {
    /// 只在 `BasicsPhase::Editing` 下被调用。提交成功时把 `phase` 推进到 `Creating`。
    pub(super) fn handle_key(&mut self, key: KeyEvent, phase: &mut BasicsPhase, providers: &[Provider], s: &'static Strings) -> Option<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let focus = self.state.focus;
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.state.step(-1, &BasicsField::ALL);
                None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.state.step(1, &BasicsField::ALL);
                None
            }
            KeyCode::Enter if focus == BasicsField::Provider => Some(self.provider_picker(providers, s)),
            KeyCode::Enter if focus == BasicsField::Endpoint => Some(self.endpoint_picker(providers, s)),
            KeyCode::Enter if matches!(focus, BasicsField::ApiKey | BasicsField::DisplayName) => {
                self.state.step(1, &BasicsField::ALL);
                None
            }
            KeyCode::Enter if focus == BasicsField::Submit => self.submit(phase, s),
            // 必须排在下面的打字分支之前: 否则 Ctrl+R 会被 `ApiKey` 的编辑分支当成字符 'r' 吃掉。
            KeyCode::Char('r') if ctrl && focus == BasicsField::ApiKey => {
                self.draft.api_key.toggle_reveal();
                None
            }
            // 文本行交给统一的编辑路径; 选择行没有文本字段, 其余按键 (含字符键) 一律吞掉。
            _ => {
                edit_focused_text(self.draft.text_field(focus), &mut self.state, key);
                None
            }
        }
    }

    /// `Submit` 行 `⏎`: 校验失败把焦点移到那个字段并挂错误; 通过则打包 `WizardCmd::Create`
    /// (槽位先放 `ModelSlots::pending()`, 第二步再绑) 并进 `Creating`。
    fn submit(&mut self, phase: &mut BasicsPhase, s: &'static Strings) -> Option<Action> {
        match validate_basics(&self.draft, s) {
            Some((field, message)) => {
                self.state.reject(field, message);
                None
            }
            None => {
                self.state.clear_all();
                self.create_error = None;
                let cmd = WizardCmd::Create(CreateInput {
                    display_name: self.draft.display_name.value().to_string(),
                    api_key: self.draft.api_key.secret(),
                    model_slots: ModelSlots::pending(),
                    source: CreateSource::Builtin { provider_id: self.draft.provider_id.clone(), endpoint_id: self.draft.endpoint_id.clone() },
                });
                *phase = BasicsPhase::Creating;
                Some(Action::WizardRequest(Box::new(cmd)))
            }
        }
    }

    /// `Provider` 行 `⏎`: 条目是全部厂商 + 5 个自定义协议。OAuth 类厂商 (TUI 不做设备码流程)
    /// 的 label 后面追加提示语, picker 本身没有"置灰不可选"的能力, 选中时用「可选但选了只给
    /// 提示」等效 (见 `Wizard::apply_provider_choice`)。
    fn provider_picker(&self, providers: &[Provider], s: &'static Strings) -> Action {
        let mut items: Vec<PickerItem> = providers
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

    /// `Endpoint` 行 `⏎`: 厂商还没选时就地拒绝, 不开弹窗。
    fn endpoint_picker(&self, providers: &[Provider], s: &'static Strings) -> Action {
        let Some(provider) = providers.iter().find(|p| p.id == self.draft.provider_id) else {
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

    /// 选中一个可用的内置厂商: 记 `provider_id`, 接入点置为默认值, 备注名按需跟随, 焦点移到
    /// `ApiKey`。**重选同一个厂商什么都不重算**——否则用户手动换过的接入点会在无意中确认同一项
    /// 时被悄悄弹回默认值。无论重选与否, 厂商行自己的错误都清掉 (这个字段已经合法了)。
    pub(super) fn choose_provider(&mut self, provider: &Provider, store: &Store) {
        self.state.clear(BasicsField::Provider);
        if provider.id == self.draft.provider_id {
            return;
        }
        self.draft.provider_id = provider.id.clone();
        self.draft.endpoint_id = provider.default_endpoint().map(|e| e.id.clone()).unwrap_or_default();
        self.state.clear(BasicsField::Endpoint);
        if follow_display_name(&mut self.draft.display_name, &mut self.last_auto_name, &provider.display_name, store) {
            self.state.clear(BasicsField::DisplayName);
        }
        self.state.focus = BasicsField::ApiKey;
    }

    pub(super) fn apply_endpoint_choice(&mut self, choice: &PickerChoice) {
        if let PickerChoice::Item(id) = choice {
            self.draft.endpoint_id = id.clone();
            self.state.clear(BasicsField::Endpoint);
        }
    }

    /// 「厂商已选」或「API Key 非空」或「备注名非空」任一成立——填了一半按 `Esc` 不该直接丢掉。
    pub(super) fn has_input(&self) -> bool {
        !self.draft.provider_id.is_empty() || !self.draft.api_key.is_empty() || !self.draft.display_name.value().trim().is_empty()
    }

    pub(super) fn hints(&self, s: &'static Strings) -> Vec<Hint<'static>> {
        let mut hints = vec![("↑↓", s.key_field)];
        match self.state.focus {
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

    /// 三个阶段的行结构完全一样; 请求在飞时 (`Creating` / `LoadingModels`) 全部字段画成
    /// `locked`、按钮画成 `busy` 并换成对应的进行中文案。
    pub(super) fn draw(&self, frame: &mut Frame, area: Rect, phase: &BasicsPhase, providers: &[Provider], p: &Paint) {
        let s = p.s;
        let busy_label = match phase {
            BasicsPhase::Editing => None,
            BasicsPhase::Creating => Some(s.wiz_creating),
            BasicsPhase::LoadingModels { .. } => Some(s.wiz_loading_models),
        };
        let busy = busy_label.is_some();

        let provider = providers.iter().find(|p| p.id == self.draft.provider_id);
        let provider_label = provider.map(|p| p.display_name.clone()).unwrap_or_default();
        let endpoint_label = provider
            .and_then(|p| p.endpoints.iter().find(|e| e.id == self.draft.endpoint_id))
            .map(|e| e.label.clone())
            .unwrap_or_default();
        let api_key_text = self.draft.api_key.display();
        let pick_hint = format!("⏎ {}", s.key_pick);
        let reveal_hint = format!("Ctrl+R {}", s.key_reveal);
        let api_key_cursor = Some(self.draft.api_key.visual_cursor());
        let display_name_cursor = Some(self.draft.display_name.visual_cursor());
        let field = |label, value, hint, cursor, field: BasicsField| FormRow::Field {
            label,
            value,
            placeholder: "",
            hint,
            cursor,
            error: self.state.error_for(field),
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
            b.push(field(label, value, hint, cursor, f), self.state.focus == f);
        }
        b.push(FormRow::Spacer, false);
        b.push(FormRow::Button { label: busy_label.unwrap_or(s.wiz_btn_next), busy }, self.state.focus == BasicsField::Submit);
        let built = b.finish();

        let view = FormView {
            title: s.wiz_title,
            steps: Some((0, s.wiz_steps.as_slice())),
            rows: &built.rows,
            focus: built.focus,
            tick: p.tick,
            show_cursor: p.show_cursor,
        };
        let _focus_rect = form::draw(frame, area, &view, p.theme, s);
    }
}
