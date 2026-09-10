//! SVG output, following `renderDrawTop_` / `renderDrawRight_` /
//! `renderDrawBottom_` / `renderDrawLeft_`.

use crate::matrix;
use crate::measure::{self, BlockLayout, StackLayout};
use crate::model::{BlockSpec, DocumentSpec, SegmentSpec};
use crate::svg::*;
use crate::theme::{colours_for, connection_colour};

/// Gap between two top-level scripts on the page.
const SCRIPT_GAP: f32 = 24.0;

pub fn render_document(document: &DocumentSpec) -> String {
    let scale = document.scale.unwrap_or(1.0).max(0.1);
    let theme = document.theme.as_deref().unwrap_or("normal");

    let mut body = String::new();
    let mut total_width = 0.0f32;
    let mut total_height = 0.0f32;

    for (index, script) in document.scripts.iter().enumerate() {
        if index > 0 {
            total_height += SCRIPT_GAP;
        }
        let mut blocks = script.blocks.clone();
        for block in blocks.iter_mut() {
            matrix::expand(block);
        }
        let layout = measure::layout_stack(&blocks);

        // A value block's plug hangs into negative x. Shifting the script
        // right by one tab keeps it inside the viewBox; the width already
        // includes the tab, so it must not be added a second time.
        let offset_x = if starts_with_output(&layout) { TAB_WIDTH } else { 0.0 };

        body.push_str(&format!(
            "<g transform=\"translate({} {})\">{}</g>",
            fmt(offset_x),
            fmt(total_height),
            render_stack(&blocks, &layout, theme)
        ));

        total_width = total_width.max(layout.width);
        total_height += layout.height;
    }

    if document.scripts.is_empty() {
        total_width = 1.0;
        total_height = 1.0;
    }

    // One pixel of air so a stroke or the bottom notch is never clipped.
    total_width += 1.0;
    total_height += 1.0;

    let width = total_width * scale;
    let height = total_height * scale;

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">{defs}<g transform=\"scale({scale})\">{body}</g></svg>",
        w = fmt(width),
        h = fmt(height),
        defs = defs(&document.font, theme),
        scale = fmt(scale),
    )
}

fn starts_with_output(stack: &StackLayout) -> bool {
    stack.blocks.first().map(|(_, b)| b.has_output).unwrap_or(false)
}

/// `.blocklyText` and the editable-field rules from Open Roberta's `css.js`.
fn defs(font: &str, theme: &str) -> String {
    let stack = css_font_stack(font);
    let label_fill = if theme == "print" { "#000" } else { "#fff" };
    format!(
        "<style>\
.nepo-label{{font-family:{stack};font-size:{size}px;fill:{label_fill}}}\
.nepo-label.mono{{font-family:\"Courier New\",Courier,monospace}}\
.nepo-field-text{{font-family:{stack};font-size:{size}px;fill:#000}}\
.nepo-field-text.mono{{font-family:\"Courier New\",Courier,monospace}}\
</style>",
        size = fmt(FONT_SIZE_PX),
    )
}

fn css_font_stack(font: &str) -> String {
    let font = font.trim();
    if font.is_empty() {
        return "sans-serif".to_string();
    }
    if font.contains(',') || font.starts_with('"') || font.starts_with('\'') {
        return font.to_string();
    }
    format!("\"{}\", sans-serif", font.replace('"', ""))
}

fn render_stack(blocks: &[BlockSpec], stack: &StackLayout, theme: &str) -> String {
    let mut svg = String::new();
    for (block, (y, layout)) in blocks.iter().zip(stack.blocks.iter()) {
        svg.push_str(&format!(
            "<g transform=\"translate(0 {})\">{}</g>",
            fmt(*y),
            render_block(block, layout, theme)
        ));
    }
    svg
}

fn render_block(block: &BlockSpec, layout: &BlockLayout, theme: &str) -> String {
    let colours = colours_for(&layout.category, theme);
    let mut steps: Vec<String> = Vec::new();

    draw_top(
        &mut steps,
        layout.right_edge,
        layout.square_top_left,
        layout.has_previous,
    );

    let mut overlays = String::new();
    let mut children = String::new();
    let mut fields = String::new();

    for (index, row) in layout.rows.iter().enumerate() {
        let spec_row = &block.rows[index];

        if row.lead_gap > 0.0 {
            vertical(&mut steps, row.lead_gap);
        }

        // Fields first, so the outline below can consume the row.
        for (position, field) in row.fields.iter().enumerate() {
            fields.push_str(&render_field(
                &spec_row.fields[position],
                field.x,
                field.y,
                field.width,
                &colours,
                colours.fill,
            ));
        }

        match row.kind.as_str() {
            "value" => {
                steps.push(TAB_PATH_DOWN_INNER.to_string());
                vertical(&mut steps, row.height - TAB_HEIGHT);

                match (row.value.as_ref(), spec_row.value.as_ref()) {
                    (Some(child_layout), Some(child_spec)) => {
                        children.push_str(&format!(
                            "<g transform=\"translate({} {})\">{}</g>",
                            fmt(row.value_origin.0),
                            fmt(row.value_origin.1),
                            render_block(child_spec, child_layout, theme)
                        ));
                    }
                    _ => {
                        // An empty socket announces the type it wants.
                        if let Some(check) = row.check.as_deref() {
                            overlays.push_str(&format!(
                                "<path d=\"M {},{} {}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
                                fmt(layout.right_edge),
                                fmt(row.y + 2.6),
                                INPUT_TYPE_PATH,
                                connection_colour(check, theme),
                                INPUT_TYPE_STROKE_WIDTH,
                            ));
                        }
                    }
                }
            }
            "statement" => {
                horizontal_to(&mut steps, layout.statement_edge + NOTCH_WIDTH + 1.0);
                steps.push(inner_top_left_corner());
                vertical(&mut steps, row.height - 2.0 * CORNER_RADIUS);
                steps.push(inner_bottom_left_corner());
                horizontal_to(&mut steps, layout.right_edge);

                if let (Some(body_layout), false) = (row.body.as_ref(), spec_row.body.is_empty()) {
                    children.push_str(&format!(
                        "<g transform=\"translate({} {})\">{}</g>",
                        fmt(row.body_origin.0),
                        fmt(row.body_origin.1),
                        render_stack(&spec_row.body, body_layout, theme)
                    ));
                }
            }
            _ => {
                vertical(&mut steps, row.height);
            }
        }

        if row.trail_gap > 0.0 {
            vertical(&mut steps, row.trail_gap);
        }
    }

    if layout.rows.is_empty() {
        steps.push(format!("V {}", fmt(layout.bottom)));
    }

    draw_bottom(&mut steps, layout.square_bottom_left, layout.has_next);

    // A typed output plug is a separate path in the colour of the data type,
    // so the main outline stops at the top of the tab.
    let typed_output = layout.has_output && layout.output_check.is_some();
    if typed_output {
        steps.push(format!("V {}", fmt(TAB_HEIGHT)));
        steps.push("z".to_string());
    } else {
        draw_left(&mut steps, layout.has_output);
    }

    let stroke = match colours.stroke {
        Some(colour) => format!(" stroke=\"{colour}\" stroke-width=\"1\""),
        None => String::new(),
    };

    let mut svg = format!(
        "<path d=\"{}\" fill=\"{}\"{}/>",
        steps.join(" "),
        colours.fill,
        stroke
    );

    if typed_output {
        let check = layout.output_check.as_deref().unwrap_or("");
        svg.push_str(&format!(
            "<path d=\"M 0,{} {} z\" fill=\"{}\"{}/>",
            fmt(TAB_HEIGHT),
            TAB_PATH_DOWN_OUTER,
            connection_colour(check, theme),
            stroke
        ));
    }

    svg.push_str(&overlays);
    svg.push_str(&fields);
    svg.push_str(&children);
    svg
}

fn render_field(
    segment: &SegmentSpec,
    x: f32,
    y: f32,
    width: f32,
    colours: &crate::theme::BlockColours,
    block_fill: &str,
) -> String {
    let mono = matches!(segment, SegmentSpec::Text { monospace: true, .. });
    let baseline = y + FIELD_TEXT_BASELINE;

    match segment {
        SegmentSpec::Text { .. } => format!(
            "<text class=\"nepo-label{}\" x=\"{}\" y=\"{}\">{}</text>",
            if mono { " mono" } else { "" },
            fmt(x),
            fmt(baseline),
            escape_text(&measure::display_text(segment))
        ),
        SegmentSpec::ColourField { colour } => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{r}\" ry=\"{r}\" fill=\"{colour}\" fill-opacity=\"1\"/>",
            fmt(x + FIELD_BOX_X),
            fmt(y),
            fmt(FIELD_BOX_PAD),
            fmt(FIELD_BOX_HEIGHT),
            r = fmt(FIELD_BOX_RADIUS),
        ),
        SegmentSpec::PixelCell { value } => {
            let mut svg = format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{r}\" ry=\"{r}\" fill=\"{}\" fill-opacity=\"{}\"/>",
                fmt(x + FIELD_BOX_X),
                fmt(y),
                fmt(PIXEL_BOX_SIZE),
                fmt(FIELD_BOX_HEIGHT),
                colours.field_fill,
                colours.field_fill_opacity,
                r = fmt(FIELD_BOX_RADIUS),
            );
            if !value.is_empty() {
                svg.push_str(&format!(
                    "<text class=\"nepo-field-text mono\" x=\"{}\" y=\"{}\">{}</text>",
                    fmt(x),
                    fmt(baseline),
                    escape_text(value)
                ));
            }
            svg
        }
        SegmentSpec::Dropdown { .. } => {
            let value = measure::display_text(segment);
            let value = value.trim_end_matches(['\u{25BE}', ' ']);
            let box_svg = format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{r}\" ry=\"{r}\" fill=\"{}\" fill-opacity=\"{}\"/>",
                fmt(x + FIELD_BOX_X),
                fmt(y),
                fmt(width + FIELD_BOX_PAD),
                fmt(FIELD_BOX_HEIGHT),
                colours.field_fill,
                colours.field_fill_opacity,
                r = fmt(FIELD_BOX_RADIUS),
            );
            // `FieldDropdown.render_` tints the arrow with the block's colour.
            format!(
                "{box_svg}<text class=\"nepo-field-text\" x=\"{}\" y=\"{}\">{}<tspan fill=\"{block_fill}\"> \u{25BE}</tspan></text>",
                fmt(x),
                fmt(baseline),
                escape_text(value)
            )
        }
        SegmentSpec::Input { .. } => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{r}\" ry=\"{r}\" fill=\"{}\" fill-opacity=\"{}\"/><text class=\"nepo-field-text\" x=\"{}\" y=\"{}\">{}</text>",
            fmt(x + FIELD_BOX_X),
            fmt(y),
            fmt(width + FIELD_BOX_PAD),
            fmt(FIELD_BOX_HEIGHT),
            colours.field_fill,
            colours.field_fill_opacity,
            fmt(x),
            fmt(baseline),
            escape_text(&measure::display_text(segment)),
            r = fmt(FIELD_BOX_RADIUS),
        ),
        SegmentSpec::Icon { .. } | SegmentSpec::Block { .. } | SegmentSpec::PixelMatrix { .. } => {
            String::new()
        }
    }
}
