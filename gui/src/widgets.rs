//! Small layout helpers shared by the pages.

use crate::prelude::*;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::theme::Colors;

/// A rounded group of rows.
pub fn card(cx: &App) -> Div {
    v_flex().bg(Colors::card()).rounded(cx.theme().radius_lg).overflow_hidden()
}

pub fn heading(text: impl Into<SharedString>) -> Div {
    div().px_0p5().text_sm().font_bold().child(text.into())
}

/// A sub-heading inside a card.
pub fn subheading(text: impl Into<SharedString>, cx: &App) -> Div {
    div().px_3().pt_2p5().pb_1().text_xs().font_semibold().text_color(cx.theme().muted_foreground).child(text.into())
}

/// A row with a label (and an optional explanation) on the left and `control` on the right.
pub fn row(label: impl Into<SharedString>, sub: Option<&str>, control: impl IntoElement, cx: &App) -> Div {
    h_flex()
        .min_h(px(38.))
        .px_3()
        .py_1()
        .gap_3()
        .border_t_1()
        .border_color(Colors::line())
        .text_sm()
        .child(
            v_flex()
                .flex_1()
                .child(label.into())
                .children(sub.map(|s| div().text_xs().text_color(cx.theme().muted_foreground).child(s.to_owned()))),
        )
        .child(control)
}

/// A slider with its value shown to the right.
pub fn slider_row(
    label: impl Into<SharedString>,
    state: &Entity<SliderState>,
    value: impl Into<SharedString>,
    disabled: bool,
    cx: &App,
) -> Div {
    slider_row_sub(label, None, state, value, disabled, cx)
}

/// Like [`slider_row`], with an explanation under the label.
pub fn slider_row_sub(
    label: impl Into<SharedString>,
    sub: Option<&str>,
    state: &Entity<SliderState>,
    value: impl Into<SharedString>,
    disabled: bool,
    cx: &App,
) -> Div {
    row(
        label,
        sub,
        h_flex().gap_3().child(div().w(px(170.)).child(Slider::new(state).disabled(disabled))).child(
            div()
                .w(px(64.))
                .text_right()
                .text_xs()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().muted_foreground)
                .child(value.into()),
        ),
        cx,
    )
}

/// An exclusive choice between a few options.
pub fn segmented(
    id: impl Into<ElementId>,
    options: &[&str],
    selected: Option<usize>,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> TabBar {
    TabBar::new(id)
        .segmented()
        .small()
        .selected_index(selected.unwrap_or(usize::MAX))
        .children(options.iter().map(|o| Tab::new().label(SharedString::from((*o).to_owned()))))
        .on_click(move |i: &usize, window, cx| on_select(*i, window, cx))
}

/// Marks a setting with unsaved changes.
pub fn edited_dot() -> Div {
    div().size(px(6.)).rounded_full().bg(Colors::edited())
}

pub fn note(text: impl Into<SharedString>, cx: &App) -> Div {
    div().px_0p5().text_xs().text_color(cx.theme().muted_foreground).child(text.into())
}

/// Two columns of equal width.
pub fn columns(left: impl IntoElement, right: impl IntoElement) -> Div {
    h_flex()
        .w_full()
        .items_start()
        .gap_4()
        .child(div().flex_1().min_w_0().child(left))
        .child(div().flex_1().min_w_0().child(right))
}

/// Percentage of a raw value in `0..=max`.
pub fn percent(v: u8, max: u8) -> String {
    let max = u32::from(max.max(1));
    let p = u32::from(v).saturating_mul(100).checked_div(max).unwrap_or(0);
    format!("{p}%")
}

/// Clamps a slider value to a byte.
pub fn byte(v: f32) -> u8 {
    crate::model::unit_to_u8(v)
}
