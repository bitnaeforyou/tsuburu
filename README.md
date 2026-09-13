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
- With a published corpus: **1.46M titles offline**, **what a work is about**, and **saying a line in your own words**
- Interface in **한국어 / English / 日本語**

---

## Install

Nothing is installed. Download, open, and run — the archive holds the program
and a `START-HERE.txt` that says the same thing in two languages.

| Your computer | Download from [the latest release](../../releases/latest) | Then |
| :-- | :-- | :-- |
| Windows | `…windows-msvc.zip` | double-click **tsuburu.exe** |
| Mac, M1 and later | `…aarch64-apple-darwin.zip` | double-click **Start tsuburu.command** |
| Mac, Intel | `…x86_64-apple-darwin.zip` | double-click **Start tsuburu.command** |
| Linux | `…linux-gnu.tar.gz` | run **start.sh** |

These builds are not signed, so each system warns once, the first time only:

- **Windows** — a blue *Windows protected your PC* box. Click *More info*,
  then *Run anyway*.
- **macOS** — *cannot be opened because the developer cannot be verified*.
  Right-click **Start tsuburu.command** and choose *Open*. That launcher also
  clears the quarantine flag the browser adds, so it is the only step.
- **Linux** — nothing warns. If your file manager will not run `start.sh`,
  open a terminal in the folder and type `./start.sh`.

A browser opens at `http://127.0.0.1:8420/`. Confirm you are an adult once and
the search screen appears.

### Out of the box

Nothing beyond the file you just ran:

| | What you get |
| :-- | :-- |
| **Search** | hitomi's tags, Korean or English. `-term` excludes. |
| **Read** | full resolution, three layouts, either direction, resumes where you stopped |
| **Keep** | a work or a single page, then read it with hitomi unreachable |
| **Remember** | favorites and history, on this machine only, in a file you can move |
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

Terms combine with AND; `-term` excludes. A number, or a hitomi address with
one in it, is not searched for: hitomi's index maps words to works, so a
gallery number is an address rather than a term, and pasting one opens that
work.

That matters more than it sounds. hitomi stops listing a work long before it
stops serving it — of a sample of sixty works its index no longer names, **52
still handed over every page**. They are missing from hitomi's own search and
from browsing, but the snapshot still knows them, so the local source finds
them and a number opens them. Where a result is one of those, it says **not
listed on hitomi**: the pages are there now, which is the only time keeping a
copy is still possible.

From the terminal:

```console
tsuburu search "glasses -school"   # search
tsuburu gallery 4170351            # inspect one work
tsuburu serve --port 9000          # pick a port (0 takes a free one)
tsuburu serve --no-open            # do not launch a browser
```

## The reader

A work opens on its own page first — cover, title, artists, series, how many
pages, what it is tagged, every page as a thumbnail, and what else is about the
same things — and reading is a step you take from there: **Read**, or **Continue from page 12** if you have been here
before. Leaving the reader comes back to it rather than out of the work. A line
found by dialogue search is the exception: it links to the page it is on.

Three ways to lay a work out. The choice is remembered.

| Layout | |
| :-- | :-- |
| **Scroll** | every page in one column, the way a webtoon is read |
| **Page** | one page, one turn |
| **Spread** | two pages side by side, the cover on its own |

Manga is drawn right to left, so **R → L** flips the arrow keys, the side of the
screen that turns forward, and the order pages sit in a spread. Scrolling and
turning each remember their own fit: filling the width suits a column, fitting
the whole page suits a turn.

| | |
| :-- | :-- |
| **←** **→** | turn, in reading order |
| **Space**, **PageDown** | onward |
| **F** | full screen |
| **Esc** | back |
| Tap the left or right third | turn |
| Tap the middle | clear everything but the pages away, and back again |
| Swipe | turn |
| Pinch, double-click, **Ctrl** + wheel | zoom, then drag to move around; turning the page lets it go |
| Drag a page taller than the frame | move it under the finger |

The slider goes straight to a page, and **▦** — or the page count in the corner,
which is all that is left once everything else has been cleared away — opens
every page at once as thumbnails. They are hitomi's own small copies, about a
thirtieth of the page, and only the ones on screen are fetched: opening the wall
of a 1,537-page work moves around 140 KB.

The next three pages are decoded before you reach them, so a turn is a frame
rather than a wait, and turning a page at a time holds one page in the window
instead of all of them — a 1,537-page work is one image element rather than
1,537.

### On a phone

The five destinations sit at the bottom, where a thumb reaches them; the filters
keep one row and slide sideways; the controls are sized to be pressed rather
than clicked. The reader takes every touch itself — one finger turns, swipes or
moves a tall page, two pinch — so nothing it does turns into a scroll of the
page behind it. What the help line says changes with the device: a phone is not
told about **Esc**.

## Keeping it up to date

A published copy knows which release it came from, because the workflow that
built it told it — the source names nobody. It asks once when it starts whether
there is a newer one, and if there is, the Settings tab says so. One press
fetches the build for this computer and puts it where the running one is,
moving the old one aside first rather than writing over it; on the platform
that will not delete a running program, the leftover is swept up next time.
Then it says to start tsuburu again.

Nothing is fetched or replaced without being pressed. A copy built from a
checkout has nowhere to ask and offers nothing.

## Taking it with you

Favorites, the artists you follow and how far you have read are the only things
tsuburu keeps about you. **Save a copy** on the Favorites or History screen puts
all three in one JSON file; **Load a copy** puts them back through the same calls
the interface uses, adding to what is there rather than replacing it. A record
the program will not accept is skipped and counted; the rest still go in.

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
Settings tab lists what this collected, with its size, and deletes any of it —
or all of it — on request.

### The background sweep

A different thing, and **off by default**: it downloads galleries you have not
opened. Turn it on from the Settings tab and it works while the app is open,
most popular Korean galleries first. Two shortcuts narrow it:

- **Import history** — paste hitomi URLs or ids from your browser history; they
  are indexed before anything else.
- **Hunt** — narrow by tags, language and type, and queue only those.

Passages read on this machine are embedded too when a pack is configured, and
scanned beside the imported index, so this reaches the works you have read.

### Say it in your own words

One switch, on the Settings tab. Turning it on fetches 2.3 GB once — the
weights from the people who trained them, and a model server from the people
who wrote it — and runs it here. Nothing is fetched before it is switched on,
nothing leaves this computer after, and switching it off stops the server and
keeps the file, so switching it on again is only the time to read the weights.
It comes back on by itself after a restart, because it was already asked for.

Then **in my own words** appears beside the dialogue in the search box. It
searches whatever has been read: pages are embedded as they are recognised,
against the model the program is running, so a corpus somebody else built is
an addition rather than a requirement. Write
the line the way you half remember it — wrong words, half the sentence — and it
finds the real one. What it will not do is find a scene from a description of
it: the index holds short lines of dialogue, so a description only lands near
lines that happen to use the same words.

| | |
| :-- | --: |
| Weights | Qwen3-Embedding-4B, Q4_K_M, from Qwen, Apache-2.0 |
| Runs on | llama.cpp's own published build for this computer |
| Fetched | 2.33 GB once, resumed if it is interrupted |
| Kept at | the same place as everything else — deleting it frees the space |

If you would rather run your own — a different quantisation, a GPU box on the
network — there is still an address to point at under *things you almost
certainly do not need*, and a **Check** button beside it: a different model
answers with plausible vectors and useless results, so it embeds a passage
already in the index and compares. Below 0.9 it says so rather than leaving it
to look like a bad corpus. The right model scores 1.000.

### Sharing what has been read

The text is the expensive part and it is small, so it can travel. The Settings
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
| By meaning, with a pack running | 1.5–3.4 s, nearly all of it the model |

| The reader | |
| :-- | --: |
| Turning to a page already fetched | **12–66 ms**, key to painted |
| Image elements for a 1,537-page work | **1** turning pages, 1,537 scrolling |

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
is missing the Settings tab says which one.

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
