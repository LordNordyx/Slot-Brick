# Box for the Brick simulator: the real Brick build of slot, compiled for x86_64 and drawn
# offscreen by Mesa's software GLES2 (llvmpipe), so it runs anywhere Docker does, no GPU.
# Used by brick/sim.sh.
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates curl git make cmake gcc g++ libc6-dev pkg-config \
      libegl1 libgles2 libegl-mesa0 libgl1-mesa-dri \
 && rm -rf /var/lib/apt/lists/*

# Matches rust-toolchain.toml, so rustup never needs to download at build time.
ENV RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | CARGO_HOME=/usr/local/cargo sh -s -- -y --no-modify-path \
      --profile minimal --default-toolchain 1.96 -c rustfmt,clippy \
 && chmod -R a+rX /usr/local/rustup /usr/local/cargo

# No display server: Mesa draws into pbuffers on its surfaceless platform, in software.
ENV EGL_PLATFORM=surfaceless \
    LIBGL_ALWAYS_SOFTWARE=1
