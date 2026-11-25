// What a credential looks like, and how a line that carries one is written down without it.
//
// A tool call and what the tool handed back are written to the agent log, and the log is read on
// the agent's own page, in a room's running turn and under a room's finished answer. Masking is
// done once, where the line is written, so the archive never holds the value and no reader has to
// remember to hide it.
//
// What is masked is the value alone. The flag it was given under, the file it was read from and
// the shape of the command all stay, because what the owner reads such a line for is which tool
// ran on what — a line that masked the whole of itself would answer nothing.

/** What stands where a credential stood. */
export const maskedValue = "«masked value»";

const secretExtensions = new Set([".pem", ".key", ".p12", ".pfx", ".jks", ".keystore"]);
const secretStems = new Set(["credentials", "secret", "secrets"]);
const dataExtensions = new Set(["", ".json", ".yaml", ".yml", ".toml", ".ini", ".txt"]);
const exampleEndings = [".example", ".sample", ".template"];

function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");

  return dot <= 0 ? "" : name.slice(dot).toLowerCase();
}

// Whether a path is a file whose contents are a credential: an env file or the directory of them,
// a key, a credentials or secrets file. One rule, asked by the code review's reading of a checkout
// and by the log's masking of what a tool read.
export function isSecretPath(path: string): boolean {
  const parts = path.split("/");
  const name = (parts.at(-1) ?? "").toLowerCase();

  if (parts.slice(0, -1).some((part) => part.toLowerCase() === "env")) {
    return true;
  }

  if ((name === ".env" || name.startsWith(".env.")) && !exampleEndings.some((ending) => name.endsWith(ending))) {
    return true;
  }

  if (secretExtensions.has(extensionOf(name))) {
    return true;
  }

  if (/^id_(rsa|dsa|ecdsa|ed25519)/.test(name) || name === ".netrc" || name === ".pgpass") {
    return true;
  }

  const extension = extensionOf(name);
  const stem = extension === "" ? name : name.slice(0, -extension.length);

  return secretStems.has(stem) && dataExtensions.has(extension);
}

// A name that holds a credential rather than a setting, whether it is a JSON key, an environment
// variable or the long name of a flag.
const secretName = /^(?:[a-z0-9_.-]*[_.-])?(?:password|passwd|pwd|secret|token|api[_-]?key|apikey|access[_-]?key|auth|credential)s?$/i;

// The shapes a credential is written in inside a line of text. Each keeps what says where the
// value came from and takes the value itself. A shape may begin only where its name begins — at the
// start of a word, or within a bounded reach of what it names — never at every character of one: a
// picture handed back is a single word hundreds of thousands of characters long, and a shape tried
// afresh from each of them is searched to its end each time.
const shapes: [RegExp, string][] = [
  // An authorization header, whichever scheme it carries. The scheme stands as a word of its own:
  // a word that merely ends in one of these, `--token` among them, is a flag and is taken below.
  [/(?<![\w-])(Bearer|Basic|Token)\s+(?!«)[^\s"'`,;]{8,}/gi, `$1 ${maskedValue}`],
  // A key of the shape the model providers issue.
  [/\bsk-([a-z]{3,8})-[A-Za-z0-9_-]{6,}/gi, `sk-$1-${maskedValue}`],
  [/\bsk-[A-Za-z0-9]{20,}/gi, `sk-${maskedValue}`],
  // A password written into an address: scheme://name:password@host. A scheme is a short word.
  [/([a-z][a-z0-9+.-]{0,31}:\/\/[^\s:/@"'`]+:)(?!«)[^\s@"'`]+@/gi, `$1${maskedValue}@`],
  // A flag given its value on the command line, with a space or an equals between. A flag opens a
  // word, so a word that ends in one of these names — a file called `client-secret`, say — is not
  // one and keeps whatever follows it.
  [
    /(?<=^|\s)(--?(?:password|passwd|pwd|token|secret|api-?key|apikey|auth|credential)s?[= ])(?!-|«)[^\s"'`;&|]+/gi,
    `$1${maskedValue}`,
  ],
  // An assignment: a shell variable, a line of an env file, a query string, a field of a config.
  [
    /(?<![A-Za-z0-9_.-])([A-Za-z0-9_.-]*(?:password|passwd|pwd|secret|token|api[_-]?key|apikey|access[_-]?key|credential)s?)(\s*[:=]\s*)(?!«)(?:"[^"]*"|'[^']*'|[^\s"'`;&|,)}]+)/gi,
    `$1$2${maskedValue}`,
  ],
  // The console's own account script, whose second argument is the password, begun at the script's
  // own name whatever path stands before it.
  [/(user\.ts\s+set\s+\S+\s+)(?!«)[^\s"'`;&|]+/g, `$1${maskedValue}`],
];

/** One piece of text with every credential shape in it masked. */
export function maskSecretText(text: string): string {
  return shapes.reduce((masked, [shape, standIn]) => masked.replace(shape, standIn), text);
}

/**
 * A tool's arguments or its result as the log may keep them. A tool's arguments arrive as JSON, so
 * they are masked as the values they are and handed back as JSON — the console reads a command out
 * of that object, and a mask that broke the shape would cost the line its reading. Anything else is
 * masked as the text it is.
 */
export function maskSecrets(text: string): string {
  const parsed = parsedObject(text);

  return parsed === null ? maskSecretText(text) : JSON.stringify(maskedWithin(parsed));
}

function parsedObject(text: string): unknown {
  const trimmed = text.trimStart();

  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) {
    return null;
  }

  try {
    return JSON.parse(text) as unknown;
  } catch {
    return null;
  }
}

// A value of a tool's arguments with what it carries masked: a field whose name says it holds a
// credential loses the whole value — a string, and a login given as an object or a list alike, whose
// parts are named by nothing a credential is recognised by — and every other string keeps whatever
// is not one. A number or a flag under such a name is left: a count of tokens is a count.
function maskedWithin(value: unknown): unknown {
  if (typeof value === "string") {
    return maskSecretText(value);
  }

  if (Array.isArray(value)) {
    return value.map(maskedWithin);
  }

  if (value === null || typeof value !== "object") {
    return value;
  }

  return Object.fromEntries(
    Object.entries(value).map(([name, held]) => [
      name,
      secretName.test(name) && (typeof held === "string" || (typeof held === "object" && held !== null)) ? maskedValue : maskedWithin(held),
    ]),
  );
}

// How long a word may be and still be looked at as a path: past this it is a body of text that
// happens to hold a slash.
const longestPath = 1024;

/**
 * Whether a tool call reads or writes a file whose contents are a credential, which is what says
 * its result must not be kept. The arguments are searched whole — the named path of a file tool
 * and the words of a shell command alike — because either is how such a file is opened.
 */
export function readsSecretFile(toolArguments: string): boolean {
  return words(toolArguments).some((word) => word.includes("/") && word.length <= longestPath && isSecretPath(word));
}

function words(toolArguments: string): string[] {
  const parsed = parsedObject(toolArguments);
  const texts = parsed === null ? [toolArguments] : strings(parsed);

  return texts.flatMap((text) => text.split(/[\s"'`;&|(){}[\]<>,]+/)).filter((word) => word !== "");
}

function strings(value: unknown): string[] {
  if (typeof value === "string") {
    return [value];
  }

  if (Array.isArray(value)) {
    return value.flatMap(strings);
  }

  return value === null || typeof value !== "object" ? [] : Object.values(value).flatMap(strings);
}
