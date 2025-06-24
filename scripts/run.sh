clear

set -eux

export RUSTC_ICE=0
export RUST_BACKTRACE=1
export RUSTC_LOG_COLOR=always
# export RUSTC_LOG="rpl=info"
export RPL_PATS="$1"

cargo run --bin rpl-driver -- --crate-type lib -Z no-codegen "$2" -W unconditional_panic -A unused
