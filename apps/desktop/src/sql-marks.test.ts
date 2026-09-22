import assert from "node:assert/strict";
import test from "node:test";
import { postgresScan } from "./sql-context.ts";
import { sqlMarks, type SqlCatalog, type SqlRole, type SqlTableName } from "./sql-marks.ts";

function catalog(tables: SqlTableName[], columns: Record<string, string[] | null> = {}): SqlCatalog {
  return {
    tables,
    columns(schema, table) {
      const key = `${(schema ?? "").toLowerCase()}.${table.toLowerCase()}`;
      if (!(key in columns)) return null;
      return columns[key];
    },
  };
}

function role(sql: string, word: string, source: SqlCatalog, occurrence = 0, cursor = -1): SqlRole | undefined {
  let from = -1;
  for (let index = 0; index <= occurrence; index += 1) from = sql.toLowerCase().indexOf(word.toLowerCase(), from + 1);
  const mark = sqlMarks(sql, postgresScan, source, cursor).marks.find((item) => item.from === from);
  return mark?.role;
}

const users = catalog(
  [
    { schema: "public", name: "users" },
    { schema: "public", name: "orders" },
  ],
  {
    "public.users": ["id", "name", "form"],
    "public.orders": ["id", "user_id"],
  },
);

test("colors clauses, fields, and tables", () => {
  const sql = "SELECT id FROM users";
  assert.equal(role(sql, "SELECT", users), "clause");
  assert.equal(role(sql, "id", users), "field");
  assert.equal(role(sql, "FROM", users), "clause");
  assert.equal(role(sql, "users", users), "table");
});

test("marks an unknown clause", () => {
  assert.equal(role("SELCT id FROM users", "SELCT", users), "error");
  assert.equal(role("SELECT id FORM orders", "FORM", users), "error");
});

test("keeps a real column named form", () => {
  assert.equal(role("SELECT form FROM users", "form", users), "field");
});

test("marks an unknown table only when the catalog is loaded", () => {
  assert.equal(role("SELECT id FROM usres", "usres", users), "error");
  assert.equal(role("SELECT id FROM usres", "usres", catalog([])), undefined);
});

test("colors alias, schema, and qualified field", () => {
  const sql = "SELECT u.id FROM public.users u";
  assert.equal(role(sql, "u", users), "table");
  assert.equal(role(sql, "id", users), "field");
  assert.equal(role(sql, "public", users), "table");
  assert.equal(role(sql, "users", users), "table");
});

test("does not mark a new table in CREATE TABLE", () => {
  const sql = "CREATE TABLE widgets (id int)";
  assert.equal(role(sql, "widgets", users), undefined);
  assert.equal(role(sql, "id", users), undefined);
  assert.equal(sqlMarks(sql, postgresScan, users).tables.length, 0);
});

test("colors a column alias and flags an unknown column", () => {
  const sql = "SELECT id AS label, nope FROM users";
  assert.equal(role(sql, "label", users), "field");
  assert.equal(role(sql, "nope", users), "error");
});

test("leaves a column plain until its table columns are loaded", () => {
  const pending = catalog([{ schema: "public", name: "users" }]);
  assert.equal(role("SELECT id FROM users", "id", pending), undefined);
  assert.equal(role("SELECT id FROM users", "users", pending), "table");
});

test("colors a clause prefix at the cursor", () => {
  const sql = "SELECT id FRO";
  assert.equal(role(sql, "FRO", users, 0, sql.length), "clause");
});
