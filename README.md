# Rust Wasm Minesweeper

A Minesweeper implementation where game logic and most UI behavior run in Rust compiled to WebAssembly.

## Run locally

1. Install Trunk: `cargo install trunk`
2. Add wasm target: `rustup target add wasm32-unknown-unknown`
3. Start dev server: `trunk serve`

## Notes

- Left click: reveal cell
- Left click on a revealed number: chord (reveal surrounding cells when flags match)
- Right click: toggle flag
- Keyboard:
  - Arrow keys / WASD: move cursor
  - Enter / Space: reveal (or chord on revealed number)
  - F: toggle flag
  - C: chord
  - N: new game
- Preset and custom difficulties are supported
- First reveal is guaranteed to be safe
- Last selected difficulty and best time per difficulty are persisted in LocalStorage
