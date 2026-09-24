//! 内置厂商路径的第二步: 订阅已经建好, 给五个槽位选模型 → 保存。

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::Frame;

use super::fields::{validate_slots, SlotsDraft, SlotsField};
use super::form_state::FormState;
use super::Paint;
use crate::action::{Action, WizardCmd};
use crate::client::dto::Slot;
use crate::i18n::Strings;
use crate::pages::subscriptions::slot_label;
use crate::widgets::form::{self, FormBuilder, FormRow, FormView};
use crate::widgets::keybar::Hint;
use crate::widgets::picker::{PickerChoice, PickerItem, PickerSpec, PickerTag};

pub(super) struct SlotsForm {
    pub(super) draft: SlotsDraft,
    pub(super) state: FormState<SlotsField>,
    /// 厂商 yaml 里的 `example_models`: 拉不到真实候选时槽位 picker 退回到它们。
    pub(super) examples: Vec<String>,
}

impl SlotsForm {
    pub(super) fn new(draft: SlotsDraft, examples: Vec<String>) -> Self {
        Self { draft, state: FormState::new(SlotsField::Row(Slot::Fable)), examples }
    }

    /// 只在没有保存请求在飞时被调用。提交成功时把 `saving` 置真。这一步没有文本行。
    pub(super) fn handle_key(&mut self, key: KeyEvent, id: &str, saving: &mut bool, s: &'static Strings) -> Option<Action> {
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.state.step(-1, &SlotsField::ALL);
                None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.state.step(1, &SlotsField::ALL);
                None
            }
            KeyCode::Enter => match self.state.focus {
                SlotsField::Row(slot) => Some(self.slot_picker(slot, s)),
                SlotsField::Save => self.submit(id, saving, s),
            },
            _ => None,
        }
    }

    /// `Save` 行 `⏎`: 校验失败把焦点移到那个槽位; 通过则发只带 `model_slots` 的 `SaveSlots`
    /// (向导不设置 effort, 少发一个字段就不会把后端默认值清掉)。
    fn submit(&mut self, id: &str, saving: &mut bool, s: &'static Strings) -> Option<Action> {
        match validate_slots(&self.draft, s) {
            Some((slot, message)) => {
                self.state.reject(SlotsField::Row(slot), message);
                None
            }
            None => {
                self.state.clear_all();
                self.draft.note = None;
                let cmd = WizardCmd::SaveSlots { id: id.to_string(), model_slots: self.draft.slots.clone() };
                *saving = true;
                Some(Action::WizardRequest(Box::new(cmd)))
            }
        }
    }

    /// 槽位行 `⏎`: 候选是拉到的真实模型; 拉不到时退回厂商的 `example_models` (只有 id), 引导手输。
    /// 兜底槽额外在最前面放一项「清空」。
    fn slot_picker(&self, slot: Slot, s: &'static Strings) -> Action {
        let initial = self.draft.slots.get(slot).to_string();
        let mut items = Vec::new();
        if slot == Slot::Fallback {
            items.push(PickerItem { id: String::new(), label: s.pick_clear_fallback.to_string(), hint: None });
        }
        if self.draft.models.is_empty() {
            items.extend(self.examples.iter().map(|id| PickerItem { id: id.clone(), label: id.clone(), hint: None }));
        } else {
            items.extend(self.draft.models.iter().map(|m| PickerItem { id: m.id.clone(), label: m.id.clone(), hint: m.display_name.clone() }));
        }
        Action::OpenPicker(PickerSpec {
            tag: PickerTag::WizardSlot { slot },
            title: (s.wiz_pick_model)(slot_label(slot, s)),
            items,
            allow_custom: true,
            initial,
        })
    }

    /// 兜底槽的「清空」项 (`id: ""`) 与正常选值走同一条路径: 空串本来就是「未配置」。自定义输入
    /// `trim` 一下, 与订阅详情页同规则。
    pub(super) fn apply_slot_choice(&mut self, slot: Slot, choice: &PickerChoice) {
        let value = match choice {
            PickerChoice::Item(id) => id.clone(),
            PickerChoice::Custom(text) => text.trim().to_string(),
        };
        self.draft.slots.set(slot, value);
        self.state.clear(SlotsField::Row(slot));
    }

    pub(super) fn hints(&self, s: &'static Strings) -> Vec<Hint<'static>> {
        let mut hints = vec![("↑↓", s.key_field)];
        match self.state.focus {
            SlotsField::Row(_) => hints.push(("⏎", s.key_pick)),
            SlotsField::Save => hints.push(("⏎", s.wiz_btn_save)),
        }
        hints
    }

    pub(super) fn draw(&self, frame: &mut Frame, area: Rect, saving: bool, p: &Paint) {
        let s = p.s;
        let pick_hint = format!("⏎ {}", s.key_pick);

        let mut b = FormBuilder::new();
        if let Some(note) = &self.draft.note {
            b.push(FormRow::Note { text: note }, false);
            b.push(FormRow::Spacer, false);
        }
        for slot in [Slot::Fable, Slot::Opus, Slot::Sonnet, Slot::Haiku, Slot::Fallback] {
            // 兜底槽留空画成灰字「(未配置)」; 核心槽留空没有占位——`validate_slots` 保存时会拦住。
            let placeholder = if slot == Slot::Fallback { s.sub_slot_unset } else { "" };
            b.push(
                FormRow::Field {
                    label: slot_label(slot, s),
                    value: self.draft.slots.get(slot),
                    placeholder,
                    hint: Some(&pick_hint),
                    cursor: None,
                    error: self.state.error_for(SlotsField::Row(slot)),
                    locked: saving,
                },
                self.state.focus == SlotsField::Row(slot),
            );
        }
        b.push(FormRow::Spacer, false);
        b.push(FormRow::Button { label: if saving { s.wiz_saving } else { s.wiz_btn_save }, busy: saving }, self.state.focus == SlotsField::Save);
        let built = b.finish();

        let view = FormView {
            title: s.wiz_title,
            steps: Some((1, s.wiz_steps.as_slice())),
            rows: &built.rows,
            focus: built.focus,
            tick: p.tick,
            show_cursor: p.show_cursor,
        };
        let _focus_rect = form::draw(frame, area, &view, p.theme, s);
    }
}
