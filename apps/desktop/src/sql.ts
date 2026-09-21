import type { TableColumn, TableOrView } from "./ipc";

export type QuoteIdent = (name: string) => string;

export function qualifyTable(quote: QuoteIdent, table: TableOrView) {
  return table.schema ? `${quote(table.schema)}.${quote(table.name)}` : quote(table.name);
}

export function selectTopSql(quote: QuoteIdent, table: TableOrView, limit = 100) {
  return `SELECT * FROM ${qualifyTable(quote, table)} LIMIT ${limit}`;
}

export function createTableSql(quote: QuoteIdent, table: TableOrView, columns: TableColumn[]) {
  const body = columns
    .map((column) => {
      const nulls = column.nullable ? "" : " NOT NULL";
      return `  ${quote(column.columnName)} ${column.dataType}${nulls}`;
    })
    .join(",\n");
  return `CREATE TABLE ${qualifyTable(quote, table)} (\n${body}\n);`;
}

export function dropSql(quote: QuoteIdent, table: TableOrView) {
  const kind = table.entityType === "view" || table.entityType === "materialized-view" ? "VIEW" : "TABLE";
  return `DROP ${kind} IF EXISTS ${qualifyTable(quote, table)}`;
}

export function truncateSql(connectionType: string, quote: QuoteIdent, table: TableOrView) {
  const ident = qualifyTable(quote, table);
  if (connectionType === "sqlite") return `DELETE FROM ${ident}`;
  return `TRUNCATE TABLE ${ident}`;
}

export function renameSql(
  connectionType: string,
  quote: QuoteIdent,
  table: TableOrView,
  newName: string,
) {
  const ident = qualifyTable(quote, table);
  const next = quote(newName);
  if (connectionType === "mysql" || connectionType === "mariadb") {
    const dest = table.schema ? `${quote(table.schema)}.${next}` : next;
    return `RENAME TABLE ${ident} TO ${dest}`;
  }
  return `ALTER TABLE ${ident} RENAME TO ${next}`;
}

export function duplicateSql(quote: QuoteIdent, table: TableOrView, newName: string) {
  const dest: TableOrView = { ...table, name: newName };
  return `CREATE TABLE ${qualifyTable(quote, dest)} AS SELECT * FROM ${qualifyTable(quote, table)}`;
}

export function dropSchemaSql(quote: QuoteIdent, schema: string) {
  return `DROP SCHEMA IF EXISTS ${quote(schema)} CASCADE`;
}

export function renameSchemaSql(quote: QuoteIdent, schema: string, newName: string) {
  return `ALTER SCHEMA ${quote(schema)} RENAME TO ${quote(newName)}`;
}

export function viewCreateSql(connectionType: string, quote: QuoteIdent, table: TableOrView) {
  const name = table.name.replace(/'/g, "''");
  const schema = (table.schema ?? "public").replace(/'/g, "''");
  if (connectionType === "sqlite") {
    return `SELECT sql FROM sqlite_master WHERE type IN ('view', 'table') AND name = '${name}'`;
  }
  if (connectionType === "mysql" || connectionType === "mariadb") {
    return `SHOW CREATE VIEW ${qualifyTable(quote, table)}`;
  }
  if (connectionType === "sqlserver") {
    return `SELECT OBJECT_DEFINITION(OBJECT_ID('${schema}.${name}')) AS definition`;
  }
  return `SELECT definition FROM pg_views WHERE schemaname = '${schema}' AND viewname = '${name}'`;
}

export function firstCell(result: { rows?: Record<string, unknown>[] } | undefined) {
  const row = result?.rows?.[0];
  if (!row) return "";
  const values = Object.values(row)
    .filter((value) => value != null)
    .map((value) => String(value));
  return values.sort((left, right) => right.length - left.length)[0] ?? "";
}

export interface ColumnEdit {
  originalName: string | null;
  name: string;
  dataType: string;
  nullable: boolean;
  originalType: string;
  originalNullable: boolean;
  dropped: boolean;
}

export interface StructureEdits {
  columns: ColumnEdit[];
  dropIndexes: string[];
  dropTriggers: string[];
}

export function structureChangeSql(
  connectionType: string,
  quote: QuoteIdent,
  table: TableOrView,
  edits: StructureEdits,
) {
  const ident = qualifyTable(quote, table);
  const mysql = connectionType === "mysql" || connectionType === "mariadb";
  const sqlserver = connectionType === "sqlserver";
  const sqlite = connectionType === "sqlite";
  const statements: string[] = [];

  for (const column of edits.columns) {
    const name = column.name.trim();
    if (column.dropped) {
      if (column.originalName) statements.push(`ALTER TABLE ${ident} DROP COLUMN ${quote(column.originalName)}`);
      continue;
    }
    if (!column.originalName) {
      if (!name) continue;
      const nullSql = column.nullable ? "NULL" : "NOT NULL";
      statements.push(`ALTER TABLE ${ident} ADD COLUMN ${quote(name)} ${column.dataType.trim() || "text"} ${nullSql}`);
      continue;
    }
    let current = column.originalName;
    if (name && name !== column.originalName) {
      if (sqlserver) {
        const schema = (table.schema ?? "dbo").replace(/'/g, "''");
        const from = `${schema}.${table.name}.${column.originalName}`.replace(/'/g, "''");
        statements.push(`EXEC sp_rename '${from}', '${name.replace(/'/g, "''")}', 'COLUMN'`);
      } else {
        statements.push(`ALTER TABLE ${ident} RENAME COLUMN ${quote(column.originalName)} TO ${quote(name)}`);
      }
      current = name;
    }
    const type = column.dataType.trim();
    const typeChanged = type !== column.originalType;
    const nullChanged = column.nullable !== column.originalNullable;
    if (mysql && (typeChanged || nullChanged)) {
      statements.push(
        `ALTER TABLE ${ident} MODIFY COLUMN ${quote(current)} ${type || column.originalType} ${column.nullable ? "NULL" : "NOT NULL"}`,
      );
      continue;
    }
    if (typeChanged && !sqlite) {
      if (sqlserver) {
        statements.push(
          `ALTER TABLE ${ident} ALTER COLUMN ${quote(current)} ${type} ${column.nullable ? "NULL" : "NOT NULL"}`,
        );
      } else {
        statements.push(`ALTER TABLE ${ident} ALTER COLUMN ${quote(current)} TYPE ${type} USING ${quote(current)}::${type}`);
      }
    }
    if (nullChanged && !sqlite && !(sqlserver && typeChanged)) {
      if (sqlserver) {
        statements.push(
          `ALTER TABLE ${ident} ALTER COLUMN ${quote(current)} ${type || column.originalType} ${column.nullable ? "NULL" : "NOT NULL"}`,
        );
      } else if (column.nullable) {
        statements.push(`ALTER TABLE ${ident} ALTER COLUMN ${quote(current)} DROP NOT NULL`);
      } else {
        statements.push(`ALTER TABLE ${ident} ALTER COLUMN ${quote(current)} SET NOT NULL`);
      }
    }
  }

  for (const indexName of edits.dropIndexes) {
    if (mysql) statements.push(`ALTER TABLE ${ident} DROP INDEX ${quote(indexName)}`);
    else if (sqlserver) statements.push(`DROP INDEX ${quote(indexName)} ON ${ident}`);
    else if (sqlite) statements.push(`DROP INDEX ${quote(indexName)}`);
    else statements.push(`DROP INDEX ${table.schema ? `${quote(table.schema)}.${quote(indexName)}` : quote(indexName)}`);
  }

  for (const triggerName of edits.dropTriggers) {
    if (mysql || sqlite) statements.push(`DROP TRIGGER ${quote(triggerName)}`);
    else statements.push(`DROP TRIGGER ${quote(triggerName)} ON ${ident}`);
  }

  return statements;
}
