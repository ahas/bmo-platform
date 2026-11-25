import { Lexer } from "marked";

import { webAddresses } from "./web-address";

import type { FoundAddress } from "./web-address";
import type { Token, Tokens } from "marked";

// The web addresses a message's words carry, read by the one rule every place that finds them
// shares (see web-address): a markdown link's address is the one its parentheses hold, and a bare
// address ends where the sentence takes over.

export interface LinkPart {
  text: string;
  // The address a part opens, or null for words.
  href: string | null;
}

// The words split into plain runs and addresses, in order, so every address can be pressed.
export function linkParts(text: string): LinkPart[] {
  const parts: LinkPart[] = [];
  let at = 0;

  for (const address of webAddresses(text)) {
    if (address.start > at) {
      parts.push({ text: text.slice(at, address.start), href: null });
    }

    parts.push({ text: text.slice(address.start, address.end), href: address.url });
    at = address.end;
  }

  if (at < text.length) {
    parts.push({ text: text.slice(at), href: null });
  }

  return parts;
}

// How many previews one message stands at most.
export const MAX_LINK_PREVIEWS = 3;

// A markdown token's children, as the groups whose raw text runs on unbroken: a list's items, a
// table's cells, and any other token's own inline or block tokens.
function childGroups(token: Token): Token[][] {
  if (token.type === "list") {
    return (token as Tokens.List).items.map((item) => item.tokens);
  }

  if (token.type === "table") {
    const table = token as Tokens.Table;

    return [...table.header, ...table.rows.flat()].map((cell) => cell.tokens);
  }

  return "tokens" in token && Array.isArray(token.tokens) ? [token.tokens] : [];
}

function isCode(token: Token): boolean {
  return token.type === "code" || token.type === "codespan";
}

function holdsCode(token: Token): boolean {
  return isCode(token) || childGroups(token).some((group) => group.some(holdsCode));
}

// The stretches of a message's words that stand outside code, each to be read for addresses on its
// own. A token holding no code is taken as it was written, so the words around code are read exactly
// as a message without any is; a link is taken whole, its address being no code whatever its label
// holds; and code itself parts the stretches, so no address runs across it.
function outsideCode(tokens: readonly Token[]): string[] {
  const stretches: string[] = [];
  let run = "";

  for (const token of tokens) {
    if (token.type === "link" || token.type === "image" || !holdsCode(token)) {
      run += token.raw;
      continue;
    }

    stretches.push(run);
    run = "";

    if (!isCode(token)) {
      stretches.push(...childGroups(token).flatMap(outsideCode));
    }
  }

  stretches.push(run);

  return stretches;
}

// The addresses a message's words carry outside code. An address in a fenced block or in inline code,
// as markdown reads them, is a part of something to copy rather than a page to visit.
function addressesOutsideCode(text: string): FoundAddress[] {
  const tokens = new Lexer({ gfm: true }).lex(text);

  return tokens.some(holdsCode) ? outsideCode(tokens).flatMap((stretch) => webAddresses(stretch)) : webAddresses(text);
}

// The distinct addresses of a message outside its code, first seen first, as many as get a preview.
export function messageLinks(text: string, limit: number = MAX_LINK_PREVIEWS): string[] {
  const seen: string[] = [];

  for (const { url } of addressesOutsideCode(text)) {
    if (!seen.includes(url)) {
      seen.push(url);
    }
  }

  return seen.slice(0, limit);
}
