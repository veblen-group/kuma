default:
  @just --list

set dotenv-load
set fallback

default_env := 'local'
copy-env type=default_env:
  cp {{ type }}.env.example .env

run:
  cargo run

default_lang := 'all'


# Format
#########
[doc("
Can format 'rust', 'toml', 'proto', or 'all'. Defaults to all.
")]
fmt lang=default_lang:
  @just _fmt-{{lang}}

_fmt-all:
  @just _fmt-rust
  @just _fmt-toml

@_lint-all:
  -just _lint-rust
  -just _lint-toml
  -just _lint-md

[no-exit-message]
_fmt-rust:
  cargo +nightly-2024-10-03 fmt --all

[no-exit-message]
_lint-rust:
  just _lint-rust-fmt
  just _lint-rust-clippy
  just _lint-rust-clippy-custom
  just _lint-rust-clippy-tools
  just _lint-rust-dylint

[no-exit-message]
_lint-rust-fmt:
  cargo +nightly-2024-10-03 fmt --all -- --check

[no-exit-message]
_lint-rust-clippy:
  cargo clippy --version
  cargo clippy --all-targets --all-features \
          -- --warn clippy::pedantic --warn clippy::arithmetic-side-effects \
          --warn clippy::allow_attributes --warn clippy::allow_attributes_without_reason \
          --deny warnings

[no-exit-message]
_lint-rust-clippy-custom:
  cargo +nightly-2024-10-03 clippy --all-targets --all-features \
          -p tracing_debug_field \
          -- --warn clippy::pedantic --deny warnings

[no-exit-message]
_lint-rust-clippy-tools:
  cargo clippy --manifest-path tools/protobuf-compiler/Cargo.toml \
          --all-targets --all-features \
          -- --warn clippy::pedantic --deny warnings

[no-exit-message]
_lint-rust-dylint:
  cargo dylint --all --workspace

[no-exit-message]
_fmt-toml:
  taplo format

[no-exit-message]
_lint-toml:
  taplo format --check

[no-exit-message]
_lint-md:
  markdownlint-cli2

