//! 内置厂商路径的第二步: 订阅已经建好, 给五个槽位选模型 → 保存。

use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;

use super::common::{self, FieldKind, FormFields, KeyOutcome, RowHints, Rows};
use super::fields::{validate_slots, SlotsDraft, SlotsField};
use super::form_state::FormState;
use super::text::TextInput;
use super::Paint;
use crate::action::{Action, WizardCmd};
use crate::client::dto::Slot;
use crate::i18n::Strings;
use crate::widgets::form::{self, FormView};
use crate::widgets::keybar::Hint;
use crate::widgets::picker::PickerChoice;

pub(super) struct SlotsForm {
    pub(super) draft: SlotsDraft,
    pub(super) state: FormState<SlotsField>,
    /// 厂商 yaml 里的 `example_models`: 拉不到真实候选时槽位选择器退回到它们。
    pub(super) examples: Vec<String>,
    /// 自动获取模型失败 / 上一次保存失败的原因, 挂在表单顶部。
    pub(super) note: Option<String>,
}

impl FormFields for SlotsForm {
    type Field = SlotsField;

    fn state(&self) -> &FormState<SlotsField> {
        &self.state
    }

    fn state_mut(&mut self) -> &mut FormState<SlotsField> {
        &mut self.state
    }

    fn order(&self) -> Vec<SlotsField> {
        SlotsField::ALL.to_vec()
    }

    fn kind(&self, field: SlotsField, s: &'static Strings) -> FieldKind {
        match field {
            SlotsField::Row(_) => FieldKind::Pick,
            SlotsField::Save => FieldKind::Button(s.wiz_btn_save),
        }
    }

    fn text_field(&mut self, _field: SlotsField) -> Option<&mut dyn TextInput> {
        None
    }
}

impl SlotsForm {
    pub(super) fn new(draft: SlotsDraft, examples: Vec<String>, note: Option<String>) -> Self {
        Self { draft, state: FormState::new(SlotsField::Row(Slot::Fable)), examples, note }
    }

    /// 只在没有保存请求在飞时被调用。提交成功时把 `saving` 置真。
    pub(super) fn handle_key(&mut self, key: KeyEvent, id: &str, saving: &mut bool, s: &'static Strings) -> Option<Action> {
        match common::handle_key(self, key, s) {
            KeyOutcome::Handled | KeyOutcome::Edited(_) => None,
            KeyOutcome::Activate(SlotsField::Row(slot)) => {
                Some(common::slot_picker(slot, self.draft.slots.get(slot), &self.draft.models, &self.examples, s))
            }
            KeyOutcome::Activate(SlotsField::Save) => self.submit(id, saving, s),
        }
    }

    /// `Save` 行 `⏎`: 校验通过则发只带 `model_slots` 的 `SaveSlots` (向导不设置 effort, 少发一个
    /// 字段就不会把后端默认值清掉)。
    fn submit(&mut self, id: &str, saving: &mut bool, s: &'static Strings) -> Option<Action> {
        let failure = validate_slots(&self.draft, s).map(|(slot, message)| (SlotsField::Row(slot), message));
        if !self.state.validate(failure) {
            return None;
        }
        self.note = None;
        let cmd = WizardCmd::SaveSlots { id: id.to_string(), model_slots: self.draft.slots.clone() };
        *saving = true;
        Some(Action::WizardRequest(Box::new(cmd)))
    }

    pub(super) fn apply_slot_choice(&mut self, slot: Slot, choice: &PickerChoice) {
        self.draft.slots.set(slot, common::slot_choice_value(choice));
        self.state.clear(SlotsField::Row(slot));
    }

    pub(super) fn hints(&self, s: &'static Strings) -> Vec<Hint<'static>> {
        common::hints(self, s)
    }

    pub(super) fn draw(&self, frame: &mut Frame, area: Rect, saving: bool, p: &Paint) {
        let s = p.s;
        let hints = RowHints::new(s);
        let mut rows = Rows::new(self, &hints, s, saving);
        rows.note(self.note.as_deref());
        rows.slots(&self.draft.slots, SlotsField::Row);
        rows.spacer();
        rows.button(SlotsField::Save, saving.then_some(s.wiz_saving));
        let built = rows.finish();

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
