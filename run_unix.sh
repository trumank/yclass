#!/bin/sh

cargo b && sudo setcap CAP_SYS_PTRACE=+eip target/debug/yclass && RUST_BACKTRACE=1 ./target/debug/yclass
