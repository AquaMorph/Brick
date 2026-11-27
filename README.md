# Brick

Brick is an open source stop motion program for Linux, Windows, and macOS. It supports Canon DSLRs through the EDSDK and webcams.

## Building

Requires Rust 1.95 or newer.

```sh
cargo run --package brick
```

Brick builds without the EDSDK; DSLR support is compiled out and only webcams are available.

## License

MIT. See [LICENSE](LICENSE).
