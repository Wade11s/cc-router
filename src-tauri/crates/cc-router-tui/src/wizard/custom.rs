//! 自定义厂商路径的单页表单: 协议 / 厂商名 / Base URL / 请求路径 / 鉴权 / API Key / 备注名 /
//! [获取模型列表] / 五个槽位 / [创建]。槽位在创建前就是真值, 所以没有第二步。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Frame;

use super::fields::{validate_custom, CustomDraft, CustomField};
use super::form_state::FormState;
use super::{edit_focused_text, follow_display_name, CustomPhase, Paint};
use crate::action::{Action, WizardCmd};
use crate::client::dto::{
    AuthHeaderFormat, CreateInput, CreateSource, CustomProtocol, CustomSource, ProbeInput, Slot, ANTHROPIC_AUTH_PRESETS,
    CUSTOM_BASE_URL_PLACEHOLDER,
};
use crate::i18n::Strings;
use crate::pages::subscriptions::slot_label;
use crate::store::Store;
use crate::widgets::form::{self, FormBuilder, FormRow, FormView};
use crate::widgets::keybar::Hint;
use crate::widgets::picker::{PickerChoice, PickerItem, PickerSpec, PickerTag};

pub(super) struct CustomForm {
    pub(super) draft: CustomDraft,
    pub(super) state: FormState<CustomField>,
    /// 上一次自动算出来的备注名, 见 `follow_display_name`。
    pub(super) last_auto_name: Option<String>,
}

impl CustomForm {
    pub(super) fn new(protocol: CustomProtocol) -> Self {
        Self { draft: CustomDraft::new(protocol), state: FormState::new(CustomField::ProviderName), last_auto_name: None }
    }

    /// 只在 `CustomPhase::Editing` 下被调用。探测 / 创建发出时把 `phase` 推进到对应的在飞阶段。
    pub(super) fn handle_key(&mut self, key: KeyEvent, phase: &mut CustomPhase, store: &Store, s: &'static Strings) -> Option<Action> {
        let locked = self.draft.protocol.auth_locked();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let focus = self.state.focus;
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.state.step(-1, &CustomField::all(locked));
                None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.state.step(1, &CustomField::all(locked));
                None
            }
            KeyCode::Enter => match focus {
                CustomField::Protocol => Some(self.protocol_picker(s)),
                CustomField::Auth if !locked => Some(self.auth_picker(s)),
                // 锁定的 `Auth` 不在导航列表里, 焦点到不了; 万一到了, `⏎` 什么都不做。
                CustomField::Auth => None,
                CustomField::ProviderName | CustomField::BaseUrl | CustomField::MessagesPath | CustomField::ApiKey | CustomField::DisplayName => {
                    self.state.step(1, &CustomField::all(locked));
                    None
                }
                CustomField::Probe => self.submit_probe(phase, s),
                CustomField::Slot(slot) => Some(self.slot_picker(slot, s)),
                CustomField::Submit => self.submit(phase, s),
            },
            // 必须排在下面的打字分支之前, 否则会被 `ApiKey` 的编辑分支当成字符 'r' 吃掉。
            KeyCode::Char('r') if ctrl && focus == CustomField::ApiKey => {
                self.draft.api_key.toggle_reveal();
                None
            }
            // 文本行交给统一的编辑路径; 其余行没有文本字段, 按键一律吞掉。编辑 Base URL 不清
            // `probe`: `CustomDraft::models_url()` 自己按值比对, 提前清空会丢掉「改回去又生效」。
            _ => {
                let changed = edit_focused_text(self.draft.text_field(focus), &mut self.state, key);
                // 备注名跟着厂商名自动生成; 厂商名 trim 后为空时不跟 (还没打字就不该凭空冒出默认值)。
                if changed && focus == CustomField::ProviderName {
                    let name = self.draft.provider_display_name.value().trim().to_string();
                    if !name.is_empty() && follow_display_name(&mut self.draft.display_name, &mut self.last_auto_name, &name, store) {
                        self.state.clear(CustomField::DisplayName);
                    }
                }
                None
            }
        }
    }

    /// `Protocol` 行 `⏎`: label 与厂商 picker 里的 `wiz_custom_labels` 一致。
    fn protocol_picker(&self, s: &'static Strings) -> Action {
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
            initial: self.draft.protocol.as_wire().to_string(),
        })
    }

    /// `Auth` 行 `⏎` (只有未锁定时): 两个预设的头名不同, 直接拿头名当 id。
    fn auth_picker(&self, s: &'static Strings) -> Action {
        let items = ANTHROPIC_AUTH_PRESETS
            .iter()
            .zip(s.wiz_auth_labels.iter())
            .map(|((header, _format), label)| PickerItem { id: (*header).to_string(), label: (*label).to_string(), hint: None })
            .collect();
        Action::OpenPicker(PickerSpec {
            tag: PickerTag::WizardAuth,
            title: s.wiz_pick_auth.to_string(),
            items,
            allow_custom: false,
            initial: self.draft.auth_header_name.clone(),
        })
    }

    /// 槽位行 `⏎`: 候选来自探测到的模型 (探测前 / 失败时为空, 引导手输)。自定义厂商没有
    /// `example_models` 可退。
    fn slot_picker(&self, slot: Slot, s: &'static Strings) -> Action {
        let mut items = Vec::new();
        if slot == Slot::Fallback {
            items.push(PickerItem { id: String::new(), label: s.pick_clear_fallback.to_string(), hint: None });
        }
        items.extend(self.draft.slots.models.iter().map(|m| PickerItem { id: m.id.clone(), label: m.id.clone(), hint: m.display_name.clone() }));
        Action::OpenPicker(PickerSpec {
            tag: PickerTag::WizardSlot { slot },
            title: (s.wiz_pick_model)(slot_label(slot, s)),
            items,
            allow_custom: true,
            initial: self.draft.slots.slots.get(slot).to_string(),
        })
    }

    /// 换协议: 连接字段重置成新协议的预设, 清掉这几个字段的错误, 焦点若停在刚被锁定的 `Auth`
    /// 行上就挪到相邻的 `ApiKey`。**重选同一个协议什么都不做**——否则用户手填的 Base URL /
    /// 请求路径 / 鉴权会在无意中确认同一项时被弹回预设 (Anthropic 的预设 Base URL 是空串, 用户
    /// 只会看到灰字占位符, 以为那就是真值)。
    pub(super) fn apply_protocol_choice(&mut self, choice: &PickerChoice) {
        let PickerChoice::Item(id) = choice else { return };
        let Some(protocol) = CustomProtocol::ALL.iter().find(|p| p.as_wire() == id).copied() else { return };
        if protocol == self.draft.protocol {
            return;
        }
        self.draft.apply_protocol(protocol);
        self.state.clear(CustomField::BaseUrl);
        self.state.clear(CustomField::MessagesPath);
        self.state.clear(CustomField::Auth);
        if self.state.focus == CustomField::Auth && protocol.auth_locked() {
            self.state.focus = CustomField::ApiKey;
        }
    }

    pub(super) fn apply_auth_choice(&mut self, choice: &PickerChoice) {
        let PickerChoice::Item(id) = choice else { return };
        let Some((header, format)) = ANTHROPIC_AUTH_PRESETS.iter().find(|entry| entry.0 == id.as_str()) else { return };
        self.draft.auth_header_name = (*header).to_string();
        self.draft.auth_header_format = *format;
        self.state.clear(CustomField::Auth);
    }

    pub(super) fn apply_slot_choice(&mut self, slot: Slot, choice: &PickerChoice) {
        let value = match choice {
            PickerChoice::Item(id) => id.clone(),
            PickerChoice::Custom(text) => text.trim().to_string(),
        };
        self.draft.slots.slots.set(slot, value);
        self.state.clear(CustomField::Slot(slot));
    }

    /// `Probe` 行 `⏎`: 只校验 Base URL 与 API Key 非空 (与桌面端一致)。发起时清掉上一次的说明行,
    /// 否则在飞期间「上次失败的原因」和「正在获取…」会同时出现。
    fn submit_probe(&mut self, phase: &mut CustomPhase, s: &'static Strings) -> Option<Action> {
        if self.draft.base_url.value().trim().is_empty() {
            self.state.reject(CustomField::BaseUrl, s.wiz_err_base_url_empty);
            return None;
        }
        if self.draft.api_key.is_empty() {
            self.state.reject(CustomField::ApiKey, s.wiz_err_api_key);
            return None;
        }
        let cmd = WizardCmd::Probe(ProbeInput {
            base_url: self.draft.base_url.value().trim().to_string(),
            auth_header_name: self.draft.auth_header_name.clone(),
            auth_header_format: self.draft.auth_header_format,
            api_key: self.draft.api_key.secret(),
            protocol: self.draft.protocol,
        });
        self.state.clear_all();
        self.draft.slots.note = None;
        *phase = CustomPhase::Probing;
        Some(Action::WizardRequest(Box::new(cmd)))
    }

    /// `Submit`(「创建」) 行 `⏎`: 校验通过则一次创建, `model_slots` 是真实选值, `models_url` 按
    /// 「探测后 Base URL 没再改过」的规则回传。发起时同样清掉说明行。
    fn submit(&mut self, phase: &mut CustomPhase, s: &'static Strings) -> Option<Action> {
        let draft = &self.draft;
        match validate_custom(draft, s) {
            Some((field, message)) => {
                self.state.reject(field, message);
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
                self.state.clear_all();
                self.draft.slots.note = None;
                *phase = CustomPhase::Creating;
                Some(Action::WizardRequest(Box::new(cmd)))
            }
        }
    }

    pub(super) fn hints(&self, s: &'static Strings) -> Vec<Hint<'static>> {
        let mut hints = vec![("↑↓", s.key_field)];
        let locked = self.draft.protocol.auth_locked();
        match self.state.focus {
            CustomField::Protocol => hints.push(("⏎", s.key_pick)),
            CustomField::Auth if !locked => hints.push(("⏎", s.key_pick)),
            CustomField::Auth => {}
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

    /// 14 行固定内容, 不加空行分组 (80×24 下内容区还有富余)。`Auth` 行总是画出来, 锁定时只是
    /// `locked`——锁定与否不影响行数, 只影响焦点导航列表。
    pub(super) fn draw(&self, frame: &mut Frame, area: Rect, phase: CustomPhase, p: &Paint) {
        let s = p.s;
        let draft = &self.draft;
        let probing = phase == CustomPhase::Probing;
        let creating = phase == CustomPhase::Creating;
        let locked_form = probing || creating;
        let locked_auth = draft.protocol.auth_locked();

        let protocol_label = protocol_label(draft.protocol, s);
        let auth_label = format!("{} · {}", draft.auth_header_name, auth_format_label(draft.auth_header_format));
        let api_key_text = draft.api_key.display();
        let pick_hint = format!("⏎ {}", s.key_pick);
        let reveal_hint = format!("Ctrl+R {}", s.key_reveal);
        let provider_name_cursor = Some(draft.provider_display_name.visual_cursor());
        let base_url_cursor = Some(draft.base_url.visual_cursor());
        let messages_path_cursor = Some(draft.messages_path.visual_cursor());
        let api_key_cursor = Some(draft.api_key.visual_cursor());
        let display_name_cursor = Some(draft.display_name.visual_cursor());
        let field_error = |field: CustomField| self.state.error_for(field);
        let focused = |field: CustomField| self.state.focus == field;
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
            let placeholder = if slot == Slot::Fallback { s.sub_slot_unset } else { "" };
            b.push(
                field(slot_label(slot, s), draft.slots.slots.get(slot), placeholder, Some(&pick_hint), None, CustomField::Slot(slot)),
                focused(CustomField::Slot(slot)),
            );
        }
        b.push(FormRow::Button { label: if creating { s.wiz_creating } else { s.wiz_btn_create }, busy: creating }, focused(CustomField::Submit));
        let built = b.finish();

        // 单页没有步骤条, 标题换成专属的 `wiz_custom_title`。
        let view =
            FormView { title: s.wiz_custom_title, steps: None, rows: &built.rows, focus: built.focus, tick: p.tick, show_cursor: p.show_cursor };
        let _focus_rect = form::draw(frame, area, &view, p.theme, s);
    }
}

/// 协议行的展示名 `wiz_protocol_names`, 与厂商 picker 里带「自定义 · 」前缀的 `wiz_custom_labels`
/// 是两份独立文案 (标题已经写明「自定义」, 字段行不重复前缀)。不靠剥前缀派生: 那需要在代码里写
/// 中文字面量, 而且文案一改或一翻译就静默失效。两个数组顺序都与 `CustomProtocol::ALL` 一致。
fn protocol_label(protocol: CustomProtocol, s: &'static Strings) -> &'static str {
    let idx = CustomProtocol::ALL.iter().position(|p| *p == protocol).unwrap_or(0);
    s.wiz_protocol_names[idx]
}

/// 鉴权格式的展示名——英文技术词汇 (与请求头名同类), 不进 `Strings`。
fn auth_format_label(format: AuthHeaderFormat) -> &'static str {
    match format {
        AuthHeaderFormat::Bearer => "Bearer",
        AuthHeaderFormat::Raw => "Raw",
    }
}
