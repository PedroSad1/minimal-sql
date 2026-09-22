export interface ScanOptions {
  identQuotes: string;
  doubleQuoteIsString: boolean;
  hashComments: boolean;
}

export const postgresScan: ScanOptions = {
  identQuotes: '"',
  doubleQuoteIsString: false,
  hashComments: false,
};

export interface TableRef {
  schema: string | null;
  name: string;
  alias: string | null;
  depth: number;
}

export type CompleteKind = "column" | "table" | "clause" | "skip";

export interface SqlContext {
  from: number;
  parents: string[];
  partial: string;
  kind: CompleteKind;
  refs: TableRef[];
}

interface Tok {
  kind: "word" | "dot" | "comma" | "paren" | "semi";
  text: string;
  from: number;
  to: number;
  quoted: boolean;
}

interface Skip {
  from: number;
  to: number;
}

const TABLE_INTRO = new Set(["from", "join", "into", "update", "table", "using", "truncate"]);
const COLUMN_WORDS = new Set([
  "select",
  "where",
  "on",
  "and",
  "or",
  "not",
  "set",
  "having",
  "returning",
  "by",
  "distinct",
  "when",
  "then",
  "else",
  "in",
  "like",
  "ilike",
  "between",
]);
const CLAUSE_WORDS = new Set([
  "left",
  "right",
  "inner",
  "full",
  "cross",
  "outer",
  "natural",
  "order",
  "group",
  "insert",
  "delete",
  "union",
  "limit",
  "offset",
  "fetch",
  "create",
  "alter",
  "drop",
  "with",
  "values",
  "window",
  "as",
  "all",
  "is",
  "null",
  "true",
  "false",
  "asc",
  "desc",
  "exists",
  "case",
  "end",
  "primary",
  "key",
  "foreign",
  "references",
  "constraint",
  "default",
  "cascade",
  "index",
  "view",
  "begin",
  "commit",
  "rollback",
  "explain",
]);
const SKIP_BEFORE_TABLE = new Set(["only", "lateral", "outer"]);
const STOP = new Set([...TABLE_INTRO, ...COLUMN_WORDS, ...CLAUSE_WORDS, "select", "from", "where", "join"]);

function blank(from: number, kind: CompleteKind): SqlContext {
  return { from, parents: [], partial: "", kind, refs: [] };
}

function closingIdent(ch: string, options: ScanOptions) {
  if (ch === "[" && options.identQuotes.includes("[")) return "]";
  if (ch === "`" && options.identQuotes.includes("`")) return "`";
  if (ch === '"' && !options.doubleQuoteIsString && options.identQuotes.includes('"')) return '"';
  return "";
}

function tokenize(doc: string, options: ScanOptions) {
  const tokens: Tok[] = [];
  const skips: Skip[] = [];
  let i = 0;
  while (i < doc.length) {
    const ch = doc[i];
    const next = doc[i + 1] ?? "";
    if (ch === "-" && next === "-") {
      const from = i;
      i += 2;
      while (i < doc.length && doc[i] !== "\n") i += 1;
      if (doc[i] === "\n") i += 1;
      skips.push({ from, to: i });
      continue;
    }
    if (ch === "/" && next === "*") {
      const from = i;
      i += 2;
      while (i < doc.length && !(doc[i] === "*" && doc[i + 1] === "/")) i += 1;
      i = Math.min(doc.length, i + 2);
      skips.push({ from, to: i });
      continue;
    }
    if (options.hashComments && ch === "#") {
      const from = i;
      while (i < doc.length && doc[i] !== "\n") i += 1;
      if (doc[i] === "\n") i += 1;
      skips.push({ from, to: i });
      continue;
    }
    if (ch === "'") {
      const from = i;
      i = skipQuote(doc, i, "'");
      skips.push({ from, to: i });
      continue;
    }
    if (ch === '"' && options.doubleQuoteIsString) {
      const from = i;
      i = skipQuote(doc, i, '"');
      skips.push({ from, to: i });
      continue;
    }
    if (ch === "$") {
      const end = skipDollar(doc, i);
      if (end > i) {
        skips.push({ from: i, to: end });
        i = end;
        continue;
      }
    }
    const close = closingIdent(ch, options);
    if (close) {
      const from = i;
      i += 1;
      let text = "";
      while (i < doc.length && doc[i] !== close) {
        if (doc[i] === close && doc[i + 1] === close) {
          text += close;
          i += 2;
          continue;
        }
        text += doc[i];
        i += 1;
      }
      if (i < doc.length) i += 1;
      tokens.push({ kind: "word", text, from, to: i, quoted: true });
      continue;
    }
    if (/\s/u.test(ch)) {
      i += 1;
      continue;
    }
    if (ch === ".") {
      tokens.push({ kind: "dot", text: ".", from: i, to: i + 1, quoted: false });
      i += 1;
      continue;
    }
    if (ch === ",") {
      tokens.push({ kind: "comma", text: ",", from: i, to: i + 1, quoted: false });
      i += 1;
      continue;
    }
    if (ch === "(" || ch === ")") {
      tokens.push({ kind: "paren", text: ch, from: i, to: i + 1, quoted: false });
      i += 1;
      continue;
    }
    if (ch === ";") {
      tokens.push({ kind: "semi", text: ";", from: i, to: i + 1, quoted: false });
      i += 1;
      continue;
    }
    if (/[A-Za-z_\u0080-\uFFFF]/u.test(ch)) {
      const from = i;
      i += 1;
      while (i < doc.length && /[A-Za-z0-9_$\u0080-\uFFFF]/u.test(doc[i])) i += 1;
      tokens.push({ kind: "word", text: doc.slice(from, i), from, to: i, quoted: false });
      continue;
    }
    i += 1;
  }
  return { tokens, skips };
}

function skipQuote(doc: string, start: number, quote: string) {
  let i = start + 1;
  while (i < doc.length) {
    if (doc[i] === quote && doc[i + 1] === quote) {
      i += 2;
      continue;
    }
    if (doc[i] === "\\" && quote === "'") {
      i += 2;
      continue;
    }
    if (doc[i] === quote) return i + 1;
    i += 1;
  }
  return doc.length;
}

function skipDollar(doc: string, start: number) {
  let i = start + 1;
  while (i < doc.length && /[A-Za-z0-9_]/u.test(doc[i])) i += 1;
  if (doc[i] !== "$") return start;
  const closer = doc.slice(start, i + 1);
  const end = doc.indexOf(closer, i + 1);
  return end < 0 ? doc.length : end + closer.length;
}

function word(token: Tok | undefined) {
  return token?.kind === "word" ? token.text.toLowerCase() : "";
}

function isStop(token: Tok | undefined) {
  return Boolean(token && token.kind === "word" && !token.quoted && STOP.has(token.text.toLowerCase()));
}

function readName(tokens: Tok[], index: number) {
  const first = tokens[index];
  if (!first || first.kind !== "word") return null;
  if (!first.quoted && STOP.has(first.text.toLowerCase()) && tokens[index + 1]?.kind !== "dot") return null;
  let schema: string | null = null;
  let name = first.text;
  let next = index + 1;
  if (tokens[next]?.kind === "dot" && tokens[next + 1]?.kind === "word") {
    schema = name;
    name = tokens[next + 1].text;
    next += 2;
  }
  return { schema, name, next };
}

function readAlias(tokens: Tok[], index: number) {
  let cursor = index;
  if (word(tokens[cursor]) === "as" && !tokens[cursor].quoted) cursor += 1;
  const token = tokens[cursor];
  if (!token || token.kind !== "word" || isStop(token)) return { alias: null as string | null, next: index };
  return { alias: token.text, next: cursor + 1 };
}

function collectRefs(tokens: Tok[]) {
  const refs: TableRef[] = [];
  let depth = 0;
  let i = 0;
  while (i < tokens.length) {
    const token = tokens[i];
    if (token.kind === "paren") {
      depth += token.text === "(" ? 1 : -1;
      if (depth < 0) depth = 0;
      i += 1;
      continue;
    }
    const intro = token.kind === "word" && !token.quoted ? token.text.toLowerCase() : "";
    if (!TABLE_INTRO.has(intro)) {
      i += 1;
      continue;
    }
    const at = depth;
    const started = i;
    i += 1;
    if (intro === "truncate" && word(tokens[i]) === "table" && !tokens[i].quoted) i += 1;
    while (SKIP_BEFORE_TABLE.has(word(tokens[i])) && !tokens[i].quoted) i += 1;
    if (tokens[i]?.kind === "paren") continue;
    while (i < tokens.length) {
      while (SKIP_BEFORE_TABLE.has(word(tokens[i])) && !tokens[i].quoted) i += 1;
      const name = readName(tokens, i);
      if (!name) break;
      i = name.next;
      const alias = readAlias(tokens, i);
      i = alias.next;
      const ref: TableRef = {
        schema: name.schema,
        name: name.name,
        alias: alias.alias,
        depth: at,
      };
      if (intro === "into" && tokens[i]?.kind === "paren" && tokens[i].text === "(") {
        ref.depth = at + 1;
      }
      refs.push(ref);
      if (tokens[i]?.kind === "comma") {
        i += 1;
        continue;
      }
      break;
    }
    if (i === started) i += 1;
  }
  return refs;
}

function statementTokens(tokens: Tok[], pos: number) {
  let from = 0;
  let depth = 0;
  let to = Number.POSITIVE_INFINITY;
  for (const token of tokens) {
    if (token.kind === "paren") {
      depth += token.text === "(" ? 1 : -1;
      if (depth < 0) depth = 0;
      continue;
    }
    if (token.kind === "semi" && depth === 0) {
      if (pos <= token.from) {
        to = token.from;
        break;
      }
      from = token.to;
    }
  }
  return tokens.filter((token) => token.from >= from && token.to <= to);
}

function kindFor(own: string | null, inherited: string | null, depth: number): CompleteKind {
  const word = own ?? inherited;
  if (!word) return "clause";
  if (own == null && word === "into" && depth > 0) return "column";
  if (TABLE_INTRO.has(word)) return "table";
  if (COLUMN_WORDS.has(word)) return "column";
  return "clause";
}

export function sqlContext(doc: string, pos: number, options: ScanOptions = postgresScan): SqlContext {
  const safe = Math.max(0, Math.min(pos, doc.length));
  const { tokens, skips } = tokenize(doc, options);
  if (skips.some((skip) => safe > skip.from && (safe < skip.to || (safe === skip.to && safe === doc.length)))) {
    return blank(safe, "skip");
  }
  const local = statementTokens(tokens, safe);
  const partial = local.find((token) => token.kind === "word" && token.from < safe && token.to >= safe);
  const from = partial ? partial.from : safe;
  const parents: string[] = [];
  const partialIndex = partial ? local.indexOf(partial) : -1;
  let cursor = partialIndex >= 0 ? partialIndex - 1 : local.length - 1;
  while (cursor >= 0 && local[cursor].to > from) cursor -= 1;
  while (cursor >= 0 && local[cursor].kind === "dot") {
    const name = local[cursor - 1];
    if (!name || name.kind !== "word") break;
    parents.unshift(name.text);
    cursor -= 2;
  }
  const lastAt: (string | null)[] = [null];
  let depth = 0;
  for (const token of local) {
    if (token.from >= from) break;
    if (token.kind === "paren") {
      if (token.text === "(") {
        depth += 1;
        lastAt[depth] = null;
      } else {
        lastAt[depth] = null;
        depth = Math.max(0, depth - 1);
      }
      continue;
    }
    if (token.kind === "word" && !token.quoted && STOP.has(token.text.toLowerCase())) {
      lastAt[depth] = token.text.toLowerCase();
    }
  }
  const own = lastAt[depth] ?? null;
  const inherited = depth > 0 ? (lastAt[depth - 1] ?? null) : null;
  const origin = partial && partial.quoted ? partial.from + 1 : from;
  const contentEnd = partial && partial.quoted && doc[partial.to - 1] === (doc[partial.from] === "[" ? "]" : doc[partial.from])
    ? partial.to - 1
    : (partial?.to ?? safe);
  const typed = partial ? doc.slice(origin, Math.min(safe, contentEnd)) : "";
  const refs = collectRefs(local).filter((ref) => {
    if (ref.depth === depth) return true;
    return ref.depth === depth - 1 && own == null && inherited !== "values" && inherited !== "limit" && inherited !== "offset";
  });
  return {
    from: origin,
    parents,
    partial: typed,
    kind: kindFor(own, inherited, depth),
    refs,
  };
}
