# Telegram markup rules

- The rulebook of the converter on the send path, read by no model. It sets what is valid in the
  Telegram Bot API's HTML parse mode, and what the markdown written in an answer becomes in that
  HTML.
- One premise: **the supervisor writes markdown, and the bot's code makes the HTML.**
  - When the writer writes tags directly, one missed escape gets the whole message refused and
    nothing is left in the room.
  - So the one place HTML is made is the converter, which also makes the same text as plain text
    with the marks removed, held as the fallback.
- Which marks the writer has and when to use them are the writer's rules, in `answer-shape.md`;
  whether a message is folded is the writer's own judgement, marked on the answer under the rule
  its room's capability section gives, and the converter folds only a send that asks for it.
- The dialect the send path carries them out in, `telegram-html`, is the Telegram provider's own
  (its `docs/telegram.md`).

## Basic rules of HTML mode

- Outside tags, `<`, `>` and `&` become `&lt;`, `&gt;` and `&amp;`; those three are all.
- The only named entities Telegram knows are `&lt;` `&gt;` `&amp;` `&quot;`; every numeric entity
  works. Attribute values are wrapped in double quotes, so a `"` inside a URL becomes `&quot;`.
- **A tag not in the list below is not ignored: the whole request is refused with 400.** `<div>`
  returns `Bad Request: can't parse entities: Unsupported start tag "div" at byte offset 0` and the
  message is not sent: a formatting failure is a lost message, not a quiet one.
- Line breaks survive as they are. `<br>` is not a supported tag and is refused as above.

## Formatting available

- Each entry gives the form, the escaping, the version it came in and the constraints.
- **Bold:** `<b>bold</b>`, `<strong>bold</strong>`; inside, escape only `< > &`. Supported from the
  start (no version noted). MarkdownV2 `*bold*`.
- **Italic:** `<i>italic</i>`, `<em>italic</em>`; no version noted. MarkdownV2 `_italic_`.
- **Underline:** `<u>underline</u>`, `<ins>underline</ins>`. Bot API 4.5 (31 December 2019), with
  strikethrough; the same release brought entity nesting and MarkdownV2. MarkdownV2 `__underline__`.
- **Strikethrough:** `<s>strikethrough</s>`, `<strike>strikethrough</strike>`, `<del>strikethrough</del>`. Bot API 4.5.
  MarkdownV2 `~strikethrough~`.
- **Spoiler:** `<tg-spoiler>spoiler</tg-spoiler>` or `<span class="tg-spoiler">spoiler</span>`. Bot API
  5.6 (30 December 2021). MarkdownV2 `||spoiler||`.
  - The content shows only when the recipient taps it.
  - Hiding a photo or video itself is a separate send parameter (`has_spoiler`), not formatting.
- **Inline code:** `<code>cargo test --workspace</code>`; inside, too, escape
  only `< > &` (escaping backticks and backslashes is MarkdownV2's alone).
  - No other formatting goes inside code or a code block, and code cannot go inside formatting
    such as bold or italic.
- **Code block:** `<pre>multiple lines</pre>`; spaces and line breaks inside are kept.
  - A language is named by `<code>` as the only child of `<pre>` with a class:
    `<pre><code class="language-python">...</code></pre>`. `<code>` alone cannot name one; the
    languages are libprisma's supported-languages.
- **Link:** `<a href="https://example.com/">visible text</a>`; inside href, `&` becomes `&amp;` and
  `"` becomes `&quot;`.
  - Bold, italic, underline, strikethrough and spoiler can go inside a link; code and code blocks
    cannot.
  - `href="tg://user?id=123456789"` mentions a user by id alone. It works only in inline links and
    inline keyboard buttons, and takes hold reliably only when the user has had a private chat
    with the bot or is a member of that room.
  - Clients show the full URL in a confirmation before opening an inline link: link text hides the
    address only where the message is read.
- **Quote:** `<blockquote>quotation</blockquote>`. Bot API 7.0 (29 December 2023). MarkdownV2 `>` at
  the start of a line.
- **Expandable quote:** `<blockquote expandable>long quotation</blockquote>`. Bot API 7.4 (28 May 2024).
  Arrives folded and unfolds when tapped.
- **Custom emoji:** `<tg-emoji emoji-id="5368324170671202286">🙂</tg-emoji>`.
  - Bot API 6.7 (21 April 2023) for bots that bought an extra username on Fragment; from Bot API
    9.4 (9 February 2026) also in private, group and supergroup messages the bot sends itself
    when its owner has Telegram Premium.
  - The tag holds one real emoji character, shown wherever the custom emoji cannot be.
  - **Neither condition has been confirmed, so this bot does not use it.**
- **Date and time:** `<tg-time unix="1647531900" format="wDT">22:45 tomorrow</tg-time>`. Bot API 9.5
  (1 March 2026).
  - Shown in the reader's local format. The format string follows `r|w?[dD]?[tT]?`: `r` relative
    time from now, combined with no other letter; `w` weekday; `d`/`D` short/long date; `t`/`T`
    short/long time.
  - Left empty, the text inside is shown as it is.

## Nesting rules

- When two entities share characters, one lies wholly inside the other; half overlapping is not
  allowed.
- Bold, italic, underline, strikethrough and spoiler freely contain and are contained by each
  other. Code and code blocks are the exception.
- A quote and an expandable quote cannot nest.
- Nothing else may contain anything else.

## Limits

- Message text: 1–4096 characters after entities are parsed; tags do not count.
- Captions of photos, videos, documents and the like: 0–1024 characters, with the same formatting
  as text; much shorter than text, so nothing long goes in a caption.
- Reply quotes: 0–1024 characters. Story captions: 0–2048 characters.
- The official documentation sets no upper limit on entities in one message; none is confirmed, so
  none is relied on.

## What markdown becomes

- The converter reads the answer's markdown once and makes the HTML and the plain text together.
- A mark is either consumed where the formatting it made stands, or left as a character because it
  found no pair, so a mark and a tag never remain on top of each other.
- It emits only bold, italic, strikethrough, spoiler, inline code, code block, link, quote, and the
  expandable quote of folding. Underline has no markdown mark and is never made.

### Each mark

- The marks that become formatting, when a mark is left as a character, the marks laid flat and
  what is not formatting are the writer's list in `answer-shape.md` (「How to write in markdown」);
  the converter carries it out. Beyond that list:
- **Emphasis pairs** stuck to Korean letters on either side are pairs; the closing-space,
  stuck-to-a-letter and no-pair cases apply to `**`, `*`, `~~` and `||` alike.
- **Backticks.** The same value wrapped several times is code at each place. Characters inside code
  are only escaped, never read as other marks. An unpaired backtick is left as a character.
- **Code inside formatting.** Formatting such as bold that holds code is split before and after the
  code instead of wrapping it; code inside link text stands as characters without a code tag. Both
  are nesting Telegram forbids.
- **Fences** may also be three tildes, and the language named on the opening line becomes the
  block's. The fence lines disappear; an unclosed fence is a code block to the end of the text.
- **Links.** An address that is not http or https does not become a link; the address is written
  out after the text. The image mark also becomes a link.
  - A bare address stays as characters, and Telegram shows it as a link on its own. Where a Korean
    letter follows it with no space, directly or after a bracket closing right behind it, it becomes a
    link ending before that letter and any bracket it did not open, since Telegram would read the
    letter into the address.
- **Quotes** do not nest, so `>>` is also one level of quote.
- **Tables.** The separator row disappears, and the cells are joined by ` · `. A vertical bar
  inside inline code or a link address is not a cell boundary.
- **Backslash.** Before a backtick, asterisk, tilde, vertical bar, square bracket or backslash, the
  backslash is removed and the character is left.
- **An HTML tag** written in an answer is escaped and shown as the characters written.
- **The one-bold rule** is `answer-shape.md`'s (「When to use formatting」). Code measures it after
  conversion, and a leading `•` or number does not count against a bold taking its whole line.
- **A quote in the folded part** loses its quote tag.

### What counts as a loss

- In verification, none of the above is a loss: a mark became formatting, a mark with no matching
  formatting was only removed, or a rule took formatting back.
- Invented characters, dropped characters and changed sentences are a loss.

### The plain text

- The plain text the same conversion makes is what goes out when Telegram refuses the HTML, and to
  rooms without formatting (KakaoTalk).
- The marks are removed, a link has its address in parentheses after its text, and a code block
  stands as its lines without fences.
