//! 可选中复制的纯文本 / Markdown 渲染。

use gpui::{ElementId, SharedString};
use gpui_component::text::TextView;

/// 将内容按 Markdown 渲染，但转义特殊字符，复制结果为原文。
pub fn selectable_plain(id: impl Into<ElementId>, text: impl Into<SharedString>) -> TextView {
    TextView::markdown(id, escape_markdown(text.into()))
        .selectable(true)
}

/// 将内容按 Markdown 渲染并支持框选复制（保留 Markdown 语法）。
pub fn selectable_markdown(id: impl Into<ElementId>, text: impl Into<SharedString>) -> TextView {
    TextView::markdown(id, text).selectable(true)
}

fn escape_markdown(text: SharedString) -> SharedString {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' | '*' | '_' | '`' | '[' | ']' | '#' | '|' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => {
                out.push_str("  \n");
            }
            _ => out.push(c),
        }
    }
    out.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_markdown_preserves_newlines_as_hard_breaks() {
        let escaped = escape_markdown("a\nb".into());
        assert_eq!(escaped.as_ref(), "a  \nb");
    }

    #[test]
    fn escape_markdown_escapes_emphasis_markers() {
        let escaped = escape_markdown("*bold*".into());
        assert_eq!(escaped.as_ref(), "\\*bold\\*");
    }
}
