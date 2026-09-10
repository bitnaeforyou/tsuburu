# tsuburu

A lightweight desktop client for searching and reading hitomi.la.

Download one file, run it, and a browser opens. No Docker, no runtime to
install, no initial index download.

```
$ tsuburu
tsuburu is running at http://127.0.0.1:8420/
press ctrl+c to stop
```

한국어 문서: [README.ko.md](README.ko.md)

## Why

[`project-artifact/artifact`](https://github.com/project-artifact/artifact) covers the
same ground with a Docker Compose stack of seven services that build their own
search index. That is a lot of machinery to ask of someone who wants to look
something up.

tsuburu reads the search index hitomi already serves, over HTTP range requests,
a few kilobytes at a time. There is nothing to index and nothing to sync. If
you also have the data artifact publishes, tsuburu will load it and everything
below gets faster and works offline — but none of it is required to search and
read.

## What it costs

Measured on an M4 Pro (macOS 26.5, 24 GB) over a residential connection, with
artifact's corpus, metadata snapshot and keyword graph all imported.

### Footprint

| | |
|---|---|
| Binary | **3.9 MB** (the frontend is inside it) |
| Frontend bundle | 136 kB of JavaScript, 14 kB of CSS — 47 kB gzipped |
| Start to first response | **0.4 s** |
| Memory, idle | **22 MB** |
| Memory, while searching the dialogue corpus | ~540 MB |
| Memory, with the embedding index open | 2.5 GB — a memory-mapped file, reclaimable |

Nothing is loaded until it is used. The 22 MB is the whole program with five
databases open; the numbers above it are what a particular search touches
while it runs.

### Disk

Only what you choose to import:

| | |
|---|---|
| Nothing imported | ~2 MB (favorites, history, settings) |
| Recognised dialogue, 108,340 works | 1.8 GB |
| Metadata snapshot, 1,464,390 works | 1.1 GB |
| Keyword graph, 106,155 works | 174 MB |
| Embedding index, 2,537,826 passages | 2.6 GB — read where it sits, never copied |
| Downloaded pages | what you keep; ~15 MB for a 23-page work |

### Speed

Searching hitomi, which is mostly network latency:

| | |
|---|---|
| A term not searched before | **1.6 s** |
| The same search again | **0.7 ms** |
| Two terms intersected | 3.8 s |
| Sorted by popularity | 0.7 s |
| Transferred, per search | **5.3 KB** in, 1 KB out — headers and TLS included |

Locally, once artifact's data is imported:

| | |
|---|---|
| Title, artist, series or character | **0.19 s** over 1.46M works |
| Tag suggestion while typing | 0.8 ms |
| One artist's works, with a count per language | 47 ms |
| What a work is about | 1 ms |
| Works about the same things | 2–18 ms |
| Every work a word runs through | 1 ms |

Dialogue, over 108,340 works and 1.5 GB of recognised text:

| | |
|---|---|
| A line you remember, exact | **1.2 s** |
| The same, allowing for misremembering | 0.7 s |
| Scenes like this one | 4.3 s first, **0.20 s** after |
| By meaning, with a pack running | 1.5–3.4 s — nearly all of it the model reading the phrase |

Reading and indexing:

| | |
|---|---|
| Text recognition | **7.4 pages/s** |
| Downloading pages | 1.7 MB/s, 2.5 pages/s |
| Importing artifact's dialogue corpus | 31 s (11 s when most of it is already there) |
| Importing the metadata snapshot | 4 min |
| Importing the keyword graph | 25 s |

Most of a hitomi search is latency, not computation: the index is a B-tree six
or seven levels deep and each level is a dependent round trip. tsuburu
prefetches the upper levels in the background at startup, which removes about
70 % of the cost. Result lists are read only as far as the current page needs.

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

In the browser one box asks every source at once — hitomi's tags, the local
titles, the recognised dialogue — and shows each in its own section as it
arrives. They are not merged into one list: a tag intersection and a fuzzy
line match cannot be ranked against each other, and pretending otherwise
would put an arbitrary order on the answer. Each section links to itself for
the full list.

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

If you have the data artifact publishes, three commands load it and most of the
heavy work disappears:

```
tsuburu import-artifact   /path/to/llm-search-index   # recognised Korean text, 31 s
tsuburu import-meta     /path/to/artifact/data.db   # 1.46M galleries, 4 min
tsuburu import-keywords /path/to/artifact/graph.csv # 106,000 works, 25 s
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
searchable by a remembered line, with no downloading or recognition.

The `llm-search-index` directory is not copied into tsuburu's own storage: the
embedding index is read where it sits, so keep the directory where it is.

## What a work is about

artifact also publishes `graph.csv`: the words it found running through each
work, scored by TF-IDF over the dialogue it recognised. A few tens of
megabytes, against the embedding index's 2.6 GB.

A work shows the words it is about, each a link to everything else those words
run through, and an **About the same things** row underneath the reader that
names what the two have in common. It is coarser than the embeddings — shared
words, not shared meaning — but it costs almost nothing to keep and answers
without a model.

Asked from the second volume of a series, it answers with the first: they
share four character names. Words held by more than 20,000 works are dropped
on import — they cost the most to store and say the least — and searching for
one says so, rather than implying the word appears nowhere.

## Dialogue search

hitomi's index only knows titles and tags. To find a work by a line you
remember, tsuburu has to read the pages itself: run the operating system's
text recognition over them and keep the text. Images are discarded as soon as
they are read.

**Works you open are read as you go.** Their pages are already coming down to
be displayed, so the image proxy hands what it is carrying to recognition
instead of fetching it again: nothing extra is downloaded, and a work you have
actually read becomes findable by a line from it. Pages that already have text
are skipped, so re-reading costs nothing. The Dialogue tab lists what this has
collected, with its size, and deletes any of it — or all of it — on request.

The **background sweep** is a different thing and is **off by default**: it
downloads galleries you have not opened. Turn it on from the Dialogue tab and
it indexes while the app is open, most popular Korean galleries first. The tab
always shows how much is covered; a miss means "not indexed yet" as often as
it means "not there".

Two shortcuts narrow it:

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

Asked from a page of the second volume of a series, it answers with the same
scene in the first volume, and the third volume after that.

Passages read on this machine are embedded too, when a pack is configured, and
scanned beside the imported index — so meaning search and Similar scenes reach
the works you have read, not just artifact's corpus.

### Searching by meaning

Searching those same vectors by a *phrase* rather than by a passage needs the
model that made them, which is a multi-gigabyte download. tsuburu does not
bundle it and does not need it: the phrase mode stays hidden until you point
the Dialogue tab's pack setting at a server that speaks the OpenAI embeddings
API — llama.cpp, Ollama, LM Studio or anything else — running
`Qwen3-Embedding-4B` or a model whose vectors it shares.

```
llama-server -m Qwen3-Embedding-4B-Q8_0.gguf --embedding --pooling last \
             -c 4096 -ub 4096 --port 8080
```

A different model answers with perfectly plausible vectors and useless
results, so the setting has a **Check** button: it embeds a passage that is
already in the index and compares the answer with the vector stored for it.
Anything below 0.9 is reported as the wrong model rather than left looking
like a bad corpus. Against artifact's own index the right model scores 1.000,
and a query takes a second or two — nearly all of it the model reading the
phrase, not the search.

## Interface language

The interface is in Korean, English or Japanese. It follows the browser on
first run and remembers the choice after that; the switch sits at the end of
the navigation bar and beside the reader's own header.

Only the wording around the content is translated. Titles, tags and dialogue
stay in whatever language hitomi holds them in. Numbers are grouped the way
the chosen language does, not the browser. An error keeps its English
diagnostic, but the sentence above it — the part that says what to do about
it — is translated, and the failures a reader can act on are named by the
server so that sentence can be specific.

Adding a language is one file under `web/src/lib/locales/`, typed against the
English one so a missing message is a build error rather than an English word
on a translated screen.

## Platforms

Searching, reading, favorites, history, downloads and the imports work
everywhere. Text recognition differs, because tsuburu ships no model and calls
whatever the machine can already reach:

| | Recognition | Decoding |
|---|---|---|
| macOS | Vision | ImageIO |
| Windows | Windows.Media.Ocr | WIC — AVIF needs the free AV1 Video Extension |
| Linux, other Unix | `tesseract` | `ffmpeg`, ImageMagick or `avifdec` |

The first two come with the operating system. Linux has neither, so tsuburu
calls two tools the distribution packages — the same arrangement `import-meta`
uses for `sqlite3`:

```
sudo apt install tesseract-ocr tesseract-ocr-kor ffmpeg
```

A decoder has to sit in front of tesseract because hitomi serves AVIF, which
leptonica cannot read. When either is absent the Dialogue tab says which one,
rather than reporting itself broken; an imported corpus stays searchable
either way.

Recognition is not equally good everywhere. Measured against Vision's reading
of a Korean page, tesseract found 7 of the 8 words Vision did — enough for
matching, which ignores spacing inside Hangul and scores fuzzily, but noisier
line by line.

## Matching across scripts

Titles and dialogue are matched through a shared encoding that covers Hangul,
Latin and everything else, so a Japanese title or a Cyrillic line matches the
way a Korean one does — spacing is ignored inside Hangul and respected between
Latin words.

Recognition is told which script to expect, because asking for all of them at
once costs accuracy. A work is read in its own language, and the Dialogue
tab's language setting picks the corpus the background sweep goes through.

## Building

Requires Rust 1.90+ and Node 20+.

```
cd web && npm install && npm run build && cd ..
cargo build --release
```

The frontend is embedded into the binary at compile time, so the release
artifact is a single file. `web/dist` is not committed; a build that skips the
frontend step still compiles and then says the assets are missing, rather than
serving a blank page.

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

Stopping it with Ctrl-C or a TERM closes the databases. Killing it outright
leaves them to be repaired on the next start, which on a full corpus costs
half a minute and two gigabytes.

## Layout

```
crates/tsuburu-hitomi   hitomi's formats: index, search, metadata, image URLs
crates/tsuburu-fetch    HTTP client, range requests, node cache
crates/tsuburu-server   JSON API, image proxy, background indexer
crates/tsuburu-store    favorites, reading history, followed artists (redb)
crates/tsuburu-korean   Korean search-term dictionary
crates/tsuburu-ocr      text recognition behind a trait (Vision, WinRT, tesseract)
crates/tsuburu-dialogue recognised text, matching, shards, artifact import
crates/tsuburu-meta     local metadata snapshot and its search
crates/tsuburu-embed    nearest-neighbour search over artifact's embeddings
crates/tsuburu-downloads pages kept on disk, shared by content hash
crates/tsuburu-keywords what each work is about, from artifact's graph.csv
crates/tsuburu-text     multi-script matching shared by the searchable stores
crates/tsuburu          CLI and server entry point
web/                    Svelte 5 + Vite frontend
docs/superpowers/       design specs and implementation plans
```
