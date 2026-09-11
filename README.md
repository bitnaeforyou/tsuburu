<div align="center">

# tsuburu

**A lightweight desktop client for searching and reading hitomi.la.**

One 3.9 MB file. No Docker, no runtime to install, no index to download.

[한국어](README.ko.md) · [Releases](../../releases/latest) · MIT

</div>

```console
$ tsuburu
tsuburu is running at http://127.0.0.1:8420/
press ctrl+c to stop
```

- **Search** hitomi's own index, in Korean or English — nothing is built or synced
- **Read** at full resolution, and **keep** works or single pages for offline
- **Find a work by a line you remember** — pages you open are recognised as they arrive
- **Follow artists**, keep favorites, resume where you stopped
- With a published corpus: **1.46M titles offline**, **what a work is about**, **scenes like this one**, and **search by meaning**
- Interface in **한국어 / English / 日本語**

---

## Install

Download the archive for your machine from the [latest release](../../releases/latest),
unpack it, run the one file inside.

<details open>
<summary><b>macOS</b></summary>

The release is unsigned, so Gatekeeper stops it the first time.

```console
tar xzf tsuburu-aarch64-apple-darwin.tar.gz   # x86_64 on an Intel Mac
xattr -d com.apple.quarantine tsuburu         # or right-click → Open, once
./tsuburu
```
</details>

<details>
<summary><b>Windows</b></summary>

Unpack the archive (Explorer handles `.tar.gz`, or `tar -xf` in a terminal) and
run `tsuburu.exe`. SmartScreen warns about an unknown publisher —
*More info → Run anyway*.
</details>

<details>
<summary><b>Linux</b></summary>

```console
tar xzf tsuburu-x86_64-unknown-linux-gnu.tar.gz
chmod +x tsuburu
./tsuburu
```

For dialogue search, install the two tools it calls:

```console
sudo apt install tesseract-ocr tesseract-ocr-kor ffmpeg
```
</details>

A browser opens at `http://127.0.0.1:8420/`. Confirm you are an adult once and
the search screen appears.

### Out of the box

Nothing beyond the file you just ran:

| | What you get |
| :-- | :-- |
| **Search** | hitomi's tags, Korean or English. `-term` excludes. |
| **Read** | full resolution, arrow keys, resumes where you stopped |
| **Keep** | a work or a single page, then read it with hitomi unreachable |
| **Remember** | favorites and history, on this machine only |
| **Dialogue search** | over every work you open — see [below](#dialogue-search) |

### What you bring

Three files make it much more capable. They are large, they are not shipped
with tsuburu, and everything above works without them. Corpora of this kind
are published for this site by others; tsuburu reads them.

| To get | You need | Size | Import |
| :-- | :-- | --: | :-- |
| Dialogue search without reading the works yourself | recognised text (`llm-search-index`) | 3.9 GB | `tsuburu import-artifact <dir>` |
| Titles, artists and characters offline | metadata (`data.db`) | 2.2 GB | `tsuburu import-meta <file>` |
| What a work is about, and works about the same things | keywords (`graph.csv`) | 190 MB | `tsuburu import-keywords <file>` |

Stop the server first — one process opens a database at a time. The corpus
directory is read where it sits and never copied, so keep it wherever you
like, including an external disk.

**Searching by meaning** needs one more thing: the embedding model that built
the index. It is a separate multi-gigabyte download, runs as your own local
server, and the mode stays hidden until you set it up.

### Where things are kept

| Platform | Path |
| :-- | :-- |
| macOS | `~/Library/Application Support/la.tsuburu.tsuburu/` |
| Linux | `~/.local/share/tsuburu/` |
| Windows | `%APPDATA%\tsuburu\tsuburu\data\` |

Deleting it resets everything and loses only what you imported and read.

## Searching

One box asks every source at once — hitomi's tags, your local titles, the
recognised dialogue — and shows each in its own section as it arrives.

They are not merged into one list. A tag intersection and a fuzzy line match
cannot be ranked against each other, and one list would put an arbitrary order
on the answer. Each section links to itself for the full results.

Terms combine with AND; `-term` excludes. From the terminal:

```console
tsuburu search "glasses -school"   # search
tsuburu gallery 4170351            # inspect one work
tsuburu serve --port 9000          # pick a port (0 takes a free one)
tsuburu serve --no-open            # do not launch a browser
```

## Reading offline

The reader has **Download** and **Download this page**; the Downloads tab shows
what is here and how far each work got.

Pages are stored under the hash hitomi already gives them, so the same page in
two re-uploads is stored once, and deleting a work leaves the pages another one
still needs. Reading a downloaded work never touches the network: the image
proxy answers from disk, and if the metadata cannot be fetched, the page list
on disk is enough to open it.

## Dialogue search

hitomi's index knows titles and tags. To find a work by a line you remember,
the pages have to be read — recognised by the operating system, and the text
kept. Images are discarded as soon as they are read.

### As you read

**Works you open are read as you go.** Their pages are already coming down to
be displayed, so the image proxy hands what it is carrying to recognition
instead of fetching it again. Nothing extra is downloaded, and a work you have
actually read becomes findable by a line from it.

Pages that already have text are skipped, so re-reading costs nothing. The
Dialogue tab lists what this collected, with its size, and deletes any of it —
or all of it — on request.

### The background sweep

A different thing, and **off by default**: it downloads galleries you have not
opened. Turn it on from the Dialogue tab and it works while the app is open,
most popular Korean galleries first. Two shortcuts narrow it:

- **Import history** — paste hitomi URLs or ids from your browser history; they
  are indexed before anything else.
- **Hunt** — narrow by tags, language and type, and queue only those.

### Similar scenes

An imported corpus carries an embedding per passage, produced by a 4B model.
Searching it by a phrase needs that model; searching it by a passage already in
the index needs nothing at all. Each result has a **Similar scenes** button.

Asked from a page of the second volume of a series, it answers with the same
scene in the first volume, and the third volume after that. First query 4.3 s
while the index comes off disk, 0.20 s after.

Passages read on this machine are embedded too when a pack is configured, and
scanned beside the imported index, so this reaches the works you have read.

### Searching by meaning

Point the Dialogue tab's pack setting at a server speaking the OpenAI
embeddings API — llama.cpp, Ollama, LM Studio — running `Qwen3-Embedding-4B`
or a model whose vectors it shares.

```console
llama-server -m Qwen3-Embedding-4B-Q8_0.gguf --embedding --pooling last \
             -c 4096 -ub 4096 --port 8080
```

A different model answers with plausible vectors and useless results, so the
setting has a **Check** button: it embeds a passage already in the index and
compares. Below 0.9 it reports the wrong model rather than leaving it to look
like a bad corpus. The right model scores 1.000.

### Sharing what has been read

The text is the expensive part and it is small, so it can travel. The Dialogue
tab exports what this machine recognised as `.tsd` shards and imports other
people's; files are checked against the hash in their name.

Text that came from an imported corpus is never included — a shard is your own
reading, and a corpus is the importer's to fetch.

## What a work is about

A keyword graph gives the words running through each work, scored by TF-IDF
over its dialogue. A few tens of megabytes against the embedding index's 2.6 GB.

A work shows the words it is about, each a link to everything else those words
run through, and an **About the same things** row under the reader naming what
the two have in common. Coarser than the embeddings — shared words, not shared
meaning — but it costs almost nothing and answers without a model.

Asked from the second volume of a series it answers with the first: they share
four character names. Words held by more than 20,000 works are dropped on
import, and searching for one says so rather than implying it appears nowhere.

## Artists

An artist's name in a work links to everything they drew — newest first, with a
count per language so you can stay in the one you read. **Follow** keeps a name
beside your favorites. Needs the metadata snapshot.

## Interface language

한국어, English or 日本語. It follows the browser on first run and remembers the
choice; the switch sits at the end of the navigation bar and beside the
reader's header.

Only the wording is translated — titles, tags and dialogue stay in whatever
language hitomi holds them in. Numbers are grouped the way the chosen language
does. An error keeps its English diagnostic, but the sentence above it, the
part that says what to do, is translated.

Adding a language is one file under `web/src/lib/locales/`, typed against the
English one so a missing message is a build error.

## What it costs

Measured on an M4 Pro (macOS 26.5, 24 GB) over a residential connection, with
all three files imported.

### Footprint

| What | How much |
| :-- | --: |
| Binary, frontend included | **3.9 MB** |
| Start to first response | **0.4 s** |
| Memory, idle | **22 MB** |
| Memory, searching the dialogue corpus | ~540 MB |
| Memory, with the embedding index open | 2.5 GB (mapped file, reclaimable) |

### Disk

| Imported | Size |
| :-- | --: |
| Nothing imported | ~2 MB |
| Recognised dialogue, 108,340 works | 1.8 GB |
| Metadata, 1,464,390 works | 1.1 GB |
| Keywords, 106,155 works | 174 MB |
| Embedding index, 2,537,826 passages | 2.6 GB, read in place |

### Speed

| Searching hitomi | |
| :-- | --: |
| A term not searched before | **1.6 s** |
| The same search again | **0.7 ms** |
| Two terms intersected | 3.8 s |
| Sorted by popularity | 0.7 s |
| Transferred per search | **5.3 KB** in, 1 KB out |

| Locally | |
| :-- | --: |
| Title, artist, series or character over 1.46M works | **0.19 s** |
| Tag suggestion while typing | 0.8 ms |
| One artist's works with a count per language | 47 ms |
| What a work is about | 1 ms |
| Works about the same things | 2–18 ms |
| Every work a word runs through | 1 ms |

| Dialogue, over 108,340 works | |
| :-- | --: |
| A line you remember, exact | **1.2 s** |
| The same, allowing for misremembering | 0.7 s |
| Scenes like this one | 4.3 s first, **0.20 s** after |
| By meaning, with a pack running | 1.5–3.4 s, nearly all of it the model |

| Reading and indexing | |
| :-- | --: |
| Text recognition | **7.4 pages/s** |
| Downloading pages | 1.7 MB/s, 2.5 pages/s |
| Importing a dialogue corpus | 31 s |
| Importing the metadata | 4 min |
| Importing the keyword graph | 25 s |

Most of a hitomi search is latency, not computation: the index is a B-tree six
or seven levels deep and each level is a dependent round trip. tsuburu
prefetches the upper levels at startup, which removes about 70 % of the cost.

## Platforms

Searching, reading, favorites, history, downloads and the imports work
everywhere. Text recognition differs, because tsuburu ships no model and calls
whatever the machine can already reach:

| Platform | Recognition | Decoding |
| :-- | :-- | :-- |
| macOS | Vision | ImageIO |
| Windows | Windows.Media.Ocr | WIC — AVIF needs the free AV1 Video Extension |
| Linux, other Unix | `tesseract` | `ffmpeg`, ImageMagick or `avifdec` |

The first two come with the operating system. A decoder has to sit in front of
tesseract because hitomi serves AVIF, which leptonica cannot read. When either
is missing the Dialogue tab says which one.

Recognition is not equally good everywhere: against Vision's reading of a
Korean page, tesseract found 7 of the 8 words — enough for matching, which
ignores spacing inside Hangul and scores fuzzily, but noisier line by line.

### Matching across scripts

Titles and dialogue are matched through one encoding covering Hangul, Latin and
everything else, so a Japanese title or a Cyrillic line matches the way a
Korean one does. Spacing is ignored inside Hangul and respected between Latin
words. A work is read in its own language.

## Building

Rust 1.90+, Node 20+.

```console
cd web && npm install && npm run build && cd ..
cargo build --release
```

The frontend is embedded at compile time, so the release is a single file.
`web/dist` is not committed; a build that skips the frontend still compiles and
then says the assets are missing.

```console
cargo test --workspace                     # offline, against recorded bytes
cargo test -p tsuburu-fetch -- --ignored   # hits the live site
```

The offline suite parses real captured bytes, so it verifies the actual format
rather than an idealised one. The live suite is what tells you the site changed.

## When it breaks

It will. hitomi's index format is undocumented and its domain has already moved
once. tsuburu checks what it reads and, when the format is not what it expects,
says the site has changed and an update is needed — rather than showing wrong
results or an empty screen.

All knowledge of hitomi's formats lives in `tsuburu-hitomi`. That is the only
place a fix has to go.

## Notes

tsuburu runs entirely on your own machine. It does not host or redistribute
content; it relays only the requests you make, and limits how many at once. The
site it searches contains adult material and the application is for adults.

Stopping with Ctrl-C or a TERM closes the databases. Killing it outright leaves
them to be repaired on the next start — on a full corpus, half a minute.

## Layout

```
crates/tsuburu-hitomi     hitomi's formats: index, search, metadata, image URLs
crates/tsuburu-fetch      HTTP client, range requests, node cache
crates/tsuburu-server     JSON API, image proxy, background indexer
crates/tsuburu-store      favorites, history, followed artists
crates/tsuburu-korean     Korean search-term dictionary
crates/tsuburu-ocr        recognition behind a trait: Vision, WinRT, tesseract
crates/tsuburu-dialogue   recognised text, matching, shards, corpus import
crates/tsuburu-meta       local metadata snapshot and its search
crates/tsuburu-embed      nearest-neighbour search over an embedding index
crates/tsuburu-downloads  pages kept on disk, shared by content hash
crates/tsuburu-keywords   what each work is about
crates/tsuburu-text       multi-script matching shared by the stores
crates/tsuburu            CLI and server entry point
web/                      Svelte 5 + Vite frontend
docs/superpowers/         design specs and implementation plans
```
