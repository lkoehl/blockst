//! What comes out of the renderer: valid SVG, and Open Roberta's own colours.

use nepo_wasm::parser;
use nepo_wasm::theme;

const PROGRAM: &str = "Start
  Zeige Text \"Hallo\"
  Schalte RGB LED an (#ff0000)
  Wiederhole unendlich oft
    Zeige Bild (.#.#.|.#.#.|.....|#...#|.###.)
  Ende";

fn render(source: &str) -> String {
    render_with(source, "normal")
}

fn render_with(source: &str, theme: &str) -> String {
    let request = serde_json::json!({
        "code": source,
        "language": "de",
        "platform": "calliope",
        "theme": theme,
    });
    parser::render_request(&request.to_string()).expect("render")
}

#[test]
fn the_svg_carries_no_invalid_measurements() {
    let svg = render(PROGRAM);
    for bad in ["NaN", "Infinity", "inf", "null", "undefined"] {
        assert!(!svg.contains(bad), "output contains {bad}");
    }
    // A stray `-` or a lone `.` would break a path. Splitting on whitespace is
    // not enough: Blockly's own fragments run numbers and commands together
    // ("l-5.154 -0.469c0 0"), so the path is scanned the way an SVG parser
    // scans it.
    for path in paths(&svg) {
        for number in numbers(&path) {
            assert!(
                number.parse::<f64>().is_ok(),
                "unparseable number {number:?} in path {path}"
            );
        }
    }
}

#[test]
fn no_placeholder_survives_into_the_output() {
    let svg = render(PROGRAM);
    for name in ["%TYPE", "%OUT", "%COLOR", "%VALUE", "%DO", "%PICTURE", "%TEXT"] {
        assert!(!svg.contains(name), "unresolved placeholder {name}");
    }
}

#[test]
fn the_svg_is_balanced() {
    let svg = render(PROGRAM);
    assert!(svg.starts_with("<svg "), "not an svg root");
    assert!(svg.ends_with("</svg>"), "unterminated svg");
    let opens = svg.matches("<g ").count();
    let closes = svg.matches("</g>").count();
    assert_eq!(opens, closes, "unbalanced groups");
    assert_eq!(
        svg.matches("<text").count(),
        svg.matches("</text>").count(),
        "unbalanced text elements"
    );
    // Self-closing elements must actually close.
    assert_eq!(svg.matches("<path").count(), svg.matches("/>").count() - svg.matches("<rect").count());
}

#[test]
fn categories_use_open_robertas_own_colours() {
    let svg = render(PROGRAM);
    // CAT_ACTIVITY_RGB, CAT_ACTION_RGB, CAT_CONTROL_RGB, CAT_IMAGE_RGB
    for (category, expected) in [
        ("activity", "#E2001A"),
        ("action", "#F29400"),
        ("control", "#EB6A0A"),
        ("image", "#DF01D7"),
        ("colour", "#EBC300"),
        ("text", "#BACC1E"),
    ] {
        assert_eq!(theme::category_colour(category), expected);
        assert!(
            svg.contains(&format!("fill=\"{expected}\"")),
            "{category} block missing from the output"
        );
    }
}

#[test]
fn a_typed_plug_takes_the_colour_of_its_data_type() {
    // A Boolean sensor on an olive body must still plug in cyan.
    let svg = render("Taste A gedrückt?");
    assert!(svg.contains("fill=\"#8FA402\""), "sensor body");
    assert!(svg.contains("fill=\"#33B8CA\""), "Boolean plug");
    assert_eq!(theme::data_type_colour("Boolean"), "#33B8CA");
}

#[test]
fn an_empty_socket_is_traced_in_its_type_colour() {
    let svg = render("Zeige Text");
    assert!(
        svg.contains("stroke=\"#BACC1E\""),
        "empty String socket should be outlined in the String colour"
    );
    let filled = render("Zeige Text \"Hallo\"");
    assert!(
        !filled.contains("stroke=\"#BACC1E\""),
        "a filled socket needs no type hint"
    );
}

#[test]
fn blocks_are_flat_fills_in_the_normal_theme() {
    // Open Roberta's stylesheet has no `.blocklyPath` stroke rule, and the
    // fork comments out the highlight and shadow paths. A stroke here would
    // be a Scratch habit leaking in.
    let svg = render(PROGRAM);
    assert!(!svg.contains("stroke=\"#000000\""), "unexpected outline");
}

#[test]
fn the_print_theme_outlines_instead_of_filling() {
    let svg = render_with(PROGRAM, "print");
    assert!(svg.contains("fill=\"#ffffff\""));
    assert!(svg.contains("stroke=\"#000000\""));
    assert!(!svg.contains("fill=\"#F29400\""), "colour survived into print");
}

#[test]
fn every_prototype_block_renders() {
    for source in [
        "Start",
        "Zeige Text \"Hallo\"",
        "Schalte RGB LED an (#ff0000)",
        "Taste A gedrückt?",
        "Wiederhole unendlich oft\n  Zeige Text \"Hallo\"\nEnde",
        "Zeige Bild (.#.#.|.#.#.|.....|#...#|.###.)",
    ] {
        let svg = render(source);
        assert!(svg.len() > 200, "suspiciously small output for {source:?}");
        assert!(svg.contains("<path"), "no outline for {source:?}");
    }
}

#[test]
fn the_matrix_paints_twenty_five_cells() {
    let svg = render("Zeige Bild (.#.#.|.#.#.|.....|#...#|.###.)");
    // 25 pixel boxes, all 16 wide.
    let cells = svg.matches("width=\"16\" height=\"16\"").count();
    assert_eq!(cells, 25);
    // Nine lit LEDs in this face: two eyes twice, then the mouth.
    let lit = svg.matches(">#</text>").count();
    assert_eq!(lit, 9);
}

#[test]
fn text_measurements_from_the_host_are_used() {
    let plain = render("Start");
    let request = serde_json::json!({
        "code": "Start",
        "language": "de",
        "platform": "calliope",
        "widths": { "prop\u{1}Start": 400.0 },
    });
    let wide = parser::render_request(&request.to_string()).expect("render");
    assert_ne!(plain, wide, "supplied widths were ignored");
    assert!(wide.contains("H 420"), "block should grow to fit the label");
}

/// Pull the numeric tokens out of path data the way an SVG parser would:
/// commands, commas and whitespace all end a number, and nothing has to
/// separate a number from the command that follows it.
fn numbers(path: &str) -> Vec<String> {
    let chars: Vec<char> = path.chars().collect();
    let mut out = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
        if !(ch.is_ascii_digit() || ch == '-' || ch == '+' || ch == '.') {
            index += 1;
            continue;
        }
        let start = index;
        if chars[index] == '-' || chars[index] == '+' {
            index += 1;
        }
        let mut seen_dot = false;
        while index < chars.len() {
            match chars[index] {
                '.' if !seen_dot => {
                    seen_dot = true;
                    index += 1;
                }
                c if c.is_ascii_digit() => index += 1,
                _ => break,
            }
        }
        out.push(chars[start..index].iter().collect());
    }

    out
}

fn paths(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<path d=\"") {
        let from = start + "<path d=\"".len();
        let Some(end) = rest[from..].find('"') else { break };
        out.push(rest[from..from + end].to_string());
        rest = &rest[from + end..];
    }
    out
}
