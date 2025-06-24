clear

set -eux

export RUSTC_ICE=0
export RUST_BACKTRACE=1
export RUSTC_LOG_COLOR=always
# export RUSTC_LOG="rpl=trace"
export RPL_PATS="$1"

case ${3:-'--test'} in
    --test)
        cargo uitest -- "${2:-}"
        ;;
    --bless)
        cargo uibless -- "${2:-}"
        ;;
    *)
        echo "Invalid mode."
        exit -1
        ;;
esac
