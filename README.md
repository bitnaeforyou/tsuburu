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

## Reading offline

Any work, or any single page, can be kept on disk. The reader has
**Download** and **Download this page**; the Downloads tab lists what is here,
how far each work got, and lets a work be finished or deleted.

Pages are stored under the hash hitomi already gives them, so the same page in
two re-uploads is stored once, and deleting a work leaves the pages another
one still needs. Reading a downloaded work does not touch the network at all:
the image proxy answers from disk first, and if the gallery's metadata cannot
be fetched, the page list on disk is enough to open it.

## Artists

An artist's name in a gallery links to a page of everything they drew — newest
first, out of the metadata snapshot, with a count per language so you can stay
in the one you read. **Follow** keeps a name next to your favorites, and the
Favorites tab lists the ones you follow with how much each has.

Artist pages come from the snapshot, so they need `import-meta` to have run.

## Using artifact's artifacts

If you have the data artifact publishes, two commands load it and most of the
heavy work disappears:

```
tsuburu import-artifact /path/to/llm-search-index   # recognised Korean text, ~30 s
tsuburu import-meta   /path/to/artifact/data.db   # metadata for 1.46M galleries, ~4 min
```

Stop the server first; each database is opened by one process at a time.
`import-meta` reads the SQLite file through the local `sqlite3` command: macOS
ships it, Linux has it in a package, and on Windows `sqlite3.exe` from
sqlite.org needs to be on `PATH`.

With the metadata snapshot loaded, cards for galleries it covers never touch
the network, and the search bar gains a **local titles & artists** scope that
finds Korean titles, artists, series and characters offline. The snapshot
stops at the day it was taken, but galleries fetched from hitomi afterwards
are filed into it as they are seen, so it keeps up with what you browse.

With the dialogue corpus loaded, every Korean gallery up to mid-2026 is
searchable by a remembered line, with no downloading or recognition. A query
over the 108,000 galleries takes about half a second.

Imported text has gaps: artifact's OCR missed bubbles that Vision reads. The
Dialogue tab can queue any gallery to be read again, and "re-read imported
galleries" makes the background sweep go over the whole imported corpus. That
costs as much as indexing from nothing, so it is off by default.

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

### Sharing what has been read

The text is the expensive part and it is small, so it can travel. The
Dialogue tab exports what a machine has read as `.tsd` shard files and imports
other people's; files are checked against the hash in their name, and an
export can leave out the galleries you chose to read yourself.

### Similar scenes

The artifact also carries an embedding per passage, produced by a 4B model.
Searching it by a phrase would need that model; searching it by a passage
already in the index needs nothing at all. Each dialogue result has a
**Similar scenes** button that takes the vector stored for that passage and
finds the nearest others, which answers "what else reads like this".

The first query pulls the 2.6 GB index off disk and takes a few seconds; after
that it is about 0.2 s.

### Searching by meaning

Searching those same vectors by a *phrase* rather than by a passage needs the
model that made them, which is a multi-gigabyte download. tsuburu does not
bundle it and does not need it: the phrase mode stays hidden until you point
the Dialogue tab's pack setting at a server that speaks the OpenAI embeddings
API — llama.cpp, Ollama, LM Studio or anything else — running
`Qwen3-Embedding-4B` or a model whose vectors it shares.

A different model answers with perfectly plausible vectors and useless
results, so the setting has a **Check** button: it embeds a passage that is
already in the index and compares the answer with the vector stored for it.
Anything below 0.9 is reported as the wrong model rather than left looking
like a bad corpus.

## Platforms

Searching, reading, favorites, history, downloads and the imports work
everywhere. Text recognition is the exception: it calls the operating system's
own OCR, which so far means Vision on macOS. Elsewhere the Dialogue tab
reports itself as unavailable rather than pretending, though an imported
corpus is still searchable.

## Languages

Titles and dialogue are matched through a shared encoding that covers Hangul,
Latin and everything else, so a Japanese title or a Cyrillic line matches the
way a Korean one does — spacing is ignored inside Hangul and respected between
Latin words.

Recognition is told which script to expect, because asking for all of them at
once costs accuracy. The Dialogue tab's language setting picks both the corpus
to sweep and the recognition language, and the engine is rebuilt when it
changes.

## Building

Requires Rust 1.90+ and Node 20+.

```
cd web && npm install && npm run build && cd ..
cargo build --release
```

The frontend is embedded into the binary at compile time, so the release
artifact is a single file. Its output is not committed — only the empty
directory — so a build that skips the frontend step compiles and then says so
instead of serving a blank page.

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
crates/tsuburu-server   JSON API, image proxy, background indexer
crates/tsuburu-store    favorites and reading history (redb)
crates/tsuburu-korean   Korean search-term dictionary
crates/tsuburu-ocr      text recognition behind a trait (Vision on macOS)
crates/tsuburu-dialogue recognised text, matching, shards, artifact import
crates/tsuburu-meta     local metadata snapshot and its search
crates/tsuburu-embed    nearest-neighbour search over artifact's embeddings
crates/tsuburu-downloads pages kept on disk, shared by content hash
crates/tsuburu-text     multi-script matching shared by the searchable stores
crates/tsuburu          CLI and server entry point
web/                    Svelte 5 + Vite frontend
docs/superpowers/       design spec and implementation plan
```
