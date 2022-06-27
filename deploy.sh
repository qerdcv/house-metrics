#!/bin/bash

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

readonly TARGET_HOST=pi@rpi_host
readonly TARGET_PATH=/home/pi/hello_world
readonly TARGET_ARCH=armv7-unknown-linux-gnueabihf
readonly SOURCE_PATH=./target/armv7-unknown-linux-gnueabihf/release/home_metrics

cargo build --release --target=${TARGET_ARCH}

rsync ${SOURCE_PATH} ${TARGET_HOST}:${TARGET_PATH}
ssh -t ${TARGET_HOST} ${TARGET_PATH}
