#!/bin/sh

cd "$(dirname "$0")" || exit
cd ..
cargo clean # cleaning before build, or a lot of cargo stuff can be put inside the "builder" image
docker build  --progress=plain --tag=webhome:latest -f ./docker/image .