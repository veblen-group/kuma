default:
  @just --list

set dotenv-load
set fallback

default_env := 'local'
copy-env type=default_env:
    cp {{ type }}.env.example .env

cli:
    cargo run

dev-webapp:
    cd webapp && npm run dev

generate-signals:
    RUST_LOG=info cargo run -p kuma-cli -- \
    --token-a usdc --token-b weth \
    --chain-a ethereum --chain-b base \
    generate-signals

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

[no-exit-message]
_fmt-rust:
  just _lint-rust-fmt
  just _lint-rust-clippy

[no-exit-message]
_lint-rust-fmt:
  cargo +nightly fmt --all -- --check

[no-exit-message]
_lint-rust-clippy:
  cargo clippy --version
  cargo clippy --all-targets --all-features \
          -- --warn clippy::pedantic --warn clippy::arithmetic-side-effects \
          --warn clippy::allow_attributes --warn clippy::allow_attributes_without_reason \
          --deny warnings

[no-exit-message]
_fmt-toml:
  taplo format --check
