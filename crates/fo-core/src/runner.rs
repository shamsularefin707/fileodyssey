//! Background job runner: a fixed worker pool, per-input event streams,
//! cancellation, and atomic placement of outputs (work dir → final name).

use crate::naming::output_path_with;
use crate::registry::Registry;
use crate::tool::{CancelToken, Ctx, ToolError};
use fo_types::{JobEvent, JobRequest, JobStatus};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

pub type Sink = Arc<dyn Fn(JobEvent) + Send + Sync>;

struct Work {
    job_id: String,
    index: usize,
    /// The input this unit reports events under.
    input: String,
    /// Every file this unit reads: just `input`, or all of a combining job's inputs.
    inputs: Vec<String>,
    combine: bool,
    req: Arc<JobRequest>,
    sink: Sink,
    cancel: CancelToken,
    remaining: Arc<AtomicUsize>,
}

type Jobs = Arc<Mutex<HashMap<String, CancelToken>>>;

pub struct Runner {
    tx: Mutex<mpsc::Sender<Work>>,
    jobs: Jobs,
    registry: Arc<Registry>,
}

impl Runner {
    pub fn new(registry: Arc<Registry>, workers: usize) -> Self {
        let (tx, rx) = mpsc::channel::<Work>();
        let rx = Arc::new(Mutex::new(rx));
        let jobs: Jobs = Arc::default();
        for _ in 0..workers.max(1) {
            let rx = Arc::clone(&rx);
            let registry: Arc<Registry> = Arc::clone(&registry);
            let jobs = Arc::clone(&jobs);
            std::thread::spawn(move || loop {
                let next = rx.lock().expect("runner queue").recv();
                let Ok(work) = next else { return };
                process(&registry, &jobs, work);
            });
        }
        Self {
            tx: Mutex::new(tx),
            jobs,
            registry,
        }
    }

    /// Queue one job; returns its id. Each input gets its own event stream:
    /// Queued → Running* → Done | Failed | Cancelled. A combining tool's job
    /// is a single unit reported under its first input.
    pub fn submit(&self, req: JobRequest, sink: Sink) -> String {
        let job_id = uuid::Uuid::new_v4().to_string();
        let cancel = CancelToken::new();
        self.jobs
            .lock()
            .expect("jobs")
            .insert(job_id.clone(), cancel.clone());
        let req = Arc::new(req);
        let combine = self
            .registry
            .get(&req.tool_id)
            .is_some_and(|t| t.combines());
        let units: Vec<(String, Vec<String>)> = if combine {
            req.inputs
                .first()
                .map(|first| (first.clone(), req.inputs.clone()))
                .into_iter()
                .collect()
        } else {
            req.inputs
                .iter()
                .map(|i| (i.clone(), vec![i.clone()]))
                .collect()
        };
        let remaining = Arc::new(AtomicUsize::new(units.len()));
        let tx = self.tx.lock().expect("runner queue");
        for (index, (input, inputs)) in units.into_iter().enumerate() {
            sink(event(&job_id, &input, JobStatus::Queued));
            let _ = tx.send(Work {
                job_id: job_id.clone(),
                index,
                input,
                inputs,
                combine,
                req: Arc::clone(&req),
                sink: Arc::clone(&sink),
                cancel: cancel.clone(),
                remaining: Arc::clone(&remaining),
            });
        }
        if req.inputs.is_empty() {
            self.jobs.lock().expect("jobs").remove(&job_id);
        }
        job_id
    }

    pub fn cancel(&self, job_id: &str) {
        if let Some(t) = self.jobs.lock().expect("jobs").get(job_id) {
            t.cancel();
        }
    }
}

fn event(job_id: &str, input: &str, status: JobStatus) -> JobEvent {
    JobEvent {
        job_id: job_id.to_owned(),
        input: input.to_owned(),
        status,
    }
}

fn process(registry: &Registry, jobs: &Jobs, w: Work) {
    let status = run_one(registry, &w);
    (w.sink)(event(&w.job_id, &w.input, status));
    if w.remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
        jobs.lock().expect("jobs").remove(&w.job_id);
    }
}

fn run_one(registry: &Registry, w: &Work) -> JobStatus {
    if w.cancel.is_cancelled() {
        return JobStatus::Cancelled;
    }
    let Some(tool) = registry.get(&w.req.tool_id) else {
        return failed("This tool isn't available");
    };
    let input = Path::new(&w.input);
    for path in w.inputs.iter().map(Path::new) {
        if !path.is_file() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return failed(&format!("File not found: {name}"));
        }
    }
    let out_dir = PathBuf::from(&w.req.out_dir);
    if !out_dir.is_absolute() {
        return failed("Choose a folder to save results in");
    }
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        return failed(&format!("Can't use the output folder: {e}"));
    }
    let work_dir = out_dir.join(format!(".fileodyssey-{}-{}", w.job_id, w.index));
    if let Err(e) = std::fs::create_dir(&work_dir) {
        return failed(&format!("Can't use the output folder: {e}"));
    }

    (w.sink)(event(
        &w.job_id,
        &w.input,
        JobStatus::Running { fraction: 0.0 },
    ));
    let progress = |f: f32| {
        let fraction = f.clamp(0.0, 1.0);
        (w.sink)(event(&w.job_id, &w.input, JobStatus::Running { fraction }));
    };
    let ctx = Ctx {
        work_dir: &work_dir,
        target: w.req.target.as_deref(),
        options: &w.req.options,
        cancel: &w.cancel,
        progress: &progress,
    };
    // A tool that panics (e.g. a decoder on a malformed file) must not kill
    // the worker or leave the job hanging.
    let paths: Vec<PathBuf> = w.inputs.iter().map(PathBuf::from).collect();
    let label = if w.combine { "combined" } else { "converted" };
    let result = catch_unwind(AssertUnwindSafe(|| {
        if w.combine {
            tool.run_many(&paths, &ctx)
        } else {
            tool.run(input, &ctx)
        }
    }))
    .unwrap_or_else(|_| Err(ToolError::Failed("The tool crashed on this file".into())));

    let status = match result {
        _ if w.cancel.is_cancelled() => JobStatus::Cancelled,
        Err(ToolError::Cancelled) => JobStatus::Cancelled,
        Err(ToolError::Failed(m)) => JobStatus::Failed { message: m },
        Ok(out) => match place_outputs(input, &out_dir, &out.files, label) {
            Ok(outputs) => JobStatus::Done {
                outputs,
                report: out.report,
            },
            Err(e) => failed(&format!("Couldn't save the result: {e}")),
        },
    };
    let _ = std::fs::remove_dir_all(&work_dir);
    status
}

fn place_outputs(
    input: &Path,
    out_dir: &Path,
    files: &[(PathBuf, String)],
    label: &str,
) -> std::io::Result<Vec<String>> {
    files
        .iter()
        .map(|(src, ext)| place_output_with(src, input, out_dir, ext, label))
        .collect()
}

/// Move `src` to the next free `(converted N)` name in `out_dir`, claiming the
/// name atomically so two jobs finishing together can never overwrite each
/// other (a plain rename replaces an existing file on every platform).
pub fn place_output(
    src: &Path,
    input: &Path,
    out_dir: &Path,
    ext: &str,
) -> std::io::Result<String> {
    place_output_with(src, input, out_dir, ext, "converted")
}

pub fn place_output_with(
    src: &Path,
    input: &Path,
    out_dir: &Path,
    ext: &str,
    label: &str,
) -> std::io::Result<String> {
    loop {
        let dest = output_path_with(input, out_dir, ext, label);
        match std::fs::hard_link(src, &dest) {
            Ok(()) => {
                std::fs::remove_file(src)?;
                return Ok(dest.to_string_lossy().into_owned());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            // Filesystems without hard links (FAT/exFAT SD cards): claim the
            // name with create_new, then copy into it.
            Err(_) => match OpenOptions::new().write(true).create_new(true).open(&dest) {
                Ok(mut out) => {
                    let copied = File::open(src).and_then(|mut i| std::io::copy(&mut i, &mut out));
                    if let Err(e) = copied {
                        drop(out);
                        let _ = std::fs::remove_file(&dest);
                        return Err(e);
                    }
                    std::fs::remove_file(src)?;
                    return Ok(dest.to_string_lossy().into_owned());
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            },
        }
    }
}

fn failed(message: &str) -> JobStatus {
    JobStatus::Failed {
        message: message.to_owned(),
    }
}
