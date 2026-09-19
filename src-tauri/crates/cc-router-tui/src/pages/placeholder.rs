//! 还没做的页面 (订阅 / 虚拟模型 / 实时路由 / 日志) 的占位, 后续阶段逐个替换掉。

use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Rect};
use ratatui::text::Line;
use ratatui::Frame;

use super::{Component, DrawCtx, StreamEvent};
use crate::action::{Action, Cmd};
use crate::i18n::Strings;
use crate::store::Store;
use crate::widgets::keybar::Hint;

#[derive(Debug, Default)]
pub struct Placeholder {
    /// 测试专用钩子: 生产代码永远不置位, 只有 `app.rs::tests` 里验证「未保存修改 → 先确认」
    /// 流程的用例会调 [`Placeholder::set_force_dirty`]。`#[cfg(test)]` 意味着这个字段/方法只在
    /// `cargo test -p cc-router-tui` 编译本 crate 的单测 (`src/` 下的 `mod tests`) 时存在——
    /// `tests/ui.rs` 之类的集成测试链接的是不带 `--cfg test` 的普通 lib, 看不到它, 也就没有
    /// 任何办法把这个开关误用在生产路径上 (不给生产代码加 feature / 公开 API)。
    #[cfg(test)]
    force_dirty: bool,
    /// 测试专用记录器 (Task 2): `on_event` 收到 `Message` 记事件名, 收到 `Lost` 记 `"<lost>"`——
    /// 与 `force_dirty` 同一条「仅单测可见」规则, Task 8 连同 `Placeholder` 一起删掉。
    /// `pub(crate)`: `app.rs::tests` 直接读 `a.pages.placeholder.seen`, 不经过访问方法。
    #[cfg(test)]
    pub(crate) seen: Vec<String>,
}

#[cfg(test)]
impl Placeholder {
    pub fn set_force_dirty(&mut self, dirty: bool) {
        self.force_dirty = dirty;
    }
}

impl Component for Placeholder {
    fn handle_key(&mut self, _key: KeyEvent, _store: &Store, _s: &'static Strings) -> Option<Action> {
        None
    }

    fn update(&mut self, _action: &Action, _store: &Store, _s: &'static Strings) -> Vec<Cmd> {
        Vec::new()
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &mut DrawCtx) {
        let line = Line::styled(ctx.s.coming_soon, ctx.theme.muted_style()).centered();
        frame.render_widget(line, area.centered_vertically(Constraint::Length(1)));
    }

    fn hints(&self, _s: &'static Strings) -> Vec<Hint<'static>> {
        Vec::new()
    }

    fn is_dirty(&self) -> bool {
        #[cfg(test)]
        {
            self.force_dirty
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    fn discard_changes(&mut self) {
        #[cfg(test)]
        {
            self.force_dirty = false;
        }
    }

    fn on_event(&mut self, ev: StreamEvent<'_>) {
        #[cfg(test)]
        {
            self.seen.push(match ev {
                StreamEvent::Message { name, .. } => name.to_string(),
                StreamEvent::Lost { .. } => "<lost>".to_string(),
            });
        }
        #[cfg(not(test))]
        {
            let _ = ev;
        }
    }
}
