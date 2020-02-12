# Tokio Diesel
> Integrate Diesel into Tokio cleanly and efficiently.

## Usage

See [the example](./examples/simple.rs) for detailed usage information.

### Feature Flags

- __tokio-rt-threaded__: Available when using the `rt-threaded` feature of tokio.
    This feature will remove the `'static` lifetime restriction on the closures sent to the
  `AsyncConnection` trait by using `tokio::task::block_in_place` instead of
  `tokio::task::spawn_blocking`. It will also remove the `'static` restriction on the
  `AsyncRunQueryDsl` and `AsyncSaveChangesDsl` implementations.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
