import {
  isSqlKeyword,
  keywordNext,
  scanSql,
  sqlTableRefs,
  SQL_KEYWORDS,
  type ScanOptions,
  type SqlToken,
} from "./sql-context";

export type SqlRole = "clause" | "table" | "field" | "error";

export interface SqlMark {
  from: number;
  to: number;
  role: SqlRole;
}

export interface SqlTableName {
  schema: string | null;
  name: string;
}

export interface SqlCatalog {
  tables: SqlTableName[];
  columns: (schema: string | null, table: string) => readonly string[] | null;
}

type Mode = "clause" | "column" | "table" | "maybe-alias" | "table-alias" | "col-alias" | "loose" | "type" | "qualified";

interface Ref {
  schema: string | null;
  name: string;
  alias: string | null;
}

interface Qualifier {
  schema: string | null;
  name: string;
  kind: "schema" | "table";
}

const LONG_KEYWORDS = SQL_KEYWORDS.filter((word) => word.length >= 4);

function lower(word: string) {
  return word.toLowerCase();
}

function same(left: string | null | undefined, right: string | null | undefined) {
  return lower(left ?? "") === lower(right ?? "");
}

function systemSchema(name: string) {
  const value = lower(name);
  return (
    value === "pg_catalog" ||
    value === "information_schema" ||
    value === "performance_schema" ||
    value === "mysql" ||
    value === "sys" ||
    value.startsWith("pg_toast") ||
    value.startsWith("pg_temp")
  );
}

function distance(left: string, right: string) {
  if (Math.abs(left.length - right.length) > 1) return 2;
  const cols = right.length + 1;
  const cell = (i: number, j: number) => i * cols + j;
  const score = new Array<number>((left.length + 1) * cols).fill(0);
  for (let i = 0; i <= left.length; i += 1) score[cell(i, 0)] = i;
  for (let j = 0; j <= right.length; j += 1) score[cell(0, j)] = j;
  for (let i = 1; i <= left.length; i += 1) {
    for (let j = 1; j <= right.length; j += 1) {
      const cost = left[i - 1] === right[j - 1] ? 0 : 1;
      let best = Math.min(score[cell(i - 1, j)] + 1, score[cell(i, j - 1)] + 1, score[cell(i - 1, j - 1)] + cost);
      if (i > 1 && j > 1 && left[i - 1] === right[j - 2] && left[i - 2] === right[j - 1]) {
        best = Math.min(best, score[cell(i - 2, j - 2)] + 1);
      }
      score[cell(i, j)] = best;
    }
  }
  return score[cell(left.length, right.length)] ?? 2;
}

function nearKeyword(word: string) {
  const value = lower(word);
  if (value.length < 4 || isSqlKeyword(value)) return false;
  return LONG_KEYWORDS.some((key) => distance(value, key) === 1);
}

function keywordPrefix(word: string) {
  const value = lower(word);
  return SQL_KEYWORDS.some((key) => key.startsWith(value) && key.length > value.length);
}

function statements(tokens: SqlToken[]) {
  const parts: SqlToken[][] = [];
  let current: SqlToken[] = [];
  let depth = 0;
  for (const token of tokens) {
    if (token.kind === "paren") {
      depth += token.text === "(" ? 1 : -1;
      if (depth < 0) depth = 0;
    }
    if (token.kind === "semi" && depth === 0) {
      parts.push(current);
      current = [];
      continue;
    }
    current.push(token);
  }
  if (current.length) parts.push(current);
  return parts;
}

export function sqlMarks(doc: string, options: ScanOptions, catalog: SqlCatalog, cursor = -1) {
  const marks: SqlMark[] = [];
  const seen = new Map<string, SqlTableName>();
  for (const part of statements(scanSql(doc, options))) {
    paint(part, catalog, cursor, marks, seen);
  }
  return { marks, tables: [...seen.values()] };
}

function paint(
  part: SqlToken[],
  catalog: SqlCatalog,
  cursor: number,
  marks: SqlMark[],
  seen: Map<string, SqlTableName>,
) {
  let mode: Mode = "clause";
  let depth = 0;
  const stack: Mode[] = [];
  const refs: Ref[] = sqlTableRefs(part).map((ref) => {
    const found = catalog.tables.find((table) => same(table.name, ref.name) && (ref.schema == null || same(table.schema, ref.schema)));
    return {
      schema: ref.schema ?? found?.schema ?? null,
      name: found?.name ?? ref.name,
      alias: ref.alias,
    };
  });
  let qualifier: Qualifier | null = null;
  let tableMustExist = true;
  const hasCatalog = catalog.tables.length > 0;

  function add(token: SqlToken, role: SqlRole) {
    marks.push({ from: token.from, to: token.to, role });
  }

  function typing(token: SqlToken) {
    return cursor >= token.from && cursor <= token.to;
  }

  function remember(schema: string | null, name: string) {
    const key = `${lower(schema ?? "")}\0${lower(name)}`;
    if (!seen.has(key)) seen.set(key, { schema, name });
  }

  function knownTable(name: string, schema: string | null) {
    return catalog.tables.some((table) => same(table.name, name) && (schema == null || same(table.schema, schema)));
  }

  function findTable(name: string, schema: string | null) {
    return catalog.tables.find((table) => same(table.name, name) && (schema == null || same(table.schema, schema)));
  }

  function knownSchema(name: string) {
    return catalog.tables.some((table) => table.schema && same(table.schema, name));
  }

  function columnsOf(schema: string | null, name: string) {
    const names = catalog.columns(schema, name);
    if (!names) return null;
    return new Set(names.map((item) => lower(item)));
  }

  function refFor(name: string) {
    return refs.find((ref) => same(ref.alias, name) || same(ref.name, name)) ?? null;
  }

  function namePrefix(word: string) {
    const value = lower(word);
    if (catalog.tables.some((table) => lower(table.name).startsWith(value) && lower(table.name).length > value.length)) {
      return true;
    }
    for (const ref of refs) {
      const set = columnsOf(ref.schema, ref.name);
      if (set && [...set].some((item) => item.startsWith(value) && item.length > value.length)) return true;
    }
    return false;
  }

  function columnRole(name: string): SqlRole | null {
    if (!refs.length) return null;
    const value = lower(name);
    let pending = false;
    for (const ref of refs) {
      const set = columnsOf(ref.schema, ref.name);
      if (!set) {
        pending = true;
        continue;
      }
      if (set.has(value)) return "field";
      if (typingCurrent && [...set].some((item) => item.startsWith(value) && item.length > value.length)) return "field";
    }
    if (pending) return null;
    return "error";
  }

  let typingCurrent = false;

  function openParen() {
    depth += 1;
    if (mode === "maybe-alias" || mode === "table") {
      stack.push("clause");
      mode = "column";
      return;
    }
    stack.push(mode === "qualified" ? "column" : mode);
  }

  function closeParen() {
    depth = Math.max(0, depth - 1);
    mode = stack.pop() ?? "clause";
    qualifier = null;
  }

  for (let index = 0; index < part.length; index += 1) {
    const token = part[index];
    if (token.kind === "paren") {
      if (token.text === "(") openParen();
      else closeParen();
      continue;
    }
    if (token.kind === "comma") {
      qualifier = null;
      if (mode === "maybe-alias" || mode === "table" || mode === "table-alias" || mode === "loose") mode = "table";
      else if (mode === "col-alias" || mode === "column" || mode === "type") mode = "column";
      continue;
    }
    if (token.kind === "dot") {
      mode = "qualified";
      continue;
    }
    if (token.kind !== "word") continue;

    const text = token.text;
    const value = lower(text);
    const nextIsDot = part[index + 1]?.kind === "dot";
    typingCurrent = typing(token);

    if (mode === "qualified" && qualifier) {
      if (qualifier.kind === "schema") {
        const found = findTable(text, qualifier.schema);
        if (found || systemSchema(qualifier.name)) {
          add(token, "table");
          refs.push({ schema: qualifier.schema, name: found?.name ?? text, alias: null });
          if (found) remember(found.schema ?? null, found.name);
        } else if (tableMustExist && hasCatalog && !typingCurrent) add(token, "error");
        else {
          refs.push({ schema: qualifier.schema, name: text, alias: null });
          remember(qualifier.schema, text);
        }
        mode = "maybe-alias";
      } else {
        const set = columnsOf(qualifier.schema, qualifier.name);
        if (set?.has(value) || (typingCurrent && set && [...set].some((item) => item.startsWith(value) && item.length > value.length))) {
          add(token, "field");
        } else if (set && !typingCurrent) add(token, "error");
        else if (!set && nearKeyword(text) && !typingCurrent) add(token, "error");
        mode = "column";
      }
      qualifier = null;
      continue;
    }

    if (mode === "type") {
      mode = "column";
      continue;
    }
    if (mode === "table-alias") {
      add(token, "table");
      const ref = refs[refs.length - 1];
      if (ref) ref.alias = text;
      mode = "loose";
      continue;
    }
    if (mode === "col-alias") {
      add(token, "field");
      mode = "column";
      continue;
    }

    if (!token.quoted && isSqlKeyword(text)) {
      add(token, "clause");
      if (value === "as" && mode === "maybe-alias") {
        mode = "table-alias";
        continue;
      }
      if (value === "as" && depth > 0) {
        mode = "type";
        continue;
      }
      const next = keywordNext(text);
      if (next === "table") tableMustExist = value !== "table";
      mode = next === "alias" ? "col-alias" : (next ?? "clause");
      continue;
    }

    if (mode === "maybe-alias") {
      add(token, "table");
      const ref = refs[refs.length - 1];
      if (ref) ref.alias = text;
      mode = "loose";
      continue;
    }

    if (!token.quoted && typingCurrent && keywordPrefix(text) && !namePrefix(text) && mode !== "table") {
      add(token, "clause");
      continue;
    }

    if (nextIsDot) {
      const ref = refFor(text);
      if (knownSchema(text) && !knownTable(text, null)) {
        add(token, "table");
        qualifier = { kind: "schema", schema: text, name: text };
      } else if (ref || knownTable(text, null)) {
        add(token, "table");
        const found = ref ?? findTable(text, null);
        qualifier = {
          kind: "table",
          schema: found && "schema" in found ? (found.schema ?? null) : null,
          name: found && "name" in found ? found.name : text,
        };
        if (!ref && found && "name" in found) remember(found.schema ?? null, found.name);
      } else if (hasCatalog && tableMustExist && !systemSchema(text) && !typingCurrent) {
        add(token, "error");
        qualifier = { kind: "schema", schema: text, name: text };
      } else {
        qualifier = { kind: "table", schema: null, name: text };
        remember(null, text);
      }
      continue;
    }

    if (mode === "table") {
      const found = findTable(text, null);
      if (found || knownSchema(text)) {
        add(token, "table");
        if (found) {
          refs.push({ schema: found.schema ?? null, name: found.name, alias: null });
          remember(found.schema ?? null, found.name);
        }
      } else if (typingCurrent && namePrefix(text)) add(token, "table");
      else if (tableMustExist && hasCatalog) add(token, "error");
      else if (tableMustExist) {
        refs.push({ schema: null, name: text, alias: null });
        remember(null, text);
      }
      mode = "maybe-alias";
      continue;
    }

    if (mode === "column") {
      const ref = refFor(text);
      const role = columnRole(text);
      if (role === "field") add(token, "field");
      else if (ref || knownTable(text, null)) add(token, "table");
      else if (role === "error") add(token, "error");
      else if (!token.quoted && nearKeyword(text) && !(typingCurrent && keywordPrefix(text))) add(token, "error");
      continue;
    }

    if (!token.quoted && !(typingCurrent && (keywordPrefix(text) || namePrefix(text)))) add(token, "error");
  }
}
