# Phage

An agar.io-style multiplayer cell game built with Rust.

Grow your cell by consuming food and smaller cells, split to chase prey, and climb the leaderboard.

## Features

- **Solo play** against 30 AI bots with varied behavior
- **Peer-to-peer multiplayer** via [iroh](https://iroh.computer) — host a game and share a ticket string for others to join
- **Host migration** — if the host disconnects, clients can promote to host
- Splitting, mass ejection, virus mechanics, and cell merging
- Leaderboard and score tracking

## Controls

| Key / Input | Action |
|---|---|
| Mouse | Steer cells toward cursor |
| Space | Split |
| W | Eject mass |

## Building

```
cargo build --release
```

## Running

```
cargo run --release
```

From the main menu, choose **Solo Play**, **Host Game**, or **Join Game** (paste a ticket string from a host).

## License

[MIT](LICENSE)
