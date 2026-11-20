//! A small binary envelope for requests and results, so files cross the
//! webview boundary as raw bytes instead of JSON number arrays.
//!
//! `MAGIC` (5 bytes) · header length (u32 LE) · JSON header · payloads in
//! header order. The worker (`crates/fo-ui/pandoc-worker/worker.js`)
//! implements the same layout.

use super::{PandocRequest, PandocResult};
use serde::{Deserialize, Serialize};

const REQ: &[u8; 5] = b"FOPQ1";
const RES: &[u8; 5] = b"FOPR1";

#[derive(Serialize, Deserialize)]
struct Entry {
    name: String,
    len: usize,
}

#[derive(Serialize, Deserialize)]
struct ReqHead {
    options: serde_json::Value,
    files: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct ResHead {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    stderr: String,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(default)]
    stdout_len: usize,
    #[serde(default)]
    files: Vec<Entry>,
}

fn pack(magic: &[u8; 5], head: &impl Serialize, blobs: &[&[u8]]) -> Vec<u8> {
    let h = serde_json::to_vec(head).expect("header serialises");
    let mut out = Vec::with_capacity(9 + h.len() + blobs.iter().map(|b| b.len()).sum::<usize>());
    out.extend_from_slice(magic);
    out.extend_from_slice(&(h.len() as u32).to_le_bytes());
    out.extend_from_slice(&h);
    for b in blobs {
        out.extend_from_slice(b);
    }
    out
}

fn unpack<'a, T: for<'de> Deserialize<'de>>(
    magic: &[u8; 5],
    buf: &'a [u8],
) -> Result<(T, &'a [u8]), String> {
    let bad = || "The converter sent a damaged reply".to_string();
    if buf.len() < 9 || &buf[..5] != magic {
        return Err(bad());
    }
    let n = u32::from_le_bytes(buf[5..9].try_into().map_err(|_| bad())?) as usize;
    let head = buf.get(9..9 + n).ok_or_else(bad)?;
    let head = serde_json::from_slice(head).map_err(|_| bad())?;
    Ok((head, &buf[9 + n..]))
}

fn take<'a>(rest: &mut &'a [u8], len: usize) -> Result<&'a [u8], String> {
    if rest.len() < len {
        return Err("The converter sent a damaged reply".into());
    }
    let (a, b) = rest.split_at(len);
    *rest = b;
    Ok(a)
}

pub fn encode_request(req: &PandocRequest) -> Vec<u8> {
    let head = ReqHead {
        options: req.options.clone(),
        files: req
            .files
            .iter()
            .map(|(n, b)| Entry {
                name: n.clone(),
                len: b.len(),
            })
            .collect(),
    };
    let blobs: Vec<&[u8]> = req.files.iter().map(|(_, b)| b.as_slice()).collect();
    pack(REQ, &head, &blobs)
}

pub fn decode_request(buf: &[u8]) -> Result<PandocRequest, String> {
    let (head, mut rest): (ReqHead, _) = unpack(REQ, buf)?;
    let mut files = Vec::new();
    for e in head.files {
        files.push((e.name, take(&mut rest, e.len)?.to_vec()));
    }
    Ok(PandocRequest {
        options: head.options,
        files,
    })
}

pub fn encode_result(res: &Result<PandocResult, String>) -> Vec<u8> {
    match res {
        Err(e) => pack(
            RES,
            &ResHead {
                error: Some(e.clone()),
                stderr: String::new(),
                warnings: vec![],
                stdout_len: 0,
                files: vec![],
            },
            &[],
        ),
        Ok(r) => {
            let head = ResHead {
                error: None,
                stderr: r.stderr.clone(),
                warnings: r.warnings.clone(),
                stdout_len: r.stdout.len(),
                files: r
                    .files
                    .iter()
                    .map(|(n, b)| Entry {
                        name: n.clone(),
                        len: b.len(),
                    })
                    .collect(),
            };
            let mut blobs: Vec<&[u8]> = vec![&r.stdout];
            blobs.extend(r.files.iter().map(|(_, b)| b.as_slice()));
            pack(RES, &head, &blobs)
        }
    }
}

/// Outer `Err`: the frame itself is broken. Inner `Err`: Pandoc failed.
pub fn decode_result(buf: &[u8]) -> Result<Result<PandocResult, String>, String> {
    let (head, mut rest): (ResHead, _) = unpack(RES, buf)?;
    if let Some(e) = head.error {
        return Ok(Err(e));
    }
    let stdout = take(&mut rest, head.stdout_len)?.to_vec();
    let mut files = Vec::new();
    for e in head.files {
        files.push((e.name, take(&mut rest, e.len)?.to_vec()));
    }
    Ok(Ok(PandocResult {
        stdout,
        stderr: head.stderr,
        files,
        warnings: head.warnings,
    }))
}
