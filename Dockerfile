# tsuburu in a container.
#
# The program is the same one the downloads carry; what the image adds is the
# two things its Linux recognition needs and a desktop would have to install by
# hand - tesseract, with the languages hitomi's pages are actually written in,
# and ffmpeg to decode an AVIF page for it.

FROM node:22-slim AS web
WORKDIR /web
# The lockfile alone first, so a change to the source does not re-resolve every
# dependency.
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1-slim-bookworm AS build
WORKDIR /src
# `ring` compiles C, so the toolchain needs a C compiler.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# rust-embed reads this at compile time; the frontend is already built.
COPY --from=web /web/dist/ web/dist/
# Where this was published from, so the program can offer the corpus and say
# when there is a newer one. Not written in the source: the image may be built
# somewhere other than where it is published.
ARG TSUBURU_RELEASES=""
RUN TSUBURU_RELEASES="$TSUBURU_RELEASES" cargo build --release --locked -p tsuburu

FROM debian:bookworm-slim
# kor/jpn/eng are what the works are in; `osd` is what tesseract wants for
# page orientation.
#
# ImageMagick is what turns an AVIF page into something tesseract reads. It is
# third in the list the recognition tries, and the one worth carrying: ffmpeg
# is first but brings four hundred megabytes of codecs for the one it needs,
# and avifdec is small but cannot resize, which is the step the recognition
# depends on - a page read at its full two thousand pixels comes back worse
# than the same page at eleven hundred.
RUN apt-get update && apt-get install -y --no-install-recommends \
        tesseract-ocr \
        tesseract-ocr-kor \
        tesseract-ocr-jpn \
        tesseract-ocr-eng \
        tesseract-ocr-osd \
        imagemagick \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /src/target/release/tsuburu /usr/local/bin/tsuburu

# Not root: the only thing this writes is its own data directory. The
# directory is made and handed over before it is declared a volume, so an
# empty named volume inherits the ownership rather than arriving root-owned.
RUN useradd --create-home --uid 10001 tsuburu \
    && mkdir -p /data && chown tsuburu:tsuburu /data
USER tsuburu
WORKDIR /home/tsuburu

# Where `directories` puts a Linux data directory, and so where the databases,
# the model and anything imported live. Mount this to keep them.
ENV XDG_DATA_HOME=/data
VOLUME ["/data"]

EXPOSE 8420
# 0.0.0.0 because a port published out of a container never reaches the
# loopback inside it, and no browser to open.
ENTRYPOINT ["tsuburu"]
CMD ["serve", "--host", "0.0.0.0", "--port", "8420", "--no-open"]
