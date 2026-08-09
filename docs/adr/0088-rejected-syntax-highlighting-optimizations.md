# ADR 0088: Rejected syntax-highlighting optimizations

Several changes looked promising but either made highlighting slower, did not
produce a meaningful improvement, or changed visible syntax colors. They are
recorded together because none changed the production implementation.

## Keep parallel old and new highlighting

Highlighting both sides sequentially made every top-window benchmark slower:
Rust by about 40%, TypeScript by 55%, JSON by 67%, and Markdown by 59%. Keep the
two scoped threads.

## Keep simple diff-side collection

Filtering temporary side lists to the viewport and its look-behind did not
improve Rust and made the TypeScript top window about 20% slower. Syntect
parsing dominates this work, so keep the simple full-side collection.

## Do not build parser checkpoints on cold jumps

An accurate checkpoint must first parse from the beginning of the file, which
would put full-file work back on the cold opening path. Warm checkpoints would
measure a different path that Diffo's prepared-window cache already covers.

## Keep owned strings in syntax spans

Changing `StyledSpan` text from `String` to `Box<str>` had no statistically
significant effect on the 9,999-line benchmark. Both representations still
allocate and copy every token, so keep the simpler public type.

## Keep syntect's Oniguruma backend

The pure-Rust `fancy-regex` backend passed correctness tests but slowed the Rust
deep-window benchmark from about 11.6 ms to 39.9 ms. Keep Oniguruma.

## Keep the 256-line look-behind

Reducing look-behind to 64 lines made Rust and TypeScript deep windows roughly
62% and 55% faster, but broke highlighting when a multiline comment started 200
lines before the viewport. Keep 256 lines and the regression test.

## Do not maintain language-specific lexers

Diffo's colors depend on semantic roles such as function names and JSON property
names. Small lexical scanners cannot reproduce that output; doing so would
require maintaining language-specific parsers and multiline state.

## Keep syntect's line convenience API

Using syntect's lower-level parser and highlight iterators avoided temporary
hidden spans but produced no significant improvement for Rust or TypeScript deep
windows. Keep `HighlightLines::highlight_line` and its simpler state handling.

## Do not cache the last syntax lookup

A one-entry cache improved TypeScript's top window by 5.5%, but Rust, JSON, and
Markdown did not change significantly; the geometric improvement was about 1.4%.
That does not justify adding a mutex and mutable cache state. Preserve
first-line detection as a fallback for extensionless or unknown files;
recognized extensions do not use it.
