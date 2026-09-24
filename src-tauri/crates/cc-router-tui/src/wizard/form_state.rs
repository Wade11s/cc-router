//! 一张表单的焦点与校验错误。三张表单 (`Basics` / `Slots` / `Custom`) 各持一份, 取代以前
//! `focus`/`slots_focus`/`custom_focus` 与 `field_error`/`slot_error`/`custom_field_error` 三对
//! 字段和三套 `move_*`/`clear_*` 方法。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormState<F> {
    /// 当前聚焦的字段。
    pub focus: F,
    /// 校验失败的字段与原因 (原因是 `Strings` 的字段), 画成那一行下面的 `⚠` 提示。同一时刻最多
    /// 一条——校验按顺序报第一个不合法的字段。
    pub error: Option<(F, &'static str)>,
}

impl<F: Copy + Eq> FormState<F> {
    pub fn new(focus: F) -> Self {
        Self { focus, error: None }
    }

    /// 在 `fields` (可聚焦字段, 顺序即上下键顺序) 里移动 `delta` 格, 两端夹住、不绕回。当前焦点
    /// 不在 `fields` 里时从第一项算起。
    pub fn step(&mut self, delta: isize, fields: &[F]) {
        let Some(last) = fields.len().checked_sub(1) else { return };
        let idx = fields.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = (idx as isize + delta).clamp(0, last as isize) as usize;
        self.focus = fields[next];
    }

    /// 清掉**这个字段自己**的错误 (评审 M3)——别的字段的错误不动, 不然改一个字段会把提交时挂在
    /// 另一个字段上的提示也一并抹掉, 反而让用户以为它也修好了。
    pub fn clear(&mut self, field: F) {
        if self.error.is_some_and(|(f, _)| f == field) {
            self.error = None;
        }
    }

    /// 校验通过、请求发出时整张表单不再有错误。
    pub fn clear_all(&mut self) {
        self.error = None;
    }

    /// 校验失败: 挂上错误, 并把焦点移到出错的字段 (Task 8 在这一行播 `fx::field_err`)。
    pub fn reject(&mut self, field: F, message: &'static str) {
        self.focus = field;
        self.error = Some((field, message));
    }

    /// `field` 自己的错误 (没有 / 挂在别的字段上都是 `None`)。
    pub fn error_for(&self, field: F) -> Option<&'static str> {
        self.error.and_then(|(f, message)| (f == field).then_some(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELDS: [char; 3] = ['a', 'b', 'c'];

    #[test]
    fn step_clamps_at_both_ends() {
        let mut form = FormState::new('a');
        form.step(-1, &FIELDS);
        assert_eq!(form.focus, 'a', "到顶不绕回");
        form.step(1, &FIELDS);
        assert_eq!(form.focus, 'b');
        form.step(5, &FIELDS);
        assert_eq!(form.focus, 'c', "到底不绕回");
    }

    /// 焦点不在列表里 (比如锁定的 `Auth` 行被剔除之后) 从第一项算起; 空列表什么都不做。
    #[test]
    fn step_starts_from_the_first_field_when_the_focus_is_not_listed() {
        let mut form = FormState::new('z');
        form.step(1, &FIELDS);
        assert_eq!(form.focus, 'b');

        let mut empty = FormState::new('z');
        empty.step(1, &[]);
        assert_eq!(empty.focus, 'z');
    }

    #[test]
    fn reject_sets_the_error_and_moves_the_focus() {
        let mut form = FormState::new('a');
        form.reject('c', "不能为空");
        assert_eq!(form.focus, 'c');
        assert_eq!(form.error, Some(('c', "不能为空")));
        assert_eq!(form.error_for('c'), Some("不能为空"));
        assert_eq!(form.error_for('a'), None);
    }

    #[test]
    fn clear_only_clears_its_own_field() {
        let mut form = FormState::new('a');
        form.reject('b', "错");
        form.clear('a');
        assert_eq!(form.error, Some(('b', "错")), "清别的字段不该动 b 的错误");
        form.clear('b');
        assert_eq!(form.error, None);

        form.reject('c', "错");
        form.clear_all();
        assert_eq!(form.error, None);
    }
}
