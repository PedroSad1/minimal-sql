import type { Completion, CompletionSource } from "@codemirror/autocomplete";
import {
  MariaSQL,
  MSSQL,
  MySQL,
  PostgreSQL,
  SQLite,
  StandardSQL,
  type SQLDialect,
} from "@codemirror/lang-sql";
import type { TableOrView } from "./ipc";
import { sqlContext, type ScanOptions, type TableRef } from "./sql-context";

export interface ColumnInfo {
  name: string;
  dataType: string;
}

const CLAUSES = [
  "SELECT",
  "FROM",
  "WHERE",
  "JOIN",
  "LEFT JOIN",
  "RIGHT JOIN",
  "INNER JOIN",
  "FULL JOIN",
  "CROSS JOIN",
  "ON",
  "AND",
  "OR",
  "NOT",
  "GROUP BY",
  "ORDER BY",
  "BY",
  "HAVING",
  "LIMIT",
  "OFFSET",
  "AS",
  "DISTINCT",
  "UNION",
  "UNION ALL",
  "WITH",
  "INSERT INTO",
  "VALUES",
  "UPDATE",
  "SET",
  "DELETE FROM",
  "RETURNING",
  "EXISTS",
  "IN",
  "IS NULL",
  "IS NOT NULL",
  "LIKE",
  "BETWEEN",
  "CASE",
  "WHEN",
  "THEN",
  "ELSE",
  "END",
  "ASC",
  "DESC",
  "NULL",
  "TRUE",
  "FALSE",
  "COUNT",
  "SUM",
  "AVG",
  "MIN",
  "MAX",
  "COALESCE",
  "CAST",
];

const IDENT = /^[\p{L}_][\p{L}\p{N}_$]*$/u;

export function dialectFor(connectionType: string) {
  if (connectionType === "mysql" || connectionType === "tidb" || connectionType === "starrocks") return MySQL;
  if (connectionType === "mariadb") return MariaSQL;
  if (connectionType === "sqlserver") return MSSQL;
  if (connectionType === "sqlite") return SQLite;
  if (
    connectionType === "postgresql" ||
    connectionType === "cockroachdb" ||
    connectionType === "redshift" ||
    connectionType === "greengage"
  ) {
    return PostgreSQL;
  }
  return StandardSQL;
}

export function postgresFamily(connectionType: string) {
  return (
    connectionType === "postgresql" ||
    connectionType === "cockroachdb" ||
    connectionType === "redshift" ||
    connectionType === "greengage"
  );
}

function scanFor(dialect: SQLDialect): ScanOptions {
  return {
    identQuotes: dialect.spec.identifierQuotes || '"',
    doubleQuoteIsString: Boolean(dialect.spec.doubleQuotedStrings),
    hashComments: Boolean(dialect.spec.hashComments),
  };
}

function identMark(dialect: SQLDialect) {
  const quotes = dialect.spec.identifierQuotes || '"';
  if (quotes.includes("[") && quotes.includes('"') && quotes.indexOf("[") < quotes.indexOf('"')) return "[";
  if (quotes.includes('"')) return '"';
  return quotes[0] || '"';
}

function quoteIdent(name: string, mark: string) {
  if (IDENT.test(name) && name === name.toLowerCase()) return name;
  const close = mark === "[" ? "]" : mark;
  return mark + name.split(close).join(close + close) + close;
}

function same(left: string | null | undefined, right: string | null | undefined) {
  return (left ?? "").toLowerCase() === (right ?? "").toLowerCase();
}

function hiddenSchema(schema: string | null | undefined) {
  if (!schema) return false;
  const name = schema.toLowerCase();
  return (
    name === "pg_catalog" ||
    name === "information_schema" ||
    name === "performance_schema" ||
    name === "mysql" ||
    name === "sys" ||
    name.startsWith("pg_toast") ||
    name.startsWith("pg_temp")
  );
}

function section(name: string, rank: number) {
  return { name, rank };
}

export function createSqlCompletion(input: {
  getConnectionType: () => string;
  getEntities: () => TableOrView[];
  loadColumns: (table: string, schema: string | null) => Promise<ColumnInfo[]>;
}): CompletionSource {
  const cache = new Map<string, Promise<ColumnInfo[]>>();

  function ensure(table: string, schema: string | null) {
    const key = `${(schema ?? "").toLowerCase()}\0${table.toLowerCase()}`;
    const cached = cache.get(key);
    if (cached) return cached;
    const pending = input.loadColumns(table, schema).catch(() => {
      cache.delete(key);
      return [] as ColumnInfo[];
    });
    cache.set(key, pending);
    return pending;
  }

  function entities() {
    return input.getEntities().filter((entity) => !hiddenSchema(entity.schema));
  }

  function clauses(rank: number, postgres: boolean): Completion[] {
    const labels = postgres ? [...CLAUSES, "ILIKE"] : CLAUSES;
    return labels.map((label) => ({
      label,
      apply: `${label} `,
      type: "keyword",
      boost: 0,
      section: section("Cláusulas", rank),
    }));
  }

  function tables(list: TableOrView[], rank: number, mark: string, withSchemas: boolean): Completion[] {
    const counts = new Map<string, number>();
    for (const entity of list) {
      const key = entity.name.toLowerCase();
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    const schemas = [...new Set(list.map((entity) => entity.schema || "").filter(Boolean))];
    const options: Completion[] = list.map((entity) => {
      const duplicate = (counts.get(entity.name.toLowerCase()) ?? 0) > 1;
      const quoted = quoteIdent(entity.name, mark);
      const schema = entity.schema || "";
      const view = entity.entityType === "view" || entity.entityType === "materialized-view";
      return {
        label: duplicate && schema ? `${schema}.${entity.name}` : entity.name,
        apply: duplicate && schema ? `${quoteIdent(schema, mark)}.${quoted}` : quoted,
        type: "type",
        detail: view ? "view" : schemas.length > 1 ? schema : undefined,
        boost: 8,
        section: section("Tabelas", rank),
      };
    });
    if (withSchemas && schemas.length > 1) {
      for (const schema of schemas) {
        options.push({
          label: schema,
          apply: `${quoteIdent(schema, mark)}.`,
          type: "namespace",
          detail: "schema",
          boost: 4,
          section: section("Tabelas", rank),
        });
      }
    }
    return options;
  }

  function matches(list: TableOrView[], ref: TableRef) {
    return list.filter((entity) => same(entity.name, ref.name) && (ref.schema == null || same(entity.schema, ref.schema)));
  }

  async function columns(refs: TableRef[], list: TableOrView[], rank: number, mark: string) {
    const seen = new Set<string>();
    const targets: TableRef[] = [];
    for (const ref of refs) {
      const key = `${(ref.schema ?? "").toLowerCase()}\0${ref.name.toLowerCase()}\0${(ref.alias ?? "").toLowerCase()}`;
      if (seen.has(key)) continue;
      seen.add(key);
      targets.push(ref);
    }
    const loaded = await Promise.all(
      targets.map(async (ref) => {
        const found = matches(list, ref);
        const source = found.length ? found : [{ name: ref.name, schema: ref.schema, entityType: "table" }];
        const groups = await Promise.all(
          source.map(async (entity) => ({
            label: ref.alias || entity.name,
            columns: await ensure(entity.name, entity.schema ?? null),
          })),
        );
        return groups;
      }),
    );
    const flat = loaded.flat();
    const many = flat.length > 1;
    const options: Completion[] = [];
    for (const group of flat) {
      for (const column of group.columns) {
        options.push({
          label: column.name,
          apply: quoteIdent(column.name, mark),
          type: "property",
          detail: many ? `${group.label} · ${column.dataType}` : column.dataType,
          boost: 12,
          section: section("Campos", rank),
        });
      }
    }
    return options;
  }

  function aliases(refs: TableRef[], rank: number, mark: string): Completion[] {
    const options: Completion[] = [];
    for (const ref of refs) {
      if (!ref.alias || same(ref.alias, ref.name)) continue;
      options.push({
        label: ref.alias,
        apply: quoteIdent(ref.alias, mark),
        type: "constant",
        detail: ref.schema ? `${ref.schema}.${ref.name}` : ref.name,
        boost: 6,
        section: section("Tabelas", rank),
      });
    }
    return options;
  }

  return async (context) => {
    const connectionType = input.getConnectionType() || "postgresql";
    const dialect = dialectFor(connectionType);
    const mark = identMark(dialect);
    const ctx = sqlContext(context.state.doc.toString(), context.pos, scanFor(dialect));
    if (ctx.kind === "skip") return null;
    const list = entities();
    const validFor = /^[\p{L}\p{N}_$]*$/u;

    if (ctx.parents.length) {
      const options = await qualified(ctx.parents, ctx.refs, input.getEntities(), mark, ctx.kind);
      if (context.aborted || !options.length) return null;
      return { from: ctx.from, options, validFor };
    }

    const typed = ctx.partial.length > 0;
    if (ctx.kind === "clause" && !context.explicit && !typed) return null;
    if (!context.explicit && !typed && ctx.kind !== "column" && ctx.kind !== "table") return null;

    const postgres = postgresFamily(connectionType);
    const options: Completion[] = [];
    if (ctx.kind === "table") options.push(...tables(list, 0, mark, true));
    if (ctx.kind === "column") {
      options.push(...aliases(ctx.refs, 1, mark));
      if (ctx.refs.length) options.push(...(await columns(ctx.refs, list, 0, mark)));
      options.push(...tables(list, 1, mark, true));
    }
    if (context.aborted) return null;
    const clauseRank = ctx.kind === "column" ? 2 : ctx.kind === "table" ? 1 : 0;
    options.push(...clauses(clauseRank, postgres));
    if (!options.length) return null;
    return { from: ctx.from, options, validFor };
  };

  async function qualified(
    parents: string[],
    refs: TableRef[],
    list: TableOrView[],
    mark: string,
    kind: "column" | "table" | "clause" | "skip",
  ) {
    if (parents.length >= 2) {
      const schema = parents[parents.length - 2];
      const name = parents[parents.length - 1];
      return columns([{ schema, name, alias: null, depth: 0 }], list, 0, mark);
    }
    const head = parents[0];
    const alias = refs.find((ref) => same(ref.alias, head));
    if (alias) return columns([alias], list, 0, mark);
    const named = list.filter((entity) => same(entity.name, head));
    const schemas = [...new Set(list.map((entity) => entity.schema || "").filter((schema) => same(schema, head)))];
    if (named.length && !schemas.length) {
      return columns(
        named.map((entity) => ({ schema: entity.schema ?? null, name: entity.name, alias: null, depth: 0 })),
        list,
        0,
        mark,
      );
    }
    if (schemas.length && !named.length) return tables(list.filter((entity) => same(entity.schema, head)), 0, mark, false);
    if (named.length && schemas.length) {
      if (kind === "table") return tables(list.filter((entity) => same(entity.schema, head)), 0, mark, false);
      return columns(
        named.map((entity) => ({ schema: entity.schema ?? null, name: entity.name, alias: null, depth: 0 })),
        list,
        0,
        mark,
      );
    }
    return columns([{ schema: null, name: head, alias: null, depth: 0 }], list, 0, mark);
  }
}
