import type { MenuOption } from "./components/ContextMenu.vue";

const divider: MenuOption = { divider: true };

export function tableMenuOptions(readOnly: boolean, isView: boolean): MenuOption[] {
  return [
    { name: "View Data", slug: "view-data" },
    { name: "View Structure", slug: "view-structure" },
    { name: "Export To File", slug: "export" },
    {
      name: "Import from File",
      slug: "import",
      disabled: readOnly || isView,
      title: isView ? "You can only import data into a table" : readOnly ? "Read-only mode is enabled" : "",
    },
    divider,
    { name: "Copy Name", slug: "copy-name" },
    { name: "Hide", slug: "hide-entity" },
    divider,
    { name: "SQL: Create", slug: "sql-create" },
    { name: "SQL: Select Top", slug: "select-top" },
    {
      name: "Rename",
      slug: "rename",
      disabled: readOnly,
      title: readOnly ? "Read-only mode is enabled" : "",
    },
    {
      name: "Drop",
      slug: "sql-drop",
      disabled: readOnly,
      title: readOnly ? "Read-only mode is enabled" : "",
    },
    {
      name: "Truncate",
      slug: "sql-truncate",
      disabled: readOnly || isView,
      title: isView ? "Views cannot be truncated" : readOnly ? "Read-only mode is enabled" : "",
    },
    {
      name: "Duplicate",
      slug: "sql-duplicate",
      disabled: readOnly || isView,
      title: isView ? "Views cannot be duplicated" : readOnly ? "Read-only mode is enabled" : "",
    },
  ];
}

export function schemaMenuOptions(readOnly: boolean): MenuOption[] {
  return [
    { name: "Hide", slug: "hide-schema" },
    divider,
    {
      name: "Rename",
      slug: "rename-schema",
      disabled: readOnly,
      title: readOnly ? "Read-only mode is enabled" : "",
    },
    {
      name: "Drop",
      slug: "drop-schema",
      disabled: readOnly,
      title: readOnly ? "Read-only mode is enabled" : "",
    },
  ];
}

export const columnMenuOptions: MenuOption[] = [{ name: "Copy Name", slug: "copy-column" }];
