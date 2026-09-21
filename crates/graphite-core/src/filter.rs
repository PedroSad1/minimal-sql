use serde::{Deserialize, Serialize};

use crate::types::Dialect;
use crate::error::{GraphiteError, Result};
use crate::types::validate_ident;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableFilter {
    pub field: String,
    pub op: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub join: String,
}

#[derive(Debug, Clone)]
pub struct WhereClause {
    pub sql: String,
    pub params: Vec<serde_json::Value>,
}

pub fn build_where(dialect: Dialect, filters: &[TableFilter]) -> Result<WhereClause> {
    if filters.is_empty() {
        return Ok(WhereClause {
            sql: String::new(),
            params: vec![],
        });
    }
    let mut parts = Vec::new();
    let mut joins = Vec::new();
    let mut params = Vec::new();
    for filter in filters {
        validate_ident(&filter.field)?;
        let column = dialect.quote_ident(&filter.field)?;
        let op = normalize_op(&filter.op)?;
        if matches!(op, "is null" | "is not null") {
            parts.push(format!("{column} {op}"));
            joins.push(filter.join.clone());
            continue;
        }
        if matches!(op, "in" | "not in") {
            let values = match &filter.value {
                serde_json::Value::Array(items) => items.clone(),
                other => vec![other.clone()],
            };
            if values.is_empty() {
                parts.push(if op == "in" {
                    "1=0".into()
                } else {
                    "1=1".into()
                });
                joins.push(filter.join.clone());
                continue;
            }
            let mut placeholders = Vec::new();
            for value in values {
                placeholders.push(dialect.placeholder(params.len()));
                params.push(value);
            }
            parts.push(format!("{column} {op} ({})", placeholders.join(", ")));
            joins.push(filter.join.clone());
            continue;
        }
        let placeholder = dialect.placeholder(params.len());
        parts.push(format!("{column} {op} {placeholder}"));
        joins.push(filter.join.clone());
        params.push(filter.value.clone());
    }
    let mut sql = parts[0].clone();
    for (index, part) in parts.iter().enumerate().skip(1) {
        let word = if joins[index].trim().eq_ignore_ascii_case("or") {
            "OR"
        } else {
            "AND"
        };
        sql = format!("({sql}) {word} ({part})");
    }
    Ok(WhereClause {
        sql: format!(" WHERE {sql}"),
        params,
    })
}

fn normalize_op(op: &str) -> Result<&'static str> {
    Ok(match op.trim().to_lowercase().as_str() {
        "=" | "eq" | "equals" => "=",
        "!=" | "<>" | "neq" | "does not equal" => "<>",
        ">" => ">",
        ">=" => ">=",
        "<" => "<",
        "<=" => "<=",
        "like" => "LIKE",
        "not like" => "NOT LIKE",
        "ilike" => "ILIKE",
        "not ilike" => "NOT ILIKE",
        "in" => "in",
        "not in" => "not in",
        "is null" | "null" => "is null",
        "is not null" | "not null" => "is not null",
        other => {
            return Err(GraphiteError::msg(format!(
                "unsupported filter operator: {other}"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unlimited_filters_and_together() {
        let filters = vec![
            TableFilter {
                field: "status".into(),
                op: "=".into(),
                value: json!("open"),
                join: String::new(),
            },
            TableFilter {
                field: "amount".into(),
                op: ">".into(),
                value: json!(10),
                join: String::new(),
            },
            TableFilter {
                field: "name".into(),
                op: "like".into(),
                value: json!("%a%"),
                join: String::new(),
            },
        ];
        let clause = build_where(Dialect::Sqlite, &filters).unwrap();
        assert!(clause.sql.contains("AND"));
        assert_eq!(clause.params.len(), 3);
        assert!(clause.sql.starts_with(" WHERE "));
    }

    #[test]
    fn rejects_bad_identifier() {
        let filters = vec![TableFilter {
            field: "id;drop".into(),
            op: "=".into(),
            value: json!(1),
            join: String::new(),
        }];
        assert!(build_where(Dialect::Postgres, &filters).is_err());
    }

    #[test]
    fn or_joins_left_to_right() {
        let filters = vec![
            TableFilter {
                field: "status".into(),
                op: "=".into(),
                value: json!("open"),
                join: String::new(),
            },
            TableFilter {
                field: "status".into(),
                op: "=".into(),
                value: json!("paid"),
                join: "or".into(),
            },
        ];
        let clause = build_where(Dialect::Sqlite, &filters).unwrap();
        assert_eq!(
            clause.sql,
            " WHERE (\"status\" = ?) OR (\"status\" = ?)"
        );
        assert_eq!(clause.params, vec![json!("open"), json!("paid")]);
    }
}
