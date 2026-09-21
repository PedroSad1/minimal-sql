export type CopyColumn = { field: string; title: string };
export type CopyBlock = { columns: CopyColumn[]; rows: Record<string, unknown>[] };
export type CopyKind = "plain" | "columnName" | "tsv" | "json" | "markdown" | "sql" | "in";

function raw(value: unknown) {
  if (value == null) return "";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function quoted(value: unknown) {
  return `"${raw(value).replace(/"/g, '""')}"`;
}

function quoteIdent(name: string) {
  return `"${name.replace(/"/g, '""')}"`;
}

function sqlLiteral(value: unknown) {
  if (value == null) return "NULL";
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
  return `'${raw(value).replace(/'/g, "''")}'`;
}

function cellCount(blocks: CopyBlock[]) {
  return blocks.reduce((sum, block) => sum + block.rows.length * block.columns.length, 0);
}

function plainBlock(block: CopyBlock) {
  return block.rows
    .map((row) => block.columns.map((column) => raw(row[column.field])).join("\t"))
    .join("\n");
}

function tsvBlock(block: CopyBlock) {
  return block.rows
    .map((row) => block.columns.map((column) => quoted(row[column.field])).join("\t"))
    .join("\n");
}

function jsonBlock(block: CopyBlock) {
  return block.rows.map((row) => {
    const item: Record<string, unknown> = {};
    for (const column of block.columns) item[column.title] = row[column.field] ?? null;
    return item;
  });
}

function markdownBlock(block: CopyBlock) {
  const head = block.columns.map((column) => column.title.replace(/\|/g, "\\|"));
  const line = head.map(() => "---");
  const body = block.rows.map((row) =>
    block.columns.map((column) => raw(row[column.field]).replace(/\|/g, "\\|").replace(/\r?\n/g, " ")),
  );
  return [head, line, ...body].map((cells) => `| ${cells.join(" | ")} |`).join("\n");
}

function sqlBlock(block: CopyBlock, table: string, schema?: string | null) {
  const target = schema ? `${quoteIdent(schema)}.${quoteIdent(table)}` : quoteIdent(table);
  const columns = block.columns.map((column) => quoteIdent(column.title)).join(", ");
  const values = block.rows
    .map((row) => `(${block.columns.map((column) => sqlLiteral(row[column.field])).join(", ")})`)
    .join(",\n");
  return `INSERT INTO ${target} (${columns}) VALUES\n${values};`;
}

function inBlock(block: CopyBlock) {
  const field = block.columns[0]?.field;
  if (!field) return "";
  const values = block.rows.map((row) => sqlLiteral(row[field]));
  return `(\n${values.join(",\n")}\n)`;
}

export function formatCopy(
  blocks: CopyBlock[],
  kind: CopyKind,
  target?: { table?: string; schema?: string | null },
) {
  if (!blocks.length) return "";
  if (kind === "columnName") {
    const names: string[] = [];
    for (const block of blocks) {
      for (const column of block.columns) {
        if (!names.includes(column.title)) names.push(column.title);
      }
    }
    return names.join(" ");
  }
  if (kind === "plain" && cellCount(blocks) === 1) {
    const block = blocks[0];
    const column = block.columns[0];
    const row = block.rows[0];
    if (!column || !row) return "";
    return raw(row[column.field]);
  }
  if (kind === "plain") return blocks.map(plainBlock).filter(Boolean).join("\n\n");
  if (kind === "tsv") return blocks.map(tsvBlock).filter(Boolean).join("\n\n");
  if (kind === "json") return JSON.stringify(blocks.flatMap(jsonBlock));
  if (kind === "markdown") return blocks.map(markdownBlock).filter(Boolean).join("\n\n");
  if (kind === "in") return blocks.map(inBlock).filter(Boolean).join("\n\n");
  if (!target?.table) return "";
  return blocks.map((block) => sqlBlock(block, target.table ?? "", target.schema)).join("\n");
}

export function hideLabel(title: string) {
  const label = `Hide ${title}`;
  if (label.length <= 33) return label;
  return `${label.slice(0, 30)}...`;
}
