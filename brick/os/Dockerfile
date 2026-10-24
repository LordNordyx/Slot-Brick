# Image box for Slot OS: partitioning and FAT tools, and the arm64 Debian libraries the RAM
# system is made of (glibc, libstdc++ for the GPU's shader compiler, ALSA, busybox, the card's
# file system tools and sgdisk, which grows the card's partition on first boot,
# wpa_supplicant for Wi-Fi).
FROM debian:bookworm-slim

RUN dpkg --add-architecture arm64 \
 && apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates cpio dosfstools gdisk mtools python3 python3-pil u-boot-tools xz-utils \
      binutils-aarch64-linux-gnu \
 && mkdir -p /opt/arm64-debs && cd /opt/arm64-debs \
 && apt-get download libc6:arm64 libgcc-s1:arm64 libstdc++6:arm64 libasound2:arm64 \
      libasound2-data alsa-utils:arm64 busybox-static:arm64 dosfstools:arm64 exfatprogs:arm64 \
      gdisk:arm64 libpopt0:arm64 libuuid1:arm64 \
      wpasupplicant:arm64 libdbus-1-3:arm64 libnl-3-200:arm64 libnl-genl-3-200:arm64 \
      libnl-route-3-200:arm64 libpcsclite1:arm64 libssl3:arm64 libsystemd0:arm64 libcap2:arm64 \
      libgcrypt20:arm64 libgpg-error0:arm64 liblz4-1:arm64 liblzma5:arm64 libzstd1:arm64 \
      libreadline8:arm64 libtinfo6:arm64 gcc-12-base:arm64 \
 && ls /opt/arm64-debs > /opt/arm64-debs/VERSIONS \
 && rm -rf /var/lib/apt/lists/*
