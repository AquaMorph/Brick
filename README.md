# Brick

Brick is an open source stop motion program for Linux, Windows, and macOS. It supports Canon DSLRs through the EDSDK and webcams.

## Building

Requires Rust 1.95 or newer.

```sh
cargo run --package brick -- <project folder>
```

The project folder is opened, or a new project is created there if it does not hold one yet.

Brick builds without the EDSDK; DSLR support is compiled out and only webcams are available. To build with Canon DSLR support, install the EDSDK as described in [docs/edsdk.md](docs/edsdk.md).

## License

MIT. See [LICENSE](LICENSE).
