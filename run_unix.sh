#!/bin/sh

cargo b --release && sudo setcap CAP_SYS_PTRACE=+eip target/release/yclass && RUST_BACKTRACE=1 ./target/release/yclass
