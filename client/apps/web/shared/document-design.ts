// The archive's one document design (bmo-artifact's `docs/archive.md`, 「How a document is drawn」): the palette
// every document is drawn in, in a light and a dark scheme, and the sheet a page document goes out
// with. The console's document screen sets the palette through its head and draws a markdown body
// in it (the finding's reading in main.css); a page carries the sheet ahead of its own lines, so
// both are drawn out of the colours kept here.

// What a reader asked to read in: the console's screen mode, where "system" is whatever the
// reader's own system asks for.
export type DocumentScheme = "light" | "dark" | "system";

// A reader who never chose a mode reads dark.
export const DEFAULT_SCHEME: DocumentScheme = "dark";

// The cookie the console keeps the screen mode in, so the server knows it for what it sends.
export const SCHEME_COOKIE = "nuxt-color-mode";

export function isDocumentScheme(value: unknown): value is DocumentScheme {
  return value === "light" || value === "dark" || value === "system";
}

export interface DocumentPalette {
  paper: string;
  surface: string;
  sunk: string;
  ink: string;
  inkSoft: string;
  muted: string;
  rule: string;
  ruleSoft: string;
  accent: string;
  accentSoft: string;
  ok: string;
  okSoft: string;
  warn: string;
  warnSoft: string;
}

// A warm paper with a raised and a sunk surface on it, the ink in three strengths, a claret accent
// and a calm and a caution tone. Every ink and tone reads at 4.5:1 or better on each surface of its
// own scheme, its own soft tint included.
export const LIGHT_PALETTE: DocumentPalette = {
  paper: "#f6f5f1",
  surface: "#ffffff",
  sunk: "#edebe4",
  ink: "#16191d",
  inkSoft: "#3d424a",
  muted: "#5c6169",
  rule: "#d9d6cd",
  ruleSoft: "#e7e4dc",
  accent: "#8c2231",
  accentSoft: "#f0dddf",
  ok: "#1e6656",
  okSoft: "#dcebe5",
  warn: "#7f5310",
  warnSoft: "#f3e6ce"
};

export const DARK_PALETTE: DocumentPalette = {
  paper: "#141416",
  surface: "#1c1c1f",
  sunk: "#232327",
  ink: "#ecebe7",
  inkSoft: "#cfccc5",
  muted: "#a3a09a",
  rule: "#36363c",
  ruleSoft: "#2a2a2f",
  accent: "#f1909d",
  accentSoft: "#3c2228",
  ok: "#7fcfb6",
  okSoft: "#1b3530",
  warn: "#e8b866",
  warnSoft: "#3a2e17"
};

// Each colour as the variable a sheet names it by: `--doc-` and the colour's name in kebab case,
// which is also what a page's own additions colour themselves with.
function variables(palette: DocumentPalette): string {
  return Object.entries(palette)
    .map(([name, colour]) => `--doc-${name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`)}:${colour}`)
    .join(";");
}

// The palette for the console, which marks its dark mode with the `dark` class on the root.
export const DOCUMENT_PALETTE = `:root{${variables(LIGHT_PALETTE)}}:root.dark{${variables(DARK_PALETTE)}}`;

// The palette of one scheme, written on the root with the browser's own colour scheme to match, so
// the canvas, the scrollbars and the form controls are drawn in it too.
function schemePalette(scheme: DocumentScheme): string {
  if (scheme === "light") return `:root{${variables(LIGHT_PALETTE)};color-scheme:light}`;
  if (scheme === "dark") return `:root{${variables(DARK_PALETTE)};color-scheme:dark}`;

  return `:root{${variables(LIGHT_PALETTE)};color-scheme:light dark}@media (prefers-color-scheme:dark){:root{${variables(DARK_PALETTE)}}}`;
}

// How a page is to be drawn, read off what it brings (bmo-artifact's `docs/archive.md`, 「An HTML page」): wholly
// in the archive's design where it brings no stylesheet of its own; in its own layout and parts
// under the archive's type otherwise, turned dark by inverting it where it declares no dark scheme of
// its own; and as it draws itself where it does.
export type PageDesign = "archive" | "own" | "own-dark";

// A light page's own colours read on a dark ground when the whole page is inverted with its hues
// turned back, its contrast eased a little so its white does not become a black hole. What is a
// picture rather than a colour is inverted again so it stands as it is.
const INVERTED =
  "html{filter:invert(1) hue-rotate(180deg) contrast(.92);background:#fff}" +
  "img,video,picture,canvas,iframe,embed,object,svg image{filter:invert(1) hue-rotate(180deg)}";

// A rule of the sheet: what it selects and what it sets.
type Rule = readonly [selector: string, declarations: string];

// The archive's type over the bare elements of a page: the face, the headings, the paragraphs and
// lists, the emphasis, the links, the code, the quotations, the tables, the pictures and the folds,
// every colour one of the palette's variables.
const ELEMENT_RULES: readonly Rule[] = [
  ["html", "background:var(--doc-paper);color:var(--doc-ink);-webkit-text-size-adjust:100%"],
  ["body", 'color:var(--doc-ink);font-family:"Pretendard",-apple-system,BlinkMacSystemFont,"Apple SD Gothic Neo","Noto Sans KR","Malgun Gothic",system-ui,sans-serif;font-size:16px;line-height:1.75;font-variant-numeric:tabular-nums;-webkit-font-smoothing:antialiased;overflow-wrap:break-word;word-break:keep-all'],
  ["h1,h2,h3,h4,h5,h6", "color:var(--doc-ink);line-height:1.35;text-wrap:balance"],
  ["h1", "margin:0 0 16px;font-size:36px;font-weight:700;line-height:1.25;letter-spacing:-.02em"],
  ["h2", "margin:48px 0 16px;padding-bottom:8px;border-bottom:1px solid var(--doc-rule);font-size:24px;font-weight:700;letter-spacing:-.01em"],
  ["h3", "margin:32px 0 12px;font-size:20px;font-weight:700"],
  ["h4,h5,h6", "margin:24px 0 8px;font-size:18px;font-weight:600"],
  ["p,ul,ol,dl,pre,blockquote,figure,details,table", "margin-top:16px;margin-bottom:16px"],
  ["ul,ol", "padding-left:24px"],
  ["li", "margin:4px 0"],
  ["strong,b", "font-weight:700;color:var(--doc-ink)"],
  ["a", "color:var(--doc-accent);text-decoration:underline;text-underline-offset:2px"],
  ["a:hover,a:active", "color:var(--doc-ink);text-decoration-thickness:2px"],
  ["hr", "margin:32px 0;border:0;border-top:1px solid var(--doc-rule)"],
  ["small", "color:var(--doc-muted)"],
  ["code,kbd,samp", 'padding:0 4px;border-radius:4px;background:var(--doc-sunk);font-family:ui-monospace,"Cascadia Code",SFMono-Regular,Menlo,Consolas,monospace;font-size:14px'],
  ["pre", "padding:12px 16px;border:1px solid var(--doc-rule);border-radius:8px;background:var(--doc-sunk);font-size:14px;line-height:1.6;white-space:pre-wrap;overflow-wrap:anywhere"],
  ["pre code", "padding:0;background:none"],
  ["blockquote", "margin-inline:0;padding:16px;border-left:4px solid var(--doc-accent);border-radius:0 8px 8px 0;background:var(--doc-sunk);color:var(--doc-ink)"],
  ["table", "border-collapse:collapse;font-size:14px;line-height:1.6;background:var(--doc-surface)"],
  ["th,td", "padding:8px 12px;border:0;border-bottom:1px solid var(--doc-rule-soft);text-align:left;vertical-align:top"],
  ["th", "border-bottom-color:var(--doc-rule);background:var(--doc-sunk);color:var(--doc-muted);font-weight:600;letter-spacing:.04em"],
  ["tr:last-child>td", "border-bottom:0"],
  ["img,video,svg", "max-width:100%;height:auto"],
  ["figcaption", "margin-top:8px;color:var(--doc-muted);font-size:14px"],
  ["summary", "font-weight:600;cursor:pointer"]
];

// The same, narrowed for a phone.
const NARROW_ELEMENT_RULES: readonly Rule[] = [
  ["h1", "font-size:30px"],
  ["h2", "font-size:20px"],
  ["h3", "font-size:18px"]
];

// What a page drawn wholly in the archive's design takes beyond the type: the reading column, the
// console's reading width of 768px with its padding outside it, the masthead, the parts it names
// and the tables boxed and scrolling inside themselves.
const PAGE_RULES = [
  "*,*::before,*::after{box-sizing:border-box}",
  "body{margin:0 auto;max-width:calc(48rem + 40px);padding:48px 20px 96px}",
  "body>header:first-child{margin:0 0 32px;padding-bottom:20px;border-bottom:2px solid var(--doc-ink)}",
  "body>header:first-child>h1{margin:0}",
  "body>header:first-child>p{margin:12px 0 0;color:var(--doc-ink-soft)}",
  ".eyebrow{margin:0 0 8px;font-size:14px;font-weight:600;letter-spacing:.08em;color:var(--doc-accent)}",
  "body>header:first-child>p.eyebrow{margin:0 0 8px;color:var(--doc-accent)}",
  ".muted{color:var(--doc-muted)}",
  "li>ul,li>ol{margin:4px 0}",
  ".lede,aside,.listen,.grid,.table{margin:16px 0}",
  "blockquote>:first-child,aside>:first-child,.lede>:first-child,.grid>*>:first-child{margin-top:0}",
  "blockquote>:last-child,aside>:last-child,.lede>:last-child,.grid>*>:last-child{margin-bottom:0}",
  ".lede{padding:16px 20px;border-left:4px solid var(--doc-accent);border-radius:0 8px 8px 0;background:var(--doc-sunk);font-size:18px;line-height:1.7}",
  ".lede strong,.lede b{color:var(--doc-accent)}",
  "aside{padding:12px 16px;border:1px solid var(--doc-rule);border-radius:8px;background:var(--doc-surface)}",
  "aside.ok{border-color:var(--doc-ok);background:var(--doc-ok-soft)}aside.warn{border-color:var(--doc-warn);background:var(--doc-warn-soft)}",
  "aside.ok>strong:first-child,aside.ok>b:first-child{color:var(--doc-ok)}aside.warn>strong:first-child,aside.warn>b:first-child{color:var(--doc-warn)}",
  ".chip{display:inline-block;padding:0 8px;border-radius:999px;background:var(--doc-sunk);color:var(--doc-ink-soft);font-size:14px;font-weight:600;line-height:1.6;white-space:nowrap}",
  ".chip.ok{background:var(--doc-ok-soft);color:var(--doc-ok)}.chip.warn{background:var(--doc-warn-soft);color:var(--doc-warn)}.chip.bad{background:var(--doc-accent-soft);color:var(--doc-accent)}",
  ".grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(16rem,1fr));gap:16px}",
  ".grid>*{margin:0;padding:16px;border:1px solid var(--doc-rule);border-radius:8px;background:var(--doc-surface)}",
  ".listen{display:flex;flex-wrap:wrap;align-items:center;gap:8px 16px;padding:12px 16px;border:1px solid var(--doc-rule);border-left:4px solid var(--doc-accent);border-radius:0 8px 8px 0;background:var(--doc-surface)}",
  ".listen>:not(audio){margin:0;color:var(--doc-muted);font-size:14px}.listen audio{flex:1 1 16rem;min-width:0;height:40px}",
  ".table{max-width:100%;overflow-x:auto;border:1px solid var(--doc-rule);border-radius:8px;background:var(--doc-surface)}",
  ":not(.table)>table{display:block;max-width:100%;width:max-content;overflow-x:auto;border:1px solid var(--doc-rule);border-radius:8px}",
  ".table>table{width:100%;margin:0}",
  "th,td{min-width:6em}th{white-space:nowrap}",
  "figure{margin-inline:0;text-align:center}figure img{display:block;margin:0 auto;border:1px solid var(--doc-rule);border-radius:8px}",
  "details{padding:0 16px;border:1px solid var(--doc-rule);border-radius:8px;background:var(--doc-surface)}",
  "summary{padding:12px 0}details[open]>summary{border-bottom:1px solid var(--doc-rule-soft)}",
  "footer{margin-top:64px;padding-top:16px;border-top:1px solid var(--doc-rule);color:var(--doc-muted);font-size:14px}",
  "@media (max-width:640px){body{padding:32px 16px 64px}.lede{font-size:16px}}",
  "@media print{body{max-width:none;padding:0}.listen{display:none}}"
].join("");

// The rules as a sheet, each selector written under `scope` where one is given. A page with a design
// of its own takes the element rules under the root's own element, which outweighs what it wrote
// for a bare element and gives way to everything it named by a class or an id.
function sheet(rules: readonly Rule[], scope = ""): string {
  const scoped = (one: string): string => (one === "html" ? one : `${scope}${one}`);

  return rules.map(([selector, declarations]) => `${selector.split(",").map(scoped).join(",")}{${declarations}}`).join("");
}

function elementSheet(scope = ""): string {
  return `${sheet(ELEMENT_RULES, scope)}@media (max-width:640px){${sheet(NARROW_ELEMENT_RULES, scope)}}`;
}

// The sheet a page goes out with, for the scheme its reader reads in. A page drawn in the archive's
// design takes the whole of it in the palette of that scheme. A page with a design of its own keeps
// its layout and every part it named, takes the archive's type over its bare elements in the light
// palette, and where it declares no dark scheme it is inverted where the scheme is dark: drawn as a
// light page, so what the browser fills in for it is light too and turns dark with the rest of it. A
// page that declares its own dark scheme draws itself, in the scheme the reader chose where they
// chose one and in what their system asks otherwise.
export function pageSheet(scheme: DocumentScheme, design: PageDesign): string {
  if (design === "archive") return `${schemePalette(scheme)}${elementSheet()}${PAGE_RULES}`;
  if (design === "own-dark") return `:root{color-scheme:${scheme === "system" ? "light dark" : scheme}}`;

  const light = `${schemePalette("light")}${elementSheet("html ")}`;

  if (scheme === "light") return light;
  if (scheme === "dark") return `${light}${INVERTED}`;

  return `${light}@media (prefers-color-scheme:dark){${INVERTED}}`;
}
