# Rustl - a game engine written in rust

* WIP (very _very_ **very** early state)
* this is going to be a game engine soon ™️ 😬 (once it's grown up)


## current state
<img src="history/2023-11-12-2.png" width="720">
<br>
<sub>model from: https://github.com/KhronosGroup/glTF-Sample-Models/tree/master/2.0/DamagedHelmet</sub>
<br><br>

<img src="history/2023-10-05.png" width="720">
<br>
<sub>model from: https://sketchfab.com/3d-models/cathedral-faed84a829114e378be255414a7826ca</sub>
<br><br>

<img src="history/2023-11-12.png" width="720">
<br>
<sub>model from: https://sketchfab.com/3d-models/apocalyptic-city-a0c8f318ed6f4075a97c2e55b1272495</sub>
<br><br>

## requrements

```bash
# install

# cargo watch
cargo install cargo-watch

# wasm-pack
#https://rustwasm.github.io/wasm-pack/installer/
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```


```bash

# build locally (with watch)
cargo watch -s "cargo run --release" -w src/ -w resources/

# build for web  (with watch)
cargo watch -s "wasm-pack build --target web" -w src/ -w resources/

# run with backtrace (on windows)
set RUST_BACKTRACE=1 && cargo watch -s "cargo run --release" -w src/ -w resources/

# run with backtrace (mac/linux)
RUST_BACKTRACE=1 && cargo watch -s "cargo run --release" -w src/ -w resources/

```

Linux (Ubuntu) Requirements:
```bash
sudo apt-get install pkg-config cmake libglib2.0-dev build-essential librust-atk-dev libgtk-3-dev
```


## Hints
* prevent large scale values for objects -> this can cause flickering (because of float precision)