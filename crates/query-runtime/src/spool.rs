use crate::{crypto::Cipher, error, private_directory};
use db_types::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use uuid::Uuid;

pub const PAGE_ROWS: usize = 256;
pub const PAGE_BYTES: usize = 2 * 1024 * 1024;
const RESULT_BYTES: u64 = 1024 * 1024 * 1024;
const TOTAL_BYTES: u64 = 4 * RESULT_BYTES;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub kind: String,
    pub value: Option<String>,
}
pub type WireRow = Vec<Cell>;
pub fn wire_row(row: &ResultRow) -> WireRow {
    use base64::Engine;
    row.iter().map(|value| {
        let (kind,value)=match value {
            DbValue::Null=>("null",None),DbValue::Boolean(v)=>("boolean",Some(v.to_string())),
            DbValue::Signed(v)=>("signed",Some(v.to_string())),DbValue::Unsigned(v)=>("unsigned",Some(v.to_string())),
            DbValue::Float(v)=>("float",Some(v.clone())),DbValue::Decimal(v)=>("decimal",Some(v.clone())),
            DbValue::Text(v)=>("text",Some(v.clone())),DbValue::Date(v)=>("date",Some(v.clone())),
            DbValue::Time(v)=>("time",Some(v.clone())),DbValue::Timestamp(v)=>("timestamp",Some(v.clone())),
            DbValue::Json(v)=>("json",Some(v.to_string())),
            DbValue::Binary(v)=>("binary",Some(base64::engine::general_purpose::STANDARD.encode(v))),
            DbValue::Opaque{database_type,format,payload}=>("opaque",Some(serde_json::json!({"database_type":database_type,"format":format,"base64":base64::engine::general_purpose::STANDARD.encode(payload)}).to_string())),
        };Cell{kind:kind.into(),value}
    }).collect()
}
struct Frame {
    offset: u64,
    length: usize,
    start: u64,
    count: usize,
}
pub struct Spool {
    file: Option<File>,
    path: PathBuf,
    cipher: Cipher,
    frames: Vec<Frame>,
    bytes: u64,
    rows: u64,
    total: Arc<AtomicU64>,
    additional: Vec<(Arc<AtomicU64>, u64)>,
    limit: u64,
    total_limit: u64,
}
fn io_error(_: std::io::Error) -> DbError {
    error(
        "SPOOL_IO_ERROR",
        "Result storage failed; available rows are partial",
    )
}
impl Spool {
    pub fn new(directory: &Path, id: Uuid, total: Arc<AtomicU64>) -> DbResult<Self> {
        Self::with_limits(directory, id, total, RESULT_BYTES, TOTAL_BYTES)
    }
    pub fn with_limits(
        directory: &Path,
        id: Uuid,
        total: Arc<AtomicU64>,
        limit: u64,
        total_limit: u64,
    ) -> DbResult<Self> {
        private_directory(directory)?;
        let path = directory.join(format!("{id}.spool"));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(Self {
            file: Some(options.open(&path).map_err(io_error)?),
            path,
            cipher: Cipher::random(),
            frames: Vec::new(),
            bytes: 0,
            rows: 0,
            total,
            additional: Vec::new(),
            limit,
            total_limit,
        })
    }
    pub fn with_budgets(mut self, budgets: Vec<(Arc<AtomicU64>, u64)>) -> Self {
        self.additional = budgets;
        self
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    fn release(&self, size: u64) {
        self.total.fetch_sub(size, Ordering::AcqRel);
        for (counter, _) in &self.additional {
            counter.fetch_sub(size, Ordering::AcqRel);
        }
    }
    fn reserve(&self, size: u64) -> DbResult<()> {
        let mut reserved: Vec<&Arc<AtomicU64>> = Vec::new();
        for (counter, limit) in std::iter::once((&self.total, self.total_limit)).chain(
            self.additional
                .iter()
                .map(|(counter, limit)| (counter, *limit)),
        ) {
            if counter
                .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    n.checked_add(size).filter(|v| *v <= limit)
                })
                .is_err()
            {
                for counter in reserved {
                    counter.fetch_sub(size, Ordering::AcqRel);
                }
                return Err(error(
                    "RESULT_QUOTA_EXCEEDED",
                    "Result storage quota reached; available rows are partial",
                ));
            }
            reserved.push(counter);
        }
        Ok(())
    }
    pub fn rows(&self) -> u64 {
        self.rows
    }
    pub fn append(&mut self, rows: &[ResultRow]) -> DbResult<()> {
        // Persist bounded frames and preserve values as strings before they reach JS.
        for chunk in rows.chunks(PAGE_ROWS) {
            let values: Vec<WireRow> = chunk.iter().map(wire_row).collect();
            self.append_wire(&values)?;
        }
        Ok(())
    }
    fn append_wire(&mut self, values: &[WireRow]) -> DbResult<()> {
        let plain = serde_json::to_vec(&values)
            .map_err(|_| error("RESULT_ENCODING_FAILED", "Result could not be encoded"))?;
        if plain.len() > PAGE_BYTES {
            return Err(error(
                "RESULT_PAGE_TOO_LARGE",
                "Serialized result batch exceeds 2 MiB",
            ));
        }
        let context = self.frames.len().to_string();
        let data = self.cipher.seal(&plain, context.as_bytes())?;
        let size = data.len() as u64;
        if self.bytes.saturating_add(size) > self.limit {
            return Err(error(
                "RESULT_QUOTA_EXCEEDED",
                "Result reached its disk limit; available rows are partial",
            ));
        }
        self.reserve(size)?;
        let offset = self.bytes;
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| error("RESULT_CLOSED", "Result is closed"))?;
        if let Err(e) = file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| file.write_all(&data))
        {
            self.release(size);
            return Err(io_error(e));
        }
        self.frames.push(Frame {
            offset,
            length: data.len(),
            start: self.rows,
            count: values.len(),
        });
        self.bytes += size;
        self.rows += values.len() as u64;
        Ok(())
    }
    pub fn page(&mut self, offset: u64, limit: usize) -> DbResult<Vec<WireRow>> {
        if limit == 0 || limit > PAGE_ROWS {
            return Err(error("INVALID_PAGE", "Request between 1 and 256 rows"));
        }
        let mut rows = Vec::new();
        let mut bytes = 0;
        for (i, frame) in self.frames.iter().enumerate() {
            if frame.start + frame.count as u64 <= offset {
                continue;
            }
            if frame.start >= offset.saturating_add(limit as u64) {
                break;
            }
            let file = self
                .file
                .as_mut()
                .ok_or_else(|| error("RESULT_CLOSED", "Result is closed"))?;
            let mut encrypted = vec![0; frame.length];
            file.seek(SeekFrom::Start(frame.offset)).map_err(io_error)?;
            file.read_exact(&mut encrypted).map_err(io_error)?;
            let decoded: Vec<WireRow> =
                serde_json::from_slice(&self.cipher.open(&encrypted, i.to_string().as_bytes())?)
                    .map_err(|_| error("RESULT_ENCODING_FAILED", "Result storage is invalid"))?;
            for (n, row) in decoded.into_iter().enumerate() {
                let index = frame.start + n as u64;
                if index < offset || index >= offset.saturating_add(limit as u64) {
                    continue;
                }
                let size = serde_json::to_vec(&row)
                    .map_err(|_| error("RESULT_ENCODING_FAILED", "Result could not be encoded"))?
                    .len()
                    + 1;
                if bytes + size > PAGE_BYTES {
                    if rows.is_empty() {
                        return Err(error(
                            "RESULT_PAGE_TOO_LARGE",
                            "Serialized row exceeds page limit",
                        ));
                    }
                    return Ok(rows);
                }
                bytes += size;
                rows.push(row);
            }
        }
        Ok(rows)
    }
    /// Build a new encrypted view without modifying the original. Sort has an explicit
    /// memory budget; filtering is a bounded streaming scan of already-received rows.
    pub fn transform(
        &mut self,
        column: Option<usize>,
        descending: bool,
        filter: &str,
    ) -> DbResult<Self> {
        let directory = self
            .path
            .parent()
            .ok_or_else(|| error("SPOOL_IO_ERROR", "Result directory unavailable"))?;
        let mut output = Self::with_limits(
            directory,
            Uuid::new_v4(),
            self.total.clone(),
            self.limit,
            self.total_limit,
        )?
        .with_budgets(self.additional.clone());
        let mut offset = 0;
        let mut sort_rows = Vec::new();
        let mut sort_bytes = 0;
        while offset < self.rows {
            let rows = self.page(offset, PAGE_ROWS)?;
            if rows.is_empty() {
                break;
            }
            offset += rows.len() as u64;
            let selected = rows
                .into_iter()
                .filter(|row| {
                    filter.is_empty()
                        || row
                            .iter()
                            .any(|c| c.value.as_ref().is_some_and(|v| v.contains(filter)))
                })
                .collect::<Vec<_>>();
            if column.is_some() {
                sort_bytes += serde_json::to_vec(&selected)
                    .map_err(|_| error("RESULT_ENCODING_FAILED", "Result invalid"))?
                    .len();
                if sort_bytes > 32 * 1024 * 1024 {
                    return Err(error(
                        "SORT_CAPACITY_EXCEEDED",
                        "Sorting exceeds the 32 MiB budget; use an explicit ORDER BY query",
                    ));
                }
                sort_rows.extend(selected);
            } else if !selected.is_empty() {
                output.append_wire(&selected)?;
            }
        }
        if let Some(column) = column {
            sort_rows.sort_by(|a, b| {
                let left = a.get(column).and_then(|c| c.value.as_deref());
                let right = b.get(column).and_then(|c| c.value.as_deref());
                let order = match (left, right) {
                    (Some(left), Some(right))
                        if a.get(column).is_some_and(|c| {
                            matches!(c.kind.as_str(), "signed" | "unsigned" | "decimal" | "float")
                        }) =>
                    {
                        match (
                            left.parse::<bigdecimal::BigDecimal>(),
                            right.parse::<bigdecimal::BigDecimal>(),
                        ) {
                            (Ok(a), Ok(b)) => a.cmp(&b),
                            _ => left.cmp(right),
                        }
                    }
                    (Some(a), Some(b)) => a.cmp(b),
                    _ => left.cmp(&right),
                };
                if descending { order.reverse() } else { order }
            });
            for chunk in sort_rows.chunks(PAGE_ROWS) {
                output.append_wire(chunk)?;
            }
        }
        Ok(output)
    }
}
impl Drop for Spool {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.path);
        self.release(self.bytes);
    }
}
pub fn cleanup_spools(directory: &Path) -> DbResult<()> {
    private_directory(directory)?;
    for entry in std::fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(stem) = name.strip_suffix(".spool")
            && Uuid::parse_str(stem).is_ok()
            && entry.file_type().map_err(io_error)?.is_file()
        {
            std::fs::remove_file(entry.path()).map_err(io_error)?;
        }
    }
    Ok(())
}
