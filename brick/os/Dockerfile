# Image box for Slot OS: partitioning and FAT tools, and the arm64 Debian libraries the RAM
# system is made of (glibc, libstdc++ for the GPU's shader compiler, ALSA, busybox, the card's
# file system checkers).
FROM debian:bookworm-slim

RUN dpkg --add-architecture arm64 \
 && apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates cpio dosfstools gdisk mtools python3 u-boot-tools xz-utils \
      binutils-aarch64-linux-gnu \
 && mkdir -p /opt/arm64-debs && cd /opt/arm64-debs \
 && apt-get download libc6:arm64 libgcc-s1:arm64 libstdc++6:arm64 libasound2:arm64 \
      libasound2-data alsa-utils:arm64 busybox-static:arm64 dosfstools:arm64 exfatprogs:arm64 \
 && ls /opt/arm64-debs > /opt/arm64-debs/VERSIONS \
 && rm -rf /var/lib/apt/lists/*
