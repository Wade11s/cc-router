//! 表单的**纯渲染**: 调用方每帧把自己的状态映射成一列 [`FormRow`], 这里只管画。它不持有任何
//! 状态, 也不处理按键——文本编辑由调用方自己的 `tui_input::Input` 负责 (与 `widgets::picker`
//! 把输入框状态放在 `PickerState` 里是同一条分工)。
//!
//! 画法细节 (决定渲染字节, 与 `widgets::picker::draw` 保持一致的地方都直接照抄):
//! - 聚焦行前缀 `"▌ "`, 非聚焦行 `"  "` (与列表选中符号一致)。
//! - 标签用 [`crate::format::fit`] 定宽到 [`LABEL_COL`]; 聚焦行 `accent_bold`, 否则 `muted`。
//! - 值列宽度用 `Layout` 的 `Constraint::Min(0)` 自动吃掉 "剩余宽度 - hint 宽度 - 1 格间隔",
//!   不手算——超长的值交给 [`crate::format::fit`] 截断 (无光标行) 或 `Paragraph::scroll` 裁切
//!   (有光标行)。
//! - `error` 另起一行, 缩进到值那一列, `⚠ ` 前缀 + warn 色。
//! - `Button` 居中画 `"[ 标签 ]"`, 聚焦时整体 `REVERSED`; `busy` 时标签前面插一个 throbber 符号
//!   (`to_symbol_span` 自带的尾随空格正好当分隔)。
//! - 光标: **照抄 `widgets::picker::draw` 的算法** (Fix round F 同款坑)——可视宽度先减 1 再算
//!   滚动量, 光标 x 再 `.min(右边界 - 1)`, 否则文本正好填满输入框时光标会画在最后一个字符上面
//!   而不是紧跟其后的空位。`form.rs` 没有 `tui_input::Input` 可以借, 所以 [`visual_scroll`] 是
//!   照同一份 `char` 宽度对齐规则重写的一份, 输入换成 `FormRow::Field::cursor` 那个已经算好的
//!   显示列偏移。
//! - 步骤条 `area.width < 60` 时不画 (与总览页 logo 同一条让位原则)。

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph};
use ratatui::Frame;
use throbber_widgets_tui::{Throbber, BRAILLE_SIX};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::format::fit;
use crate::i18n::Strings;
use crate::theme::Theme;
use crate::widgets::spinner_state;

/// 标签列的显示宽度; 所有字段的值左对齐到同一条竖线。
pub const LABEL_COL: usize = 12;

/// 聚焦/非聚焦行前缀 (`"▌ "` / `"  "`) 的显示宽度。
const PREFIX_COL: u16 = 2;
/// 值列与右端 hint 之间固定留一格间隔。
const HINT_GAP: u16 = 1;
/// 步骤条低于这个宽度就不画。
const STEP_BAR_MIN_WIDTH: u16 = 60;

pub enum FormRow<'a> {
    /// 一个字段。`value` 是**已经处理好的显示文本**(掩码、占位由调用方决定)。
    Field {
        label: &'a str,
        value: &'a str,
        /// `value` 为空时画的灰字提示。
        placeholder: &'a str,
        /// 右端灰字, 比如「⏎ 选择」「Ctrl+R 显示」。
        hint: Option<&'a str>,
        /// 聚焦且可输入时, 光标在 `value` 里的**显示列**偏移 (`Input::visual_cursor()`)。
        /// `None` = 这一行不是文本输入 (选择行 / 锁定行)。
        cursor: Option<usize>,
        /// 校验失败的原因, 画在这一行正下方 (warn 色, 前缀 `⚠ `)。
        error: Option<&'a str>,
        /// 不可编辑 (锁定的鉴权头): 值画成 muted。
        locked: bool,
    },
    /// 居中的按钮。`busy` 时前面画 throbber。
    Button { label: &'a str, busy: bool },
    /// 整行说明 (muted)。
    Note { text: &'a str },
    Spacer,
}

impl FormRow<'_> {
    /// 这一行占几个终端行: 带错误信息的字段行额外占一行。
    fn height(&self) -> u16 {
        match self {
            FormRow::Field { error: Some(_), .. } => 2,
            _ => 1,
        }
    }
}

pub struct FormView<'a> {
    pub title: &'a str,
    /// 步骤指示条: `(当前步下标, 全部步骤名)`。`None` = 不画 (自定义单页)。
    pub steps: Option<(usize, &'a [&'a str])>,
    pub rows: &'a [FormRow<'a>],
    /// 聚焦的行在 `rows` 里的下标。
    pub focus: usize,
    /// 画 throbber 用 (与「重连中」同一套 `widgets::spinner_state`)。
    pub tick: u64,
}

/// 画在 `area` 里: `Block::bordered()` + `BorderType::Rounded`, `title_top` 左边是 `title`、
/// 右边是步骤条, `Padding::new(2, 2, 1, 1)`。内容超过可视高度时从顶部往下画, 放不下的那些整体
/// 截断, 在最后一行画 `s.form_more`——表单不做滚动。
///
/// **返回聚焦行的矩形** (被截断而没画出来时是 `None`)。调用方 (向导) 拿它播 `fx::field_err`
/// (Task 8) ——几何只有 `draw` 知道, 而校验失败时焦点一定就在出错的那一行, 所以一个矩形就够。
pub fn draw(frame: &mut Frame, area: Rect, view: &FormView, theme: &Theme, s: &'static Strings) -> Option<Rect> {
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style())
        .padding(Padding::new(2, 2, 1, 1))
        .title_top(format!(" {} ", view.title));
    if let Some((current, steps)) = view.steps {
        if area.width >= STEP_BAR_MIN_WIDTH {
            block = block.title_top(step_bar(current, steps, theme).right_aligned());
        }
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // 两遍: 先算总高度决定要不要给 `form_more` 留一行, 再真正画——避免"最后一行恰好卡在边界"
    // 时该不该留提示行的边界判断出错 (先量整体, 不是画一行判一次)。
    let total_height: u16 = view.rows.iter().map(FormRow::height).sum();
    let reserve_more = total_height > inner.height;
    let usable = if reserve_more { inner.height.saturating_sub(1) } else { inner.height };

    let mut y = inner.y;
    let mut used = 0u16;
    let mut focus_rect = None;
    for (i, row) in view.rows.iter().enumerate() {
        let needed = row.height();
        if used + needed > usable {
            break;
        }
        let row_rect = Rect::new(inner.x, y, inner.width, needed);
        draw_row(frame, row_rect, row, i == view.focus, theme, view.tick);
        if i == view.focus {
            focus_rect = Some(row_rect);
        }
        y += needed;
        used += needed;
    }
    if reserve_more {
        frame.render_widget(Line::raw(s.form_more).centered().style(theme.muted_style()), Rect::new(inner.x, y, inner.width, 1));
    }

    focus_rect
}

fn step_bar(current: usize, steps: &[&str], theme: &Theme) -> Line<'static> {
    // 首尾各留一格, 跟左边 `title_top(format!(" {} ", view.title))` 同一条规矩——不留白的话步骤条
    // 会贴着圆角边框的拐角画, 跟左边标题的呼吸感不一致。
    let mut spans = Vec::with_capacity(steps.len() * 2 + 2);
    spans.push(Span::raw(" "));
    for (i, step) in steps.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" ── "));
        }
        let style = if i == current { theme.accent_bold() } else { theme.muted_style() };
        spans.push(Span::styled((*step).to_string(), style));
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

fn draw_row(frame: &mut Frame, area: Rect, row: &FormRow, focused: bool, theme: &Theme, tick: u64) {
    match row {
        FormRow::Field { label, value, placeholder, hint, cursor, error, locked } => {
            let line_area = Rect { height: 1, ..area };
            draw_field_line(frame, line_area, label, value, placeholder, *hint, *cursor, *locked, focused, theme);
            if let Some(err) = error {
                let error_area = Rect { y: area.y + 1, height: 1, ..area };
                draw_field_error(frame, error_area, err, theme);
            }
        }
        FormRow::Button { label, busy } => draw_button(frame, area, label, *busy, focused, tick),
        FormRow::Note { text } => frame.render_widget(Line::raw(*text).style(theme.muted_style()), area),
        FormRow::Spacer => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_field_line(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    placeholder: &str,
    hint: Option<&str>,
    cursor: Option<usize>,
    locked: bool,
    focused: bool,
    theme: &Theme,
) {
    let prefix = if focused { "▌ " } else { "  " };
    let hint_width = hint.map(|h| h.width() as u16).unwrap_or(0);
    let label_style = if focused { theme.accent_bold() } else { theme.muted_style() };

    let [prefix_area, label_area, value_area, _gap_area, hint_area] = Layout::horizontal([
        Constraint::Length(PREFIX_COL),
        Constraint::Length(LABEL_COL as u16),
        Constraint::Min(0),
        Constraint::Length(HINT_GAP),
        Constraint::Length(hint_width),
    ])
    .areas(area);

    frame.render_widget(Span::raw(prefix), prefix_area);
    frame.render_widget(Span::styled(fit(label, LABEL_COL), label_style), label_area);

    let show_placeholder = value.is_empty();
    let text = if show_placeholder { placeholder } else { value };
    let value_style = if show_placeholder || locked { theme.muted_style() } else { Style::new() };

    if let Some(pos) = cursor {
        // Fix round F 同款坑: 可视宽度先减 1 再算滚动量, 否则文本正好填满时光标会画在最后一个
        // 字符上面而不是紧跟其后的空位。
        let visual_width = value_area.width.max(1).saturating_sub(1) as usize;
        let scroll = visual_scroll(text, pos, visual_width);
        frame.render_widget(Paragraph::new(text).style(value_style).scroll((0, scroll as u16)), value_area);
        if focused {
            let cursor_x = value_area.x + pos.saturating_sub(scroll) as u16;
            frame.set_cursor_position((cursor_x.min(value_area.right().saturating_sub(1)), value_area.y));
        }
    } else {
        frame.render_widget(Paragraph::new(fit(text, value_area.width as usize)).style(value_style), value_area);
    }

    if let Some(hint) = hint {
        frame.render_widget(Span::styled(hint, theme.muted_style()), hint_area);
    }
}

/// 照抄 `tui_input::Input::visual_scroll` 的算法 (逐字符累加宽度直到追上目标滚动量, 保证滚动
/// 永远落在字符边界上, 不会把一个宽字符从中间切开)——`form.rs` 不持有 `Input`, 只有调用方已经
/// 算好的显示列光标 (`pos`), 所以按同一份规则在这里重算一次滚动量。
fn visual_scroll(text: &str, pos: usize, width: usize) -> usize {
    let target = pos.max(width) - width;
    let mut consumed = 0usize;
    let mut chars = text.chars();
    while consumed < target {
        match chars.next() {
            Some(c) => consumed += c.width().unwrap_or(0),
            None => break,
        }
    }
    consumed
}

fn draw_field_error(frame: &mut Frame, area: Rect, error: &str, theme: &Theme) {
    let indent = PREFIX_COL + LABEL_COL as u16;
    let text_area = Rect { x: area.x + indent.min(area.width), width: area.width.saturating_sub(indent), ..area };
    frame.render_widget(Line::raw(format!("⚠ {error}")).style(Style::new().fg(theme.warn)), text_area);
}

fn draw_button(frame: &mut Frame, area: Rect, label: &str, busy: bool, focused: bool, tick: u64) {
    let text = if busy {
        let state = spinner_state(tick);
        // `to_symbol_span` 自带一个尾随空格, 正好当符号与标签之间的分隔, 不用再手动加。
        let spinner = Throbber::default().throbber_set(BRAILLE_SIX).to_symbol_span(&state);
        format!("[ {}{label} ]", spinner.content)
    } else {
        format!("[ {label} ]")
    };
    let mut line = Line::raw(text).centered();
    if focused {
        line = line.style(Style::new().add_modifier(Modifier::REVERSED));
    }
    frame.render_widget(line, area);
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::i18n::ZH;
    use crate::theme::ColorMode;

    fn field<'a>(label: &'a str, value: &'a str) -> FormRow<'a> {
        FormRow::Field { label, value, placeholder: "", hint: None, cursor: None, error: None, locked: false }
    }

    fn render(view: &FormView, width: u16, height: u16) -> String {
        let theme = Theme::new(ColorMode::TrueColor);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| { draw(frame, frame.area(), view, &theme, &ZH); }).unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn the_focused_row_is_marked_and_others_are_not() {
        let rows = vec![field("厂商", "智谱"), field("接入点", "国内版")];
        let view = FormView { title: "T", steps: None, rows: &rows, focus: 0, tick: 0 };
        let out = render(&view, 50, 12);
        let lines: Vec<&str> = out.lines().collect();

        let provider_line = lines.iter().find(|l| l.contains("智谱")).expect(&out);
        assert!(provider_line.contains('▌'), "聚焦行应该带 ▌ 前缀\n{out}");

        let endpoint_line = lines.iter().find(|l| l.contains("国内版")).expect(&out);
        assert!(!endpoint_line.contains('▌'), "非聚焦行不该带 ▌ 前缀\n{out}");
    }

    #[test]
    fn an_empty_value_shows_the_placeholder() {
        let rows = vec![FormRow::Field {
            label: "备注名",
            value: "",
            placeholder: "留空自动生成",
            hint: None,
            cursor: None,
            error: None,
            locked: false,
        }];
        let view = FormView { title: "T", steps: None, rows: &rows, focus: 0, tick: 0 };
        let out = render(&view, 50, 12);
        assert!(out.contains("留空自动生成"), "{out}");
    }

    #[test]
    fn a_field_error_is_drawn_under_its_row() {
        let rows = vec![FormRow::Field {
            label: "API Key",
            value: "",
            placeholder: "",
            hint: None,
            cursor: None,
            error: Some("API Key 不能为空"),
            locked: false,
        }];
        let view = FormView { title: "T", steps: None, rows: &rows, focus: 0, tick: 0 };
        let out = render(&view, 50, 12);
        let lines: Vec<&str> = out.lines().collect();

        let label_idx = lines.iter().position(|l| l.contains("API Key")).expect(&out);
        assert!(lines[label_idx + 1].contains("⚠ API Key 不能为空"), "错误应该画在字段行正下方\n{out}");
    }

    #[test]
    fn the_step_bar_disappears_on_a_narrow_area() {
        // 用短 ASCII 步骤名 (不是真实 i18n 文案), 避开宽字符/圆圈数字的显示宽度歧义——这里只关心
        // "够不够 60 列就画不画", 不关心真实文案在边界宽度下会不会被裁切。
        let rows = vec![field("厂商", "智谱")];
        let steps: [&str; 2] = ["STEP1", "STEP2"];
        let view = FormView { title: "新建订阅", steps: Some((0, &steps)), rows: &rows, focus: 0, tick: 0 };

        let wide = render(&view, 60, 10);
        assert!(wide.contains("STEP1"), "宽度够时应该画步骤条\n{wide}");

        let narrow = render(&view, 59, 10);
        assert!(!narrow.contains("STEP1"), "宽度不够时步骤条应该让位\n{narrow}");
    }
}
