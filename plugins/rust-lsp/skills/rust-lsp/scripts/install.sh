#!/usr/bin/env bash
set -e

rustup component add rust-analyzer
cargo install --git https://github.com/fedemagnani/rust-lsp-plugin
