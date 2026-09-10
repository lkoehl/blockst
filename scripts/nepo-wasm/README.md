# NEPO plugin for Blockst (prototype)

Renders Open Roberta (NEPO) blocks as SVG, for the Calliope mini and micro:bit
block sets. Separate from `scratchblocks-wasm`: the two dialects disagree about
notches, corner radii and what a block is even made of, so they share a Typst
front end and nothing below it.

Status: prototype. Six blocks, static rendering only. No execution, no
simulation, no mutators.

## Blocks

| Catalog id | German | Shape | Category |
|---|---|---|---|
| `mbedControls_start` | Start | start | activity `#E2001A` |
| `mbedActions_display_text` | Zeige Text … | statement | action `#F29400` |
| `mbedActions_leds_on` | Schalte RGB LED an … | statement | action `#F29400` |
| `robSensors_key_isPressed` | Taste … gedrückt? | value → Boolean | sensor `#8FA402` |
| `robControls_loopForever` | Wiederhole unendlich oft | statement + mouth | control `#EB6A0A` |
| `mbedActions_display_image` | Zeige Bild … | statement | action `#F29400` |

Plus the reporting blocks they plug into: `text_text`, `mbedColour_picker` and
`mbedImage_image` (the 5×5 LED matrix).

## Where the numbers come from

Everything visual is transcribed from the official sources rather than matched
by eye, because "the colours and shapes are exactly right" is what the
prototype had to demonstrate:

| What | Source |
|---|---|
| Category colours, data-type colours | `core/constants.js` |
| Notches, tabs, corner radius, spacing | `core/block_render_svg.js` |
| Field box, baseline, dropdown arrow | `core/field.js`, `core/field_dropdown.js` |
| Pixel cell | `core/field_pixelbox.js` |
| Label font | `core/css.js` (`.blocklyText`, 11pt sans-serif) |
| Block structure | `blocks/mbedControls.js`, `blocks/mbedActions.js`, `blocks/mbedImage.js`, `blocks/robControls.js`, `blocks/robSensorDefinitions.js` |
| German text | `msg/js/de.js` |

All from <https://github.com/OpenRoberta/blockly>.

## Layout

```
src/
  catalog.rs   blocks.toml + locales -> renderable blocks
  parser.rs    indentation-based source text -> blocks
  matrix.rs    the 5x5 LED grid, the one special case
  measure.rs   port of Blockly's renderCompute_
  render.rs    port of Blockly's renderDraw*
  svg.rs       the geometry constants, verbatim
  theme.rs     the palette, verbatim
data/
  blocks.toml          structure: shape, category, field kinds
  locales/de.toml      German text, dropdown options, aliases
  locales/en.toml      English text
```

Adding a block normally means adding one entry to `blocks.toml` and one line
per locale. Five of the six blocks above need nothing else.

## Build

```bash
rustup target add wasm32-unknown-unknown
cd scripts/nepo-wasm
cargo build --target wasm32-unknown-unknown --release --lib
cd ../..
cp scripts/nepo-wasm/target/wasm32-unknown-unknown/release/nepo_wasm.wasm \
   libs/nepo/plugins/nepo_wasm.wasm
```

## Test

```bash
cd scripts/nepo-wasm && cargo test
```

`tests/test_geometry.rs` checks the layout and the outline against
`examples/nepo/references/blockly-reference.mjs`, an independent JavaScript
transcription of the same Blockly source. Regenerate its output with
`examples/nepo/comparisons/build.sh` after changing any geometry.

## Render without Typst

```bash
cargo run --bin nepo-render -- '{"code":"Start","language":"de"}'
```
