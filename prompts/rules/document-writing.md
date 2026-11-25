# Document writing

Every agent that writes or edits a stored document follows this rulebook: the repository's
documents, the knowledge library, room rules, person books and lore. An answer to a person (a chat
reply, a report, a summary returned to the caller) is not a stored document; its own instructions
set its language. A document of the archive (what the search runner writes about one search, or
what the supervisor writes there when asked to record something) is outside the language rule and stays in
the language of the person it was written for, never translated; the diagram rule under Shape and
`plain-writing.md` still hold for it.

## Language

- Write the document in English: headings, list labels, brackets, statements, index nouns.
- Keep in the original language whatever loses its meaning in translation:
  - names, nicknames and forms of address (the owner's name, a term of address such as older
    brother or older sister, honorific or casual speech)
  - examples of how someone talks: sentence endings, catchphrases, a short phrase of theirs
  - short quotes, and every passage quoted from a source
  - domain terms bound to their language: a room's own words, an internal name, a product or
    statute name
- Code, commands, paths, links, identifiers and error messages stay exactly as written.
- A document you edit still in another language is carried into English whole, every statement
  kept.

## Shape

- Divide the document into short sections, so a reader finds one thing fast and reads only that.
- Make the top-level sections broad, self-contained units that could each stand as a tab.
- Write bullet-style: short, clipped statements, never long paragraphs.
- Write the words plainly, free of the habits `plain-writing.md` lists: no dash between words, no
  bold scattered through sentences, no bullet opening on a bold label and a colon, no filler
  sentence a reader would not miss.
- List facts as numbered or bulleted items, one thing per item.
- Give a long item a short summary title with its body under it, so the title alone tells a
  reader whether to read on.
- Draw structure rather than tell it: how parts connect, the order something flows in, its stages
  and what depends on what are a diagram, with a line beside it for what the picture cannot say.
  - In markdown, a fenced `mermaid` block, top to bottom (`flowchart TD`), a few words to a box so
    it reads on a phone; the console draws it.
  - Name no colour: the console gives each series its own hue in either theme. A pie or a chart
    keeps to eight series, the rest folded into one other; boxes of a flow differing in kind are
    marked `class A,B c1` through `c8`, one class to a kind.
  - In an HTML page, inline SVG in the page's own colours, scaled by its view box.
- A fixed format the agent's own instructions set (a chunk template, a bracket, an index line)
  wins over the general shape above.
