//! What the parser makes of a line of NEPO source.

use nepo_wasm::{matrix, parser};
use serde_json::Value;

fn parse(source: &str) -> Value {
    let request = serde_json::json!({
        "code": source,
        "language": "de",
        "platform": "calliope",
    });
    let json = parser::parse_request(&request.to_string()).expect("parse");
    serde_json::from_str(&json).expect("json")
}

/// The first script's blocks.
fn blocks(source: &str) -> Vec<Value> {
    parse(source)[0].as_array().expect("script").clone()
}

#[test]
fn text_maps_to_the_right_block() {
    let script = blocks("Start\n  Zeige Text \"Hallo\"\n  Schalte RGB LED an (#ff0000)");
    let ids: Vec<&str> = script.iter().map(|b| b["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        vec![
            "mbedControls_start",
            "mbedActions_display_text",
            "mbedActions_leds_on"
        ]
    );
}

#[test]
fn dropdowns_keep_their_value() {
    let script = blocks("Zeige Zeichen \"A\"");
    let fields = script[0]["rows"][0]["fields"].as_array().unwrap();
    let dropdown = fields
        .iter()
        .find(|field| field["kind"] == "dropdown")
        .expect("a dropdown");
    assert_eq!(dropdown["value"], "Zeichen");
}

#[test]
fn an_unspecified_dropdown_takes_the_first_option() {
    let script = blocks("Taste B gedrückt?");
    let fields = script[0]["rows"][0]["fields"].as_array().unwrap();
    let dropdown = fields.iter().find(|f| f["kind"] == "dropdown").unwrap();
    assert_eq!(dropdown["value"], "B");
}

#[test]
fn a_sensor_block_reports_a_boolean() {
    let script = blocks("Taste A gedrückt?");
    assert_eq!(script[0]["id"], "robSensors_key_isPressed");
    assert_eq!(script[0]["shape"], "value");
    assert_eq!(script[0]["check"], "Boolean");
}

#[test]
fn a_control_block_takes_nested_statements() {
    let script = blocks("Wiederhole unendlich oft\n  Zeige Text \"Hallo\"\n  Zeige Text \"Welt\"\nEnde");
    let rows = script[0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "title row plus a mouth");
    assert_eq!(rows[1]["kind"], "statement");
    let body = rows[1]["body"].as_array().unwrap();
    assert_eq!(body.len(), 2);
    assert_eq!(body[0]["id"], "mbedActions_display_text");
}

#[test]
fn nesting_survives_without_the_closing_keyword() {
    let with_end = blocks("Wiederhole unendlich oft\n  Zeige Text \"Hallo\"\nEnde");
    let without_end = blocks("Wiederhole unendlich oft\n  Zeige Text \"Hallo\"");
    assert_eq!(with_end, without_end);
}

#[test]
fn a_value_block_plugs_into_the_socket() {
    let script = blocks("Zeige Text \"Hallo\"");
    let row = &script[0]["rows"][0];
    assert_eq!(row["kind"], "value");
    assert_eq!(row["check"], "String");
    assert_eq!(row["value"]["id"], "text_text");
    let field = &row["value"]["rows"][0]["fields"][0];
    assert_eq!(field["kind"], "input");
    assert_eq!(field["value"], "Hallo");
}

#[test]
fn an_empty_socket_keeps_its_type() {
    let script = blocks("Zeige Text");
    let row = &script[0]["rows"][0];
    assert_eq!(row["check"], "String");
    assert!(row.get("value").is_none(), "socket should be empty");
}

#[test]
fn aliases_parse_but_print_open_robertas_wording() {
    let script = blocks("Programmstart\n  Wiederhole fortlaufend\n    Zeige Text \"Hallo\"\n  Ende");
    assert_eq!(script[0]["id"], "mbedControls_start");
    assert_eq!(script[0]["rows"][0]["fields"][0]["value"], "Start");

    assert_eq!(script[1]["id"], "robControls_loopForever");
    assert_eq!(
        script[1]["rows"][0]["fields"][0]["value"],
        "Wiederhole unendlich oft"
    );
    assert_eq!(script[1]["rows"][1]["fields"][0]["value"], "mache");
}

#[test]
fn the_matrix_field_survives_parsing() {
    let script = blocks("Zeige Bild (.#.#.|.#.#.|.....|#...#|.###.)");
    let image = &script[0]["rows"][0]["value"];
    assert_eq!(image["id"], "mbedImage_image");
    let rows = image["rows"][0]["fields"][0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0], ".#.#.");
}

#[test]
fn the_matrix_expands_to_open_robertas_own_rows() {
    let scripts = parser::parse(
        "Zeige Bild (.#.#.|.#.#.|.....|#...#|.###.)",
        "de",
        "calliope",
    )
    .expect("parse");
    let mut block = scripts[0][0].clone();
    matrix::expand(&mut block);

    let image = block.rows[0].value.as_ref().expect("image block");
    assert_eq!(image.rows.len(), matrix::SIZE + 1, "a ruler plus five rows");
    for row in &image.rows {
        assert_eq!(row.align, "right");
    }
    let cells = image.rows[1]
        .fields
        .iter()
        .filter(|field| matches!(field, nepo_wasm::model::SegmentSpec::PixelCell { .. }))
        .count();
    assert_eq!(cells, matrix::SIZE);
}

#[test]
fn a_start_block_opens_a_new_script() {
    let scripts = parse("Start\n  Zeige Text \"A\"\nStart\n  Zeige Text \"B\"");
    assert_eq!(scripts.as_array().unwrap().len(), 2);
}

#[test]
fn an_unknown_line_names_the_blocks_that_do_exist() {
    let error = parser::parse("Fliege zum Mond", "de", "calliope").unwrap_err();
    assert!(error.contains("line 1"), "{error}");
    assert!(error.contains("mbedControls_start"), "{error}");
}

#[test]
fn a_block_the_platform_does_not_offer_is_rejected() {
    // mbedActions_leds_on is Calliope-only; the micro:bit has no RGB LED.
    assert!(parser::parse("Schalte RGB LED an (#ff0000)", "de", "calliope").is_ok());
    let error = parser::parse("Schalte RGB LED an (#ff0000)", "de", "microbit").unwrap_err();
    assert!(error.contains("microbit"), "{error}");
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let plain = blocks("Start\n  Zeige Text \"Hallo\"");
    let noisy = blocks("// ein Kommentar\nStart\n\n  Zeige Text \"Hallo\" // und noch einer\n");
    assert_eq!(plain, noisy);
}

#[test]
fn english_renders_the_english_wording() {
    let request = serde_json::json!({
        "code": "Start\n  Show Text \"Hello\"",
        "language": "en",
        "platform": "calliope",
    });
    let json = parser::parse_request(&request.to_string()).expect("parse");
    let scripts: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(scripts[0][0]["rows"][0]["fields"][0]["value"], "Start");
    assert_eq!(scripts[0][1]["rows"][0]["fields"][0]["value"], "Show");
}
