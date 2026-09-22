import { RangeSetBuilder, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import type { TableOrView } from "./ipc";
import { dialectFor } from "./sql-complete";
import { sqlMarks, type SqlCatalog, type SqlRole, type SqlTableName } from "./sql-marks";
import type { ScanOptions } from "./sql-context";

export const sqlToneRefresh = StateEffect.define<null>();

const epoch = StateField.define<number>({
  create: () => 0,
  update(value, transaction) {
    let next = value;
    for (const effect of transaction.effects) if (effect.is(sqlToneRefresh)) next += 1;
    return next;
  },
});

const markFor: Record<SqlRole, Decoration> = {
  clause: Decoration.mark({ class: "sql-clause" }),
  table: Decoration.mark({ class: "sql-table" }),
  field: Decoration.mark({ class: "sql-field" }),
  error: Decoration.mark({ class: "sql-error" }),
};

function scanFor(connectionType: string): ScanOptions {
  const dialect = dialectFor(connectionType);
  return {
    identQuotes: dialect.spec.identifierQuotes || '"',
    doubleQuoteIsString: Boolean(dialect.spec.doubleQuotedStrings),
    hashComments: Boolean(dialect.spec.hashComments),
  };
}

export function sqlTone(input: {
  getConnectionType: () => string;
  getEntities: () => TableOrView[];
  loadColumns: (table: string, schema: string | null) => Promise<{ name: string }[] | null>;
}) {
  const cache = new Map<string, readonly string[] | "loading">();

  function cacheKey(schema: string | null, table: string) {
    return `${(schema ?? "").toLowerCase()}\0${table.toLowerCase()}`;
  }

  function catalog(): SqlCatalog {
    return {
      tables: input.getEntities().map((entity) => ({ schema: entity.schema ?? null, name: entity.name })),
      columns(schema, table) {
        const hit = cache.get(cacheKey(schema, table));
        if (!hit || hit === "loading") return null;
        return hit;
      },
    };
  }

  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      private dead = false;
      private tables: SqlTableName[] = [];

      constructor(private view: EditorView) {
        this.decorations = this.paint();
        this.fetch();
      }

      update(update: ViewUpdate) {
        const refreshed = update.state.field(epoch) !== update.startState.field(epoch);
        if (!update.docChanged && !update.selectionSet && !refreshed) return;
        this.decorations = this.paint();
        this.fetch();
      }

      destroy() {
        this.dead = true;
      }

      paint() {
        const cursor = this.view.state.selection.main.head;
        const painted = sqlMarks(
          this.view.state.doc.toString(),
          scanFor(input.getConnectionType() || "postgresql"),
          catalog(),
          cursor,
        );
        this.tables = painted.tables;
        const builder = new RangeSetBuilder<Decoration>();
        for (const mark of painted.marks) builder.add(mark.from, mark.to, markFor[mark.role]);
        return builder.finish();
      }

      fetch() {
        for (const table of this.tables) {
          const id = cacheKey(table.schema, table.name);
          if (cache.has(id)) continue;
          cache.set(id, "loading");
          input
            .loadColumns(table.name, table.schema)
            .then((columns) => {
              if (this.dead) return;
              if (!columns) {
                cache.delete(id);
                return;
              }
              cache.set(id, columns.map((column) => column.name));
              this.view.dispatch({ effects: sqlToneRefresh.of(null) });
            })
            .catch(() => {
              cache.delete(id);
            });
        }
      }
    },
    { decorations: (value) => value.decorations },
  );

  return [epoch, plugin];
}
