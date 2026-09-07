# tsuburu

A lightweight desktop client for searching and reading hitomi.la.

Download one file, run it, and a browser opens. No Docker, no runtime to
install, no initial index download.

```
$ tsuburu
tsuburu is running at http://127.0.0.1:8420/
press ctrl+c to stop
```

## Why

[`project-artifact/artifact`](https://github.com/project-artifact/artifact) covers the
same ground with a Docker Compose stack of seven services that build their own
search index. That is a lot of machinery to ask of someone who wants to look
something up.

tsuburu reads the search index hitomi already serves, over HTTP range requests,
a few kilobytes at a time. There is nothing to index and nothing to sync.

| | artifact (next gen) | tsuburu |
|---|---|---|
| Deployment | docker compose, multiple services | one executable |
| First search | clone, build containers | download, run |
| Search index | built locally | read remotely, on demand |
| Binary / image | — | 2.6 MB |

## Measurements

Against the live index, from a residential connection:

- Cold search: **1.05 s** (3.6 s without index warm-up)
- Repeated search: **0.6 ms**
- First page of a 140,000-result query: **3.3 KB** transferred
- Release binary: **2.6 MB**

Most of a search is latency, not computation: the index is a B-tree six or
seven levels deep and each level is a dependent round trip. tsuburu prefetches
the upper levels in the background at startup, which removes about 70 % of the
cost. Result lists are read only as far as the current page needs.

## Usage

```
tsuburu                          # start the server and open a browser
tsuburu serve --port 9000        # pick a port (0 chooses a free one)
tsuburu serve --no-open          # do not launch a browser
tsuburu search "glasses -school" # search from the terminal
tsuburu gallery 4170351          # inspect one gallery
```

Search terms are combined with AND. Prefix a term with `-` to exclude it; quote
the whole query so your shell does not read it as a flag.

## Dialogue search

hitomi's index only knows titles and tags. To find a work by a line you
remember, tsuburu has to read the pages itself: download them, run the
operating system's text recognition, and keep the text. Images are discarded
as soon as they are read.

This is **off by default**. Turn it on from the Dialogue tab and it indexes in
the background while the app is open, most popular Korean galleries first, so
the works you are most likely to have read are covered soonest. The tab always
shows how much is covered; a miss means "not indexed yet" as often as it means
"not there".

Measured on an M4 Pro: recognition runs at about 11 pages/s, but the CDN
delivers roughly 1.2 MB/s, so a gallery of 32 pages takes about 7 s. The whole
Korean doujinshi corpus is around 583 GB and a week of nights. Two shortcuts
cut that down:

- **Import history.** Paste hitomi URLs or ids from your browser history or an
  old artifact database; they are indexed before anything else.
- **Hunt.** Narrow with tags, language and type, and queue only those.

The text is the expensive part and it is small, so it can travel. The
Dialogue tab exports what a machine has read as `.tsd` shard files and imports
other people's; files are checked against the hash in their name, and an
export can leave out the galleries you chose to read yourself.

Recognition uses Vision on macOS. Other platforms report the feature as
unavailable rather than pretending.

## Building

Requires Rust 1.90+ and Node 20+.

```
cd web && npm install && npm run build && cd ..
cargo build --release
```

The frontend is embedded into the binary at compile time, so the release
artifact is a single file. `web/dist` is not committed; if you build without it,
the server will say so instead of serving a blank page.

## Testing

```
cargo test --workspace          # offline; uses recorded response fixtures
cargo test -p tsuburu-fetch -- --ignored   # hits the live site
```

The offline suite parses real captured bytes, so it verifies the actual format
rather than an idealised one. The live suite is what tells you the site has
changed.

## When it breaks

It will. hitomi's index format is undocumented and its domain has already moved
once (`ltn.hitomi.la` no longer resolves). tsuburu checks what it reads and, when
the format is not what it expects, says the site has changed and that an update
is needed — rather than showing wrong results or an empty screen.

All knowledge of hitomi's formats lives in the `tsuburu-hitomi` crate. That is
the only place a fix has to go.

## macOS

Releases are unsigned, so macOS shows an "unidentified developer" warning the
first time. Right-click the binary and choose Open to get past it.

## Notes

tsuburu runs entirely on your own machine. It does not host or redistribute
content; it relays only the requests you make, and limits how many it makes at
once. The site it searches contains adult material and the application is
intended for adults.

## Layout

```
crates/tsuburu-hitomi   hitomi's formats: index, search, metadata, image URLs
crates/tsuburu-fetch    HTTP client, range requests, node cache
crates/tsuburu-server   JSON API and image proxy
crates/tsuburu          CLI and server entry point
web/                    Svelte 5 + Vite frontend
docs/superpowers/       design spec and implementation plan
```
