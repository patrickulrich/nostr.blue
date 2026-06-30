# NKBIP-08

# NKBIP-08: Book Wikilinks

## Abstract

This NKBIP defines a `book::` wikilink macro (based upon [NIP-54](/wiki/nip-54)) that references kind `30040` chapters and kind `30041` sections within a kind `30040` publication (assumed to usually be a book). Other hierarchically-structured events might also find this specification useful, for internal referencing.

This technique allows a book's content to be embedded and searched effectively, and simplifies the ability to view a book reference within a larger context within the book. For instance, you can use the link to reference a single paragraph, and then expand it, to view it highlighted within the chapter's text. Alternatively, you could view the chapter's text highlighted within the book's text.

The structure is based upon a combination of Asciidoc `image::` links and Biblical citations, and can be used for any publication. It is meant to be used in Asciidoc documents, but it MAY be used in other formats.

An example of the book wikilink is: `book::bible:genesis 2:4-9 | kjv`.

## Specification

### Link Structure

The basic link structure is: `book::[<collection>:]<title> [<chapter_name>][:<section_name_or_range>] [| <version>]`

Multiple references can be comma-separated. When a space follows a comma, it indicates a new book reference. For example: `[ kjv](/wiki/book-bible-gen-2-4-9-romans-1-1-10)` references two different books (genesis and romans).

The link format is human-readable and may contain spaces, colons, and mixed case for display purposes. However, when clients search for events, they MUST normalize the link fields to lowercase ASCII characters, with non-letter characters converted to hyphens, following NIP-54 normalization rules.

If the `section` field is omitted, assume all `30041` sections are included. If the `chapter` field is omitted, assume all `30040` chapters in the book are included.

* `collection` (optional): A library, compendium, digest, series, etc. prefix or label to categorize books/publications, followed by a colon (e.g. "bible:", "a-song-of-ice-and-fire", "readers-digest:" or "american-medical-journal:").
* `title` (mandatory): The book name, magazine article, volume, play, or similar. (e.g. "genesis", "joan-of-arc-book-1", "game-of-thrones", "the-odin-codex", "theory-of-evolution"). This corresponds to a `30040` event that SHOULD only contain other `30040` events.
* `chapter` (optional): The chapter or act name or number (e.g. "2", "introduction", "act-III", "conclusion"). If omitted, all chapters in the book are included. This corresponds to a `30040` event that SHOULD contain at least one `30041` event and MAY contain `30040' events (subchapters).
* `section` (optional): The section, verse or paragraph name or number (e.g. "4", "preface"). Multiple sections can be comma-separated within the same chapter or book context. This corresponds to a `30041` publication content event, containing the text to be read, such as "It was the best of times, it was the worst of times..." 
* `version` (optional): The version, translation or edition of the book, to differentiate between published works that contain the same content. Multiple versions can be space-separated. (e.g. "kjv", "first-edition", "gitcitadel-publishing", "npub1l5sga6xg72phsz5422ykujprejwud075ggrr3z2hwyrfgr7eylqstegx9z")

### Tag Structure

Within the 30040 and 30041 events, the following indexable tags MUST be used to store the metadata required for the link to be resolved, in addition to the tags required by [NKBIP-01](/wiki/nkbip-01), such as the `d` tag. The NKBIP-08 tags, and their corresponding link fields, are:

* `C`: collection
* `T`: title
* `c`: chapter
* `s`: section
* `v`: version

```json
{
    "C": "bible",
    "T": "genesis",
    "c": "2",
    "s": "4",
    "v": "drb"
}
```

Genesis 2:4 of the Douay-Rheims Bible

```json
{
    "C": "shakespeare-complete",
    "T": "hamlet",
    "c": "2",
    "s": "2",
    "v": "oxford-classics
}
```

Act 2, Scene 1 of the Oxford Classics edition of "Hamlet", contained in the compendium "Shakespeare's Complete Works"

```json
{
    "T": "jane-eyre",
    "c": "21",
    "s": "8",
    "v": "penguin-classics"
}
```

Paragraph 8, from chapter 21 of "Jane Eyre", from Penguin Classics

```json
{
    "T": "a-tale-of-two-cities",
    "c": "1",
    "s": "4",
    "v": "1st-edition"
}
```

Paragraph 4, from chapter 1 of "A Tale of Two Cities", from the original edition

Only the 'T' tag MUST be present. The 'C', 'c', 's', and 'v' tags MAY be present, but are not required. All tags MAY be repeated, to support multiple macros, or macros with multiple options (such as allowing both the canonical longform "Song of Solomon" and the shortform "Song" to be used). 

When resolving links:

* If a collection is specified in the link, clients SHOULD search for events with matching 'C' tags in addition to 'T' tags.
* If a version is specified in the link, clients SHOULD search for events with matching 'v' tags.
* If a collection or version is omitted from the link, clients SHOULD NOT filter by those tags (but they MAY use fallback logic as described in the Considerations section).

### Sections

Most sections will tend to be numbered, as they are verses or paragraphs. Some sections have names such as "Preface", "Introduction", "Conclusion", "Table of Contents", "Appendix A", "Appendix B", etc. Ranges are determined by the preceding chapter's a-tag list, which MUST be ordered (see NKBIP-01 specification). A range of "Preface-3" would return the sections "Preface", "1", "2", and "3".

If the chapter's event is not available, and the section range only contains numbers, the client MAY infer the range from the integer values. In this case, "17-21" would return the sections "17", "18", "19", "20", and "21".

### Examples

#### Entire Book, Version Included

```asciidoc
[ drb](/wiki/book-bible-genesis)
```

This example would return the entire book of Genesis, of the Douay-Rheims Bible.

The client would search for the 30040 event with the collection "C" tag with value "bible" (normalized), the book "T" tag with value "genesis" (normalized), and the "v" tag with value "drb" (normalized), and return it and all of its children (branches and leaves), which are the (possibly nested) chapters and sections of the book.

#### Entire Chapter, Version and Collection Omitted

```asciidoc
[book::genesis 2](/wiki/book-genesis-2)
```

This example would return the entire chapter 2 of Genesis.

The client would search for the 30040 event with the book "T" tag with value "genesis" (normalized), and the chapter "c" tag with value "2" (normalized), and return it and all of its children (branches and leaves), which are the (possibly nested) subchapters and sections of the chapter.

#### Particular Section

```asciidoc
[ kjv](/wiki/book-bible-genesis-2-4)
```

This example would return the section "Genesis 2:4", of the King James Version of the Bible.

The client would search for the 30041 event with the collection "C" tag with value "bible" (normalized), the book "T" tag with value "genesis" (normalized), the "c" tag with value "2" (normalized), the "s" tag with value "4" (normalized), and the "v" tag with value "kjv" (normalized).

```asciidoc
[ penguin-classics](/wiki/book-jane-eyre-21-8)
```

This example would return paragraph 8 of chapter 21 of "Jane Eyre", from the Penguin Classics edition.

The client would search for the 30041 event with the book "T" tag with value "jane-eyre" (normalized), the "c" tag with value "21" (normalized), the "s" tag with value "8" (normalized), and the "v" tag with value "penguin-classics" (normalized).

```asciidoc
[ 1st-edition](/wiki/book-a-tale-of-two-cities-1-4)
```

This example would return paragraph 4 of chapter 1 of "A Tale of Two Cities", from the first edition.

The client would search for the 30041 event with the book "T" tag with value "a-tale-of-two-cities" (normalized), the "c" tag with value "1" (normalized), the "s" tag with value "4" (normalized), and the "v" tag with value "1st-edition" (normalized).

```asciidoc
[book::quran:al-baqarah 2:286](/wiki/book-quran-al-baqarah-2-286)
```

This example would return verse 286 of chapter 2 (Surah Al-Baqarah) of the Quran.

The client would search for the 30041 event with the collection "C" tag with value "quran" (normalized), the book "T" tag with value "al-baqarah" (normalized), the "c" tag with value "2" (normalized), and the "s" tag with value "286" (normalized). Since no version is specified, the client MAY return results from any translation or use version selection logic.

```asciidoc
[ no-2](/wiki/book-baltimore-catechism-1-50)
```

This example would return question 50 of lesson 1 of the Baltimore Catechism, from Number 2 (typically used for grades 6-9).

The client would search for the 30041 event with the book "T" tag with value "baltimore-catechism" (normalized), the "c" tag with value "1" (normalized), the "s" tag with value "50" (normalized), and the "v" tag with value "no-2" (normalized).

#### Section Range

```asciidoc
[book::bible:genesis 2:4-9](/wiki/book-bible-genesis-2-4-9)
```

This example would return the sections "Genesis 2:4-9".

The client would search for 30041 events with the collection "C" tag with value "bible" (normalized), the book "T" tag with value "genesis" (normalized), the "c" tag with value "2" (normalized), and the "s" tag matching sections "4" through "9" (normalized). This would return the 30041 events for the sections "Genesis 2:4", "Genesis 2:5", "Genesis 2:6", "Genesis 2:7", "Genesis 2:8", and "Genesis 2:9".

Since no version is specified, the client MAY return results from any version or use version selection logic as described in the Version Handling section.

#### Multiple Sections, Same Book

```asciidoc
[book::bible:gen 2:4-9,11-20,22-25](/wiki/book-bible-gen-2-4-9-11-20-22-25)
```

This example would return the sections "Genesis 2:4-9", "Genesis 2:11-20", and "Genesis 2:22-25".

#### Multiple Sections, Multiple Chapters, Same Book

```asciidoc
[book::bible:gen 2:4-9,4:11-20,4:22-25](/wiki/book-bible-gen-2-4-9-4-11-20-4-22-25)
```

This example would return the sections "Genesis 2:4-9", "Genesis 4:11-20", and "Genesis 4:22-25".

#### Multiple Sections, Different Books

```asciidoc
[ kjv](/wiki/book-bible-gen-2-4-9-romans-1-1-10)
```

This example would return:

* the section "Genesis 2:4-9", of the King James Version of the Bible.
* the section "Romans 1:1-10", of the King James Version of the Bible.

#### Multiple Sections, Same Book and Different Books

```asciidoc
[book::bible:gen 2:4-9,11-20,22-25, romans 1:1-10](/wiki/book-bible-gen-2-4-9-11-20-22-25-romans-1-1-10)
```

This example would return the sections "Genesis 2:4-9", "Genesis 2:11-20", "Genesis 2:22-25", and "Romans 1:1-10". The space after the comma is used to denote a different book.

##### Multiple Versions

```asciidoc
[ kjv niv](/wiki/book-bible-gen-2-4-9-song-of-solomon-1-1-10)
```

This example would return:

* the section "Genesis 2:4-9", of the King James Version of the Bible.
* the section "Song of Solomon 1:1-10", of the King James Version of the Bible.
* the section "Genesis 2:4-9", of the New International Version of the Bible.
* the section "Song of Solomon 1:1-10", of the New International Version of the Bible.

This example demonstrates the usage of different book name formats in one link, with the shortform "gen" (normalized from "Genesis") and the normalized longform "song-of-solomon" (from "Song of Solomon") being used interchangeably.

## Considerations

### Version Handling

When no version is specified in the link, the client MAY:

* determine which version to use (e.g. "KJV", as default, or according to web-of-trust settings, or etc.)
* display all versions found on the relays (e.g. "KJV", "NIV", "ESV", etc.)
* display the first version returned by the relays.

If a version is specified, but not returned by the relays, the client MAY:

* display an error message to the user.
* display a different version returned by the relays.

### Handling of wrongly-entered links

Make sure to normalize the link fields prior to parsing and searching for the events. During normalization, quotes and other non-alphanumeric characters (except hyphens after normalization) SHOULD be removed or converted according to NIP-54 normalization rules. Versions separated by commas (like `| kjv, niv`, should remove the commas.

For example:

Both of the following examples:

```asciidoc
[ KJV](/wiki/book-bible-song-of-solomon-1-1-10)
```

```asciidoc
[ KJV](/wiki/book-bible-song-of-solomon-1-1-10)
```

would be normalized to:

```asciidoc
[ kjv](/wiki/book-bible-song-of-solomon-1-1-10)
```