#!/bin/sh

set -eu

cd "$(dirname "$0")/.."
docker build --progress=plain --tag=webhome:latest -f ./docker/image .
