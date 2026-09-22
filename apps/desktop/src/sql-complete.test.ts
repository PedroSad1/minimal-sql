import { CompletionContext } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import assert from "node:assert/strict";
import test from "node:test";
import type { Completion } from "@codemirror/autocomplete";
import { createSqlCompletion } from "./sql-complete.ts";

const source = createSqlCompletion({
  getConnectionType: () => "postgresql",
  getEntities: () => [
    { name: "users", schema: "public", entityType: "table" },
    { name: "orders", schema: "public", entityType: "table" },
    { name: "accounts", schema: "billing", entityType: "table" },
    { name: "pg_class", schema: "pg_catalog", entityType: "table" },
  ],
  loadColumns: async (table) => {
    if (table === "users") {
      return [
        { name: "id", dataType: "uuid" },
        { name: "name", dataType: "text" },
      ];
    }
    if (table === "orders") return [{ name: "id", dataType: "uuid" }, { name: "user_id", dataType: "uuid" }];
    return [{ name: "id", dataType: "int" }];
  },
});

function at(sql: string, explicit = false) {
  const pos = sql.indexOf("|");
  const state = EditorState.create({ doc: sql.replace("|", "") });
  return new CompletionContext(state, pos, explicit);
}

async function labels(sql: string, explicit = false) {
  const result = await source(at(sql, explicit));
  assert.ok(result);
  return result.options.map((option: Completion) => option.label);
}

test("clauses appear at the start of a query", async () => {
  const options = await labels("S|", true);
  assert.equal(options.includes("SELECT"), true);
  assert.equal(options.includes("FROM"), true);
});

test("from lists tables and schemas, not catalog tables", async () => {
  const options = await labels("SELECT id FROM |");
  assert.equal(options.includes("users"), true);
  assert.equal(options.includes("orders"), true);
  assert.equal(options.includes("accounts"), true);
  assert.equal(options.includes("public"), true);
  assert.equal(options.includes("billing"), true);
  assert.equal(options.includes("pg_class"), false);
});

test("select lists fields of the table in the query", async () => {
  const options = await labels("SELECT | FROM users");
  assert.equal(options.includes("id"), true);
  assert.equal(options.includes("name"), true);
  assert.equal(options.includes("SELECT"), true);
});

test("a join offers fields from both tables", async () => {
  const options = await labels("SELECT | FROM users u JOIN orders o");
  assert.equal(options.includes("name"), true);
  assert.equal(options.includes("user_id"), true);
  assert.equal(options.includes("u"), true);
  assert.equal(options.includes("o"), true);
});

test("alias dot lists only that table", async () => {
  const options = await labels("SELECT u.| FROM users u JOIN orders o");
  assert.deepEqual(options, ["id", "name"]);
});

test("schema dot lists tables in that schema", async () => {
  const options = await labels("SELECT * FROM billing.|");
  assert.deepEqual(options, ["accounts"]);
});

test("table dot lists its fields", async () => {
  const options = await labels("SELECT users.| FROM users");
  assert.deepEqual(options, ["id", "name"]);
});
