use crate::{Profile, error};
use db_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct TableRequest {
    pub object: ObjectId,
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
    pub filter_column: Option<String>,
    pub filter: Option<String>,
    pub sort_column: Option<String>,
    #[serde(default)]
    pub descending: bool,
}
fn default_limit() -> u64 {
    100
}
pub fn table_query(profile: &Profile, request: &TableRequest) -> DbResult<(String, Vec<String>)> {
    db_core::validate_metadata(
        &profile.config(String::new())?,
        &MetadataRequest::Inspect {
            object: request.object.clone(),
        },
    )?;
    if !matches!(
        request.object.kind,
        ObjectKind::Table | ObjectKind::View | ObjectKind::MaterializedView
    ) || request.limit == 0
        || request.limit > 1000
        || request.offset > 1_000_000_000
    {
        return Err(error(
            "INVALID_TABLE",
            "Choose a table/view and bounded page",
        ));
    }
    let quote = |value: &str| {
        if profile.engine == Engine::Postgres {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            format!("`{}`", value.replace('`', "``"))
        }
    };
    let namespace = if profile.engine == Engine::Postgres {
        request
            .object
            .schema
            .as_deref()
            .ok_or_else(|| error("MISSING_SCHEMA", "Schema required"))?
    } else {
        request.object.database.as_str()
    };
    let mut sql = format!(
        "SELECT * FROM {}.{}",
        quote(namespace),
        quote(&request.object.name)
    );
    let mut parameters = Vec::new();
    if let Some(column) = request.filter_column.as_deref().filter(|v| !v.is_empty())
        && let Some(value) = request.filter.as_deref().filter(|v| !v.is_empty())
    {
        if value.len() > 4096 {
            return Err(error("INVALID_FILTER", "Filter is limited to 4096 bytes"));
        }
        sql.push_str(&format!(
            " WHERE CAST({} AS {}) LIKE {} ESCAPE '!'",
            quote(column),
            if profile.engine == Engine::Postgres {
                "TEXT"
            } else {
                "CHAR"
            },
            if profile.engine == Engine::Postgres {
                "$1"
            } else {
                "?"
            }
        ));
        parameters.push(format!(
            "%{}%",
            value
                .replace('!', "!!")
                .replace('%', "!%")
                .replace('_', "!_")
        ));
    }
    if let Some(column) = request.sort_column.as_deref().filter(|v| !v.is_empty()) {
        sql.push_str(&format!(
            " ORDER BY {} {}",
            quote(column),
            if request.descending { "DESC" } else { "ASC" }
        ));
    }
    sql.push_str(&format!(
        " LIMIT {} OFFSET {}",
        request.limit, request.offset
    ));
    Ok((sql, parameters))
}
