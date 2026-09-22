import assert from "node:assert/strict";
import test from "node:test";
import { sqlContext } from "./sql-context.ts";

function at(sql: string) {
  const pos = sql.indexOf("|");
  assert.notEqual(pos, -1);
  return sqlContext(sql.replace("|", ""), pos);
}

test("select list uses tables written after the cursor", () => {
  const ctx = at("SELECT | FROM users");
  assert.equal(ctx.kind, "column");
  assert.deepEqual(ctx.refs.map((ref) => ref.name), ["users"]);
});

test("from offers tables", () => {
  const ctx = at("SELECT id FROM |");
  assert.equal(ctx.kind, "table");
  assert.equal(ctx.refs.length, 0);
  assert.equal(at("SELECT id FROM us|").partial, "us");
});

test("alias dot points at the alias", () => {
  const ctx = at("SELECT u.| FROM users u");
  assert.deepEqual(ctx.parents, ["u"]);
  assert.equal(ctx.kind, "column");
  assert.equal(ctx.refs[0]?.alias, "u");
});

test("schema dot stays on the schema name", () => {
  const ctx = at("SELECT * FROM public.|");
  assert.deepEqual(ctx.parents, ["public"]);
  assert.equal(ctx.kind, "table");
});

test("where uses every joined table", () => {
  const ctx = at("SELECT * FROM users u JOIN orders o ON |");
  assert.equal(ctx.kind, "column");
  assert.deepEqual(
    ctx.refs.map((ref) => [ref.name, ref.alias]),
    [
      ["users", "u"],
      ["orders", "o"],
    ],
  );
});

test("qualified column keeps both parents", () => {
  const ctx = at("SELECT * FROM users u JOIN orders o ON u.|");
  assert.deepEqual(ctx.parents, ["u"]);
  assert.equal(ctx.partial, "");
});

test("strings and comments do not complete", () => {
  assert.equal(at("SELECT 'from |'").kind, "skip");
  assert.equal(at("SELECT 1 -- from |\n").kind, "skip");
});

test("a second statement ignores the first", () => {
  const ctx = at("SELECT * FROM users; SELECT |");
  assert.equal(ctx.kind, "column");
  assert.equal(ctx.refs.length, 0);
});

test("comma separated tables and update set", () => {
  assert.deepEqual(at("SELECT * FROM users, orders WHERE |").refs.map((ref) => ref.name), ["users", "orders"]);
  const update = at("UPDATE users SET |");
  assert.equal(update.kind, "column");
  assert.deepEqual(update.refs.map((ref) => ref.name), ["users"]);
});

test("insert column list uses the target table", () => {
  const ctx = at("INSERT INTO users (|)");
  assert.equal(ctx.kind, "column");
  assert.deepEqual(ctx.refs.map((ref) => ref.name), ["users"]);
});

test("subquery does not borrow the outer table", () => {
  const ctx = at("SELECT * FROM users WHERE id IN (SELECT | FROM orders)");
  assert.equal(ctx.kind, "column");
  assert.deepEqual(ctx.refs.map((ref) => ref.name), ["orders"]);
});

test("typed prefix starts at the word", () => {
  const ctx = at("SELECT na| FROM users");
  assert.equal(ctx.partial, "na");
  assert.equal(ctx.kind, "column");
});

test("schema and table before a column", () => {
  const ctx = at("SELECT public.users.| FROM public.users");
  assert.deepEqual(ctx.parents, ["public", "users"]);
});
