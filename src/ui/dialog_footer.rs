//! Dialog 底部确认栏。
//!
//! gpui-component 的 `Dialog` 在重构后不再根据 `button_props` 自动渲染 OK/Cancel；
//! `button_props` 只绑定 Enter/Esc 回调。可见按钮须显式挂 `DialogFooter`。

use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dialog::{DialogAction, DialogClose, DialogFooter},
};

pub fn dialog_ok_cancel_footer(
    ok_label: impl Into<SharedString>,
    cancel_label: impl Into<SharedString>,
) -> DialogFooter {
    DialogFooter::new()
        .child(DialogClose::new().child(Button::new("dialog-cancel").label(cancel_label)))
        .child(
            DialogAction::new().child(Button::new("dialog-ok").primary().label(ok_label)),
        )
}
