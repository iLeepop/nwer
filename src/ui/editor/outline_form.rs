use chrono::Utc;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Sizable as _, button::Button, button::ButtonVariants as _, h_flex,
    input::Input, input::Textarea, v_flex,
};

use crate::ui::Workspace;

/// 大纲 key-value 表单：字段名、值、删除。
pub fn render_outline_form(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    workspace.ensure_outline_form(window, cx);

    let Some(entry) = workspace.state.current_outline.as_ref() else {
        return div()
            .p_4()
            .text_color(cx.theme().muted_foreground)
            .child("选择一个大纲条目")
            .into_any_element();
    };

    let title = format!("{} · {}", entry.category.label(), entry.key);
    let field_keys: Vec<String> = entry.fields.keys().cloned().collect();

    v_flex()
        .id("outline-form")
        .size_full()
        .p_4()
        .gap_3()
        .child(div().text_lg().font_weight(FontWeight::BOLD).child(title))
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("outline-add-field")
                        .small()
                        .label("添加字段")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.prompt_add_outline_field(window, cx);
                        })),
                )
                .child(
                    Button::new("outline-save")
                        .small()
                        .label("保存")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.save_document(cx);
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("字段修改 500ms 防抖保存"),
                ),
        )
        .child(
            div()
                .id("outline-fields")
                .flex_1()
                .overflow_y_scroll()
                .child(
                    v_flex().gap_2().children(
                        field_keys
                            .into_iter()
                            .map(|key| render_field_row(workspace, &key, cx))
                            .collect::<Vec<_>>(),
                    ),
                ),
        )
        .into_any_element()
}

fn render_field_row(
    workspace: &Workspace,
    key: &str,
    cx: &mut Context<'_, Workspace>,
) -> AnyElement {
    let key_owned = key.to_string();
    let name_input = workspace.outline_field_name_inputs.get(key).cloned();
    let value_input = workspace.outline_field_value_inputs.get(key).cloned();

    h_flex()
        .id(SharedString::from(format!("outline-field-{key}")))
        .w_full()
        .gap_2()
        .items_start()
        .child(div().w(px(140.)).child(if let Some(input) = name_input {
            Input::new(&input).into_any_element()
        } else {
            div().child(key.to_string()).into_any_element()
        }))
        .child(div().flex_1().child(if let Some(input) = value_input {
            Textarea::new(&input).into_any_element()
        } else {
            div().child("").into_any_element()
        }))
        .child(
            Button::new(format!("outline-del-field-{key}"))
                .ghost()
                .xsmall()
                .label("删除")
                .on_click(cx.listener({
                    let key = key_owned;
                    move |this, _, _, cx| {
                        if let Err(err) = this.state.remove_outline_field(&key, Utc::now()) {
                            eprintln!("remove field failed: {err:#}");
                        }
                        this.invalidate_outline_inputs();
                        cx.notify();
                    }
                })),
        )
        .into_any_element()
}
