//! A summons nobody is watching: the job (§FS-005-dispatch.17,
//! §AR-002-summons.5).
//!
//! A job is a directory under ephor's state and nothing else — `job.json` (the
//! steps, the site, the dossier, the matter it is about), `log` (its whole
//! output, in order), `lock`, and `outcome.json` written at the end. That
//! layout is spelled here and nowhere else: the interface starts a job, the
//! board watches one, and the command line reads one, all through this module.
//!
//! The executor is unchanged (§AR-002-summons.2). The supervisor is `ephor job
//! run <dir>`, whose own standard output *is* the log, so each step runs in
//! [`Mode::Interactive`] — inheriting streams that happen not to be a terminal
//! — and no third mode has to exist.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{EphorError, Result};
use crate::seams::summons::{self, Mode, Outcome, Place, Site};

/// The lock a supervisor holds for exactly as long as it runs. Its being held
/// is the only liveness signal (§FS-005-dispatch.17): a record that a job
/// started is a different claim, and the operating system releases the lock
/// however the supervisor dies.
const LOCK: &str = "lock";

/// What the job is, written before it starts and never rewritten.
const RECORD: &str = "job.json";

/// Everything the reader would have watched, in order.
const LOG: &str = "log";

/// How it ended, written by the supervisor on its way out.
const OUTCOME: &str = "outcome.json";

/// How long an ended job is kept before it is swept (§FS-005-dispatch.17). Its
/// record stays with the item that long — long enough to read the log of
/// something that ran overnight, short enough that the directory is not an
/// archive.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// One thing the job runs, in order. A step is a summons: a binding, a place,
/// and the words that name it on a row (§AR-002-summons.5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub icon: String,
    pub description: String,
    pub command: String,
    /// Where it runs, in the binding's own spelling — `workspace`, `root`, or
    /// `repo:<name>` (§AR-002-summons.1).
    #[serde(default)]
    pub cwd: Option<String>,
    /// A directory this step's contract is to create, verified afterwards
    /// rather than trusted (§FS-006-project-interface.8). The checkout's, and
    /// nothing else's.
    #[serde(default)]
    pub creates: Option<PathBuf>,
    /// Where the steps after this one run, when this step is the checkout that
    /// made it. The site the job started with, otherwise.
    #[serde(default)]
    pub becomes_workspace: bool,
}

/// What this seam writes today. A record from an older ephor still reads: every
/// field added since is defaulted, so a job started before the version moved is
/// a row like any other rather than a directory nothing can open.
pub const VERSION: u32 = 2;

/// What a job is, written once before it starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub version: u32,
    /// The project the entry was pressed in.
    pub project: String,
    /// The matter it was started about, where one was — a branch row has none
    /// (§FS-004-quick-actions.6), and that is not an edge.
    #[serde(default)]
    pub item: Option<String>,
    /// What the row said, so a job says the same thing wherever it is read.
    pub icon: String,
    pub description: String,
    pub root: PathBuf,
    #[serde(default)]
    pub workspace: Option<PathBuf>,
    pub steps: Vec<Step>,
    /// Which menu entry this job came from, by the name a command calls it —
    /// the configured id, or the description where the entry is anonymous
    /// (§FS-005-dispatch.21). A job records which entry it came from because
    /// nothing could otherwise match it back to the row that started it.
    #[serde(default)]
    pub action: Option<String>,
    /// The branch the entry was pressed on, where the subject was a branch row
    /// rather than a matter (§FS-004-quick-actions.6). A replay is offered per
    /// branch, so a job on one branch is not the running mark of another
    /// (§FS-005-dispatch.21).
    #[serde(default)]
    pub branch: Option<String>,
    /// The handle the reader's own window opener printed, where this job's
    /// supervisor runs inside a window (§FS-005-dispatch.22). What opening the
    /// job gets the reader, in place of a log: the program's own output went to
    /// a screen they were looking at and is not duplicated
    /// (§AR-002-summons.6).
    #[serde(default)]
    pub window: Option<String>,
    /// The matter as `EPHOR_*` pairs, resolved where the entry was pressed —
    /// one vocabulary, identical to what a watched action receives
    /// (§FS-005-dispatch.8). Carried rather than recomputed: the supervisor
    /// has no feed to rebuild it from.
    #[serde(default)]
    pub dossier: Vec<(String, String)>,
    pub started: String,
}

/// How a job ended, written by the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ended {
    /// `done`, `failed`, or `parked` — the summons' own three
    /// (§FS-006-project-interface.3).
    pub outcome: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// The step that ended it, where one did.
    #[serde(default)]
    pub step: Option<String>,
    /// The one line worth showing, already composed: it is read on a screen
    /// that has no job to ask (§FS-005-dispatch.17).
    pub says: String,
    pub ended: String,
}

/// A job as anything reading one sees it: what it is, where it is, and whether
/// it is going.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub dir: PathBuf,
    pub record: Record,
    /// The lock is held: something is running this right now.
    pub live: bool,
    /// How it ended, where it has. A job that is neither live nor ended is one
    /// whose supervisor died (§AR-002-summons.5).
    pub ended: Option<Ended>,
}

impl Job {
    /// What the reader is told about a job that is not live and wrote no
    /// outcome: the supervisor died, and saying so is the honest answer where
    /// "started" would be a claim nothing supports.
    pub fn died(&self) -> bool {
        !self.live && self.ended.is_none()
    }

    /// The one line a row shows: the outcome where there is one, the last
    /// thing the log said while it runs — "still going" and "stuck" are the
    /// same words on a row, and this is the difference (§FS-005-dispatch.17).
    pub fn says(&self) -> String {
        match &self.ended {
            Some(ended) => ended.says.clone(),
            None if self.died() => format!("{}: the job died", self.record.description),
            None => tail(&self.log_path(), 1)
                .pop()
                .unwrap_or_else(|| "started".to_string()),
        }
    }

    /// When it started, where the record's stamp reads as a time
    /// (§FS-005-dispatch.18). A record ephor did not write — or wrote before
    /// it stamped one — has no time here, and the row says nothing rather
    /// than dating the job by whatever the filesystem happens to hold.
    pub fn started_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        stamp(&self.record.started)
    }

    /// When it ended, where it ended and said so.
    pub fn ended_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.ended.as_ref().and_then(|ended| stamp(&ended.ended))
    }

    /// How long it has been running, in seconds: to its end where it ended,
    /// to now where it is still going — the same question asked of a thing
    /// that has not finished. A job whose supervisor died is not timed: what
    /// is known is when it started, and how long it lasted is exactly the
    /// thing nothing recorded.
    pub fn took(&self, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
        let started = self.started_at()?;
        let until = match (self.live, self.ended_at()) {
            (true, _) => now,
            (false, Some(ended)) => ended,
            (false, None) => return None,
        };
        Some((until - started).num_seconds())
    }

    pub fn log_path(&self) -> PathBuf {
        self.dir.join(LOG)
    }
}

/// A stamp as it was written: RFC 3339, or nothing.
fn stamp(text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.with_timezone(&chrono::Utc))
}

/// Where jobs live. One directory, listed rather than remembered
/// (§FS-005-dispatch.17): a job another ephor started is a row here like any
/// other.
pub fn jobs_dir() -> PathBuf {
    crate::paths::state_dir().join("jobs")
}

/// Whether a job is running: its lock is held. Probed with a non-blocking
/// shared try-lock and never waited on, exactly as a runtime's execution root
/// is (§AR-007-runtime.1) — a probe that queued would park the reader behind
/// the job they are asking about.
pub fn live(dir: &Path) -> bool {
    let Ok(file) = fs::File::open(dir.join(LOCK)) else {
        return false;
    };
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(fs::TryLockError::WouldBlock) => true,
        // The probe could not say. Not-live is the honest default: a row
        // claiming a job that may not exist is the watch inventing news.
        Err(fs::TryLockError::Error(_)) => false,
    }
}

/// Every job on disk, newest first. Reads four small files per job and asks
/// nothing of anything else.
pub fn all() -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    let Ok(entries) = fs::read_dir(jobs_dir()) else {
        return jobs;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        if let Some(job) = read(&dir) {
            jobs.push(job);
        }
    }
    // The id leads with the instant it started, so its own order is time's.
    jobs.sort_by(|left, right| right.id.cmp(&left.id));
    jobs
}

/// One job, by the directory it is. None where the record cannot be read: a
/// directory without one is not a job, and half a job is not reported as one.
pub fn read(dir: &Path) -> Option<Job> {
    let text = fs::read_to_string(dir.join(RECORD)).ok()?;
    let record: Record = serde_json::from_str(&text).ok()?;
    let ended = fs::read_to_string(dir.join(OUTCOME))
        .ok()
        .and_then(|text| serde_json::from_str::<Ended>(&text).ok());
    Some(Job {
        id: dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        live: live(dir),
        dir: dir.to_path_buf(),
        record,
        ended,
    })
}

/// One job by its id.
pub fn find(id: &str) -> Option<Job> {
    read(&jobs_dir().join(id))
}

/// The last lines of a file, without reading the whole of it into a row's
/// worth of screen. Cheap by construction: a log is the one artifact here that
/// can be large.
pub fn tail(path: &Path, lines: usize) -> Vec<String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut kept: Vec<String> = text
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(lines)
        .map(|line| line.trim_end().to_string())
        .collect();
    kept.reverse();
    kept
}

/// Start a job and leave it (§FS-005-dispatch.17): write the record, then
/// spawn the supervisor in its own process group so the reader's Ctrl-C and
/// the terminal's hangup are not the job's business. Nothing is waited on —
/// the returned [`Job`] is what was just written, and the lock the supervisor
/// takes is what will answer for it from here on.
pub fn start(record: Record) -> Result<Job> {
    let id = name(&record);
    let dir = jobs_dir().join(&id);
    fs::create_dir_all(&dir)
        .map_err(|err| EphorError::Command(format!("Cannot create {}: {err}", dir.display())))?;
    let text = serde_json::to_string_pretty(&record)
        .map_err(|err| EphorError::Command(format!("Cannot write the job: {err}")))?;
    fs::write(dir.join(RECORD), text)
        .map_err(|err| EphorError::Command(format!("Cannot write {}: {err}", dir.display())))?;
    // Created here rather than by the supervisor: a job whose directory exists
    // and whose log does not would read as a job that wrote nothing, when what
    // happened is that the supervisor never got as far as starting.
    let log = fs::File::create(dir.join(LOG))
        .map_err(|err| EphorError::Command(format!("Cannot open the job log: {err}")))?;
    let errors = log
        .try_clone()
        .map_err(|err| EphorError::Command(format!("Cannot open the job log: {err}")))?;

    let exe = std::env::current_exe()
        .map_err(|err| EphorError::Command(format!("Cannot find ephor itself: {err}")))?;
    let mut command = Command::new(exe);
    command
        .arg("job")
        .arg("run")
        .arg(&dir)
        // Its streams are the log: that is the whole of what "detached" means
        // here, and why the steps below need no summons mode of their own
        // (§AR-002-summons.5).
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(errors));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // A group of its own, so quitting the interface — or closing the
        // terminal under it — does not take down a job that was started
        // precisely because nobody has to stay (§FS-005-dispatch.17).
        command.process_group(0);
    }
    summons::spawn(&mut command)
        .map_err(|err| EphorError::Command(format!("Cannot start the job: {err}")))?;

    // Wait for the supervisor to take the lock, briefly. Not for the job —
    // nothing here waits for a job — but so that the caller's first look at it
    // is the truth: liveness is the lock (§FS-005-dispatch.17), and a job
    // reported as not running for the second it takes to start would have the
    // reader pressing the key again. Bounded, because a supervisor that never
    // starts is a job that died, and that is a row of its own rather than
    // something to hang on.
    for _ in 0..STARTING.as_millis() / STARTED_EVERY.as_millis() {
        if live(&dir) {
            break;
        }
        std::thread::sleep(STARTED_EVERY);
    }

    read(&dir).ok_or_else(|| EphorError::Command("The job could not be read back".to_string()))
}

/// How long [`start`] will watch for the supervisor's lock, and how often.
/// Taking it is a spawn and a file open; the wait is generous and is spent
/// only on the way to the first frame that shows the job.
const STARTING: Duration = Duration::from_millis(500);
const STARTED_EVERY: Duration = Duration::from_millis(5);

/// The directory name: the instant it started, so listing is time's own order,
/// then what it is, so a directory is readable without opening anything.
fn name(record: &Record) -> String {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let slug: String = record
        .description
        .chars()
        .map(|character| match character.is_ascii_alphanumeric() {
            true => character.to_ascii_lowercase(),
            false => '-',
        })
        .collect();
    let slug = slug.trim_matches('-').replace("--", "-");
    let slug: String = slug.chars().take(48).collect();
    match slug.is_empty() {
        true => format!("{stamp}"),
        false => format!("{stamp}-{}", slug.trim_matches('-')),
    }
}

/// Run a job: the supervisor (§AR-002-summons.5). Takes the lock, runs each
/// step in turn, and writes how it ended. Its own streams are the log, so a
/// step inheriting them writes there without anything here copying bytes.
pub fn supervise(dir: &Path) -> Result<()> {
    let text = fs::read_to_string(dir.join(RECORD))
        .map_err(|err| EphorError::Command(format!("Cannot read {}: {err}", dir.display())))?;
    let record: Record = serde_json::from_str(&text)
        .map_err(|err| EphorError::Command(format!("Corrupt job {}: {err}", dir.display())))?;

    // Exclusive and non-blocking: two supervisors on one job would interleave
    // their output into one log and race to write the outcome. The lock is
    // held for this function's whole body — dropping the file frees it, which
    // is exactly the lifetime a reader's probe is asking about.
    let lock = fs::File::create(dir.join(LOCK))
        .map_err(|err| EphorError::Command(format!("Cannot open the job lock: {err}")))?;
    if lock.try_lock().is_err() {
        return Err(EphorError::Command(format!(
            "{} is already running",
            dir.display()
        )));
    }

    let mut site = Site {
        root: record.root.clone(),
        workspace: record.workspace.clone(),
    };
    let mut ended = Ended {
        outcome: "done".to_string(),
        exit_code: Some(0),
        step: None,
        says: format!("{} {}: ok", record.icon, record.description),
        ended: String::new(),
    };
    for step in &record.steps {
        println!("\n▶ {} {}", step.icon, step.description);
        println!("  $ {}\n", step.command);
        match run_step(&record, step, &site) {
            Ok(()) => {}
            Err(Stopped { says, code, parked }) => {
                ended.outcome = match parked {
                    true => "parked".to_string(),
                    false => "failed".to_string(),
                };
                // The step's own code, not a stand-in: `None` here means a
                // signal killed it, which is a different fact from any number
                // (§FS-006-project-interface.3).
                ended.exit_code = code;
                ended.step = Some(step.description.clone());
                ended.says = says;
                break;
            }
        }
        if step.becomes_workspace {
            if let Some(made) = &step.creates {
                site.workspace = Some(made.clone());
            }
        }
    }
    println!("\n{}", ended.says);
    ended.ended = chrono::Utc::now().to_rfc3339();
    let text = serde_json::to_string_pretty(&ended)
        .map_err(|err| EphorError::Command(format!("Cannot write the outcome: {err}")))?;
    fs::write(dir.join(OUTCOME), text)
        .map_err(|err| EphorError::Command(format!("Cannot write the outcome: {err}")))?;
    Ok(())
}

/// Why a step stopped the job: the sentence, the code the command exited
/// with where it exited, and whether it was the *come back later* one.
struct Stopped {
    says: String,
    code: Option<i32>,
    parked: bool,
}

impl Stopped {
    /// Something on ephor's side of the step — an unparsable place, a
    /// directory that is not there. No command ran, so there is no code.
    fn here(says: String) -> Stopped {
        Stopped {
            says,
            code: None,
            parked: false,
        }
    }
}

/// One step, and what it leaves behind when it does not succeed. The contract
/// of a step that makes a directory is checked afterwards rather than trusted
/// (§FS-006-project-interface.8).
fn run_step(record: &Record, step: &Step, site: &Site) -> std::result::Result<(), Stopped> {
    let place = match &step.cwd {
        Some(spec) => Place::parse(spec).map_err(|err| Stopped::here(err.to_string()))?,
        None => Place::Workspace,
    };
    let summons = summons::Summons::new(&step.description, &step.command)
        .at(place)
        .carrying(record.dossier.clone());
    let answer = summons::run(&summons, site, Mode::Interactive)
        .map_err(|err| Stopped::here(err.to_string()))?;
    if answer.outcome != Outcome::Done {
        return Err(Stopped {
            says: answer.refusal(&step.description),
            code: answer.exit_code,
            parked: answer.outcome == Outcome::Parked,
        });
    }
    if let Some(made) = &step.creates {
        if !made.is_dir() {
            return Err(Stopped::here(format!(
                "{}: did not create {}",
                step.description,
                made.display()
            )));
        }
    }
    Ok(())
}

/// Sweep the jobs that are over and old (§FS-005-dispatch.17). A live job is
/// never touched, and neither is one whose age cannot be read — a sweep that
/// guessed would delete the log somebody is reading.
pub fn sweep() {
    for job in all() {
        if job.live {
            continue;
        }
        // A job that ended is dated by its outcome; one whose supervisor died
        // wrote none, so it is dated by the record that started it — otherwise
        // exactly the jobs with nothing to say for themselves would be the
        // ones kept forever.
        let dated = match job.ended {
            Some(_) => job.dir.join(OUTCOME),
            None => job.dir.join(RECORD),
        };
        let Ok(age) = fs::metadata(dated).and_then(|meta| meta.modified()) else {
            continue;
        };
        if age.elapsed().map(|since| since > KEEP).unwrap_or(false) {
            let _ = fs::remove_dir_all(&job.dir);
        }
    }
}

/// `ephor job` (§FS-005-dispatch.17): the same job the interface starts, read
/// from the command line — because a job that outlives the interface has to be
/// answerable without it.
pub fn job(args: &crate::cli::JobArgs) -> Result<std::process::ExitCode> {
    use crate::cli::JobCommand;
    match args.command.as_ref() {
        Some(JobCommand::Run(run)) => {
            supervise(&run.dir)?;
            Ok(std::process::ExitCode::SUCCESS)
        }
        Some(JobCommand::Log(log)) => {
            let mut job = find(&log.id)
                .ok_or_else(|| EphorError::Command(format!("No job called '{}'", log.id)))?;
            // `--follow` waits for the job to end, whichever form the answer
            // takes. Following is how a script waits on a job, so the two
            // flags together are the useful pair, not a contradiction: under
            // `--json` nothing is streamed — standard output belongs to the
            // reading alone (§FS-011-command-line.7) — and the whole log lands
            // in it as a field once the job is over.
            if log.follow {
                let quiet = log.json;
                let mut out = std::io::stdout();
                follow(&job, |fresh| {
                    if quiet {
                        return;
                    }
                    use std::io::Write;
                    let _ = out.write_all(fresh);
                    let _ = out.flush();
                });
                // The job as it *ended*, not as it stood when the command was
                // typed: what the follow waited for is the final state, and
                // answering with the one from before the wait would be
                // answering a question nobody asked.
                job = find(&log.id).unwrap_or(job);
            }
            if log.json {
                // The log is the reading's own field rather than raw text on
                // standard output, so a script gets the job's state with it
                // (§FS-011-command-line.7).
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "id": job.id,
                        "project": job.record.project,
                        "item": job.record.item,
                        "description": job.record.description,
                        "started": job.record.started,
                        "live": job.live,
                        "died": job.died(),
                        "outcome": job.ended.as_ref().map(|ended| ended.outcome.clone()),
                        "says": job.says(),
                        "log": log_text(&job.log_path()),
                    }))
                    .unwrap_or_default()
                );
            } else if !log.follow {
                print!("{}", log_text(&job.log_path()));
            }
            Ok(std::process::ExitCode::SUCCESS)
        }
        Some(JobCommand::List(list)) => print_jobs(list),
        None => print_jobs(&crate::cli::JobListArgs::default()),
    }
}

/// A job's log as text, whatever the job actually wrote.
///
/// Lossy rather than all-or-nothing. A log is bytes — a runtime that printed a
/// stray `0x80` wrote a log, not a broken one — and reading it into a `String`
/// answered the empty string for the whole file, so `ephor job log` and its
/// `--json` reported nothing at all where `--follow` printed the log fine
/// (§FS-005-dispatch.17). Two answers to "what did this job say", from one
/// command, decided by a byte.
fn log_text(path: &Path) -> String {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

/// Keep up with a job that is still writing (§FS-005-dispatch.17) — what the
/// interface asks its pager for, done here without one. It ends when the job
/// does, so a script can wait on a job by reading its log to the end.
///
/// `emit` is handed each fresh slice: the terminal for a person, and nothing
/// at all under `--json`, where the log belongs to the reading printed after
/// this returns (§FS-011-command-line.7).
fn follow(job: &Job, mut emit: impl FnMut(&[u8])) {
    let path = job.log_path();
    let mut at = 0u64;
    loop {
        // Liveness *before* the read, never after it. The lock's answer, never
        // the record's: a job whose supervisor died is over, however it died
        // (§AR-002-summons.5). Asked first because a job that ends between the
        // two is still read one more time afterwards, so the bytes it wrote on
        // its way out reach the reader — the other order drops exactly the
        // last thing the job said, which is usually why anyone followed it.
        let live = matches!(find(&job.id), Some(fresh) if fresh.live);
        // The last pass takes everything that is there, including a character
        // cut short: with the job over, the rest of it is never coming, and
        // holding those bytes back would drop the end of the log for good.
        at += fresh_bytes(&path, at, !live, &mut emit);
        if !live {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// What the log has gained since `at`, handed to `emit`, and how far that
/// moves the cursor.
///
/// Bytes rather than a string. A log is whatever the job wrote, and a
/// multi-byte character split across two writes makes a read into a `String`
/// fail — which, with the cursor left where it was, is a follow that spins
/// forever printing nothing. What is whole is emitted now; a character still
/// arriving waits for the rest of itself, and a byte that is simply not text
/// is taken with the rest rather than stalling the follow on it.
///
/// `last` says nothing more is coming. Waiting for the rest of a cut character
/// is right while the job is still writing and wrong once it has stopped: the
/// rest never arrives, and the bytes would be held back for ever — a follow
/// that silently ends one or two bytes short of what the job actually wrote.
fn fresh_bytes(path: &Path, at: u64, last: bool, emit: &mut impl FnMut(&[u8])) -> u64 {
    use std::io::{Read, Seek, SeekFrom};

    // Re-opened each pass rather than held: the supervisor may still be
    // creating the file, and a handle taken before it existed reads nothing
    // forever.
    let Ok(mut file) = fs::File::open(path) else {
        return 0;
    };
    if file.seek(SeekFrom::Start(at)).is_err() {
        return 0;
    }
    let mut fresh = Vec::new();
    if file.read_to_end(&mut fresh).is_err() || fresh.is_empty() {
        return 0;
    }
    let whole = match std::str::from_utf8(&fresh) {
        Ok(_) => fresh.len(),
        // No `error_len` is a character cut short by where the read stopped:
        // the rest of it is still coming — unless nothing more is coming at
        // all, in which case this is the whole of it. A real invalid byte is
        // never going to become valid, so it goes out with everything else.
        Err(err) if err.error_len().is_none() && !last => err.valid_up_to(),
        Err(_) => fresh.len(),
    };
    if whole == 0 {
        return 0;
    }
    emit(&fresh[..whole]);
    whole as u64
}

fn print_jobs(args: &crate::cli::JobListArgs) -> Result<std::process::ExitCode> {
    let jobs: Vec<Job> = all()
        .into_iter()
        .filter(|job| !args.live || job.live)
        .collect();
    if args.json {
        let rows: Vec<serde_json::Value> = jobs
            .iter()
            .map(|job| {
                serde_json::json!({
                    "id": job.id,
                    "project": job.record.project,
                    "item": job.record.item,
                    "description": job.record.description,
                    "started": job.record.started,
                    "live": job.live,
                    "died": job.died(),
                    "outcome": job.ended.as_ref().map(|ended| ended.outcome.clone()),
                    "says": job.says(),
                    "log": job.log_path(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
        return Ok(std::process::ExitCode::SUCCESS);
    }
    if jobs.is_empty() {
        println!("Nothing is running beneath the screen.");
        return Ok(std::process::ExitCode::SUCCESS);
    }
    for job in &jobs {
        // The state is the lock's answer, never the record's: a job whose
        // supervisor died says so rather than claiming to be running
        // (§AR-002-summons.5).
        let state = match (&job.ended, job.live) {
            (_, true) => "running",
            (Some(ended), false) => ended.outcome.as_str(),
            (None, false) => "died",
        };
        println!(
            "{:<9} {} {}  ({})",
            state, job.record.icon, job.record.description, job.record.project
        );
        println!("           {}", job.says());
        println!("           ephor job log {}", job.id);
    }
    Ok(std::process::ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The directory name leads with the instant, so the plain sort of a
    /// A record written by an older ephor is still a job: the fields added
    /// since default, so a job started before the version moved keeps its row
    /// rather than becoming a directory nothing can open
    /// (§FS-005-dispatch.17). And what a job records now is what matches it
    /// back to the row that started it (§FS-005-dispatch.21).
    #[test]
    fn a_record_from_before_the_version_moved_still_reads() {
        let older = r#"{
            "version": 1,
            "project": "widget",
            "icon": "⤴",
            "description": "rebase onto master",
            "root": "/w",
            "steps": [],
            "started": "2026-08-18T09:00:00Z"
        }"#;
        let record: Record = serde_json::from_str(older).expect("a v1 record reads");
        assert_eq!(record.version, 1);
        assert_eq!(record.action, None);
        assert_eq!(record.branch, None);
        assert_eq!(record.window, None);

        let now = Record {
            version: VERSION,
            action: Some("rebase".to_string()),
            branch: Some("you/ABC-42".to_string()),
            window: Some("@7".to_string()),
            ..record
        };
        let text = serde_json::to_string(&now).expect("it serializes");
        let back: Record = serde_json::from_str(&text).expect("and reads back");
        assert_eq!(back.version, 2);
        assert_eq!(back.action.as_deref(), Some("rebase"));
        assert_eq!(back.branch.as_deref(), Some("you/ABC-42"));
        assert_eq!(back.window.as_deref(), Some("@7"));
    }

    /// listing is time's order and nothing has to read a record to sort rows.
    #[test]
    fn a_job_is_named_for_when_it_started_and_what_it_is() {
        let record = Record {
            version: 1,
            project: "widget".to_string(),
            item: None,
            icon: "⤴".to_string(),
            description: "rebase onto master (3 behind as of Jul 28)".to_string(),
            root: PathBuf::from("/w"),
            workspace: None,
            action: None,
            branch: None,
            window: None,
            steps: Vec::new(),
            dossier: Vec::new(),
            started: String::new(),
        };
        let name = name(&record);
        assert!(name.starts_with("20"), "{name} does not lead with the year");
        assert!(
            name.ends_with("rebase-onto-master-3-behind-as-of-jul-28"),
            "{name} does not say what it is"
        );
    }

    /// A job with neither a lock nor an outcome is one whose supervisor died,
    /// and is reported as that rather than as running (§AR-002-summons.5).
    #[test]
    fn a_job_that_wrote_no_outcome_and_holds_no_lock_died() {
        let job = Job {
            id: "j".to_string(),
            dir: PathBuf::from("/nowhere"),
            record: Record {
                version: 1,
                project: "widget".to_string(),
                item: None,
                icon: "⤴".to_string(),
                description: "replay".to_string(),
                root: PathBuf::from("/w"),
                workspace: None,
                action: None,
                branch: None,
                window: None,
                steps: Vec::new(),
                dossier: Vec::new(),
                started: String::new(),
            },
            live: false,
            ended: None,
        };
        assert!(job.died());
        assert_eq!(job.says(), "replay: the job died");
    }

    /// A character split across two writes waits for the rest of itself
    /// rather than failing the read: the cursor would stay where it was and
    /// the follow would spin forever printing nothing (§FS-005-dispatch.17).
    #[test]
    fn a_character_cut_in_half_by_a_write_waits_for_its_other_half() {
        let dir = tempfile::tempdir().expect("a place to write a log");
        let path = dir.path().join("log");
        // "ok ▶" — the arrow is three bytes, and only two of them have landed.
        let whole = "ok \u{25b6}".as_bytes().to_vec();
        std::fs::write(&path, &whole[..whole.len() - 1]).expect("the partial write");

        let mut seen: Vec<u8> = Vec::new();
        let read = fresh_bytes(&path, 0, false, &mut |fresh| seen.extend_from_slice(fresh));
        assert_eq!(
            read, 3,
            "the three whole bytes, and not the split character"
        );
        assert_eq!(String::from_utf8_lossy(&seen), "ok ");

        // The rest lands, and the follow picks up the character entire.
        std::fs::write(&path, &whole).expect("the rest of the write");
        let more = fresh_bytes(&path, read, false, &mut |fresh| {
            seen.extend_from_slice(fresh)
        });
        assert_eq!(more, 3);
        assert_eq!(String::from_utf8_lossy(&seen), "ok \u{25b6}");
    }

    /// …unless nothing more is coming. Once the job is over, the rest of a cut
    /// character never arrives, and holding those bytes back would end the
    /// follow one or two bytes short of what the job actually wrote — silently,
    /// which is the worst way to lose the end of a log (§FS-005-dispatch.17).
    #[test]
    fn the_last_read_takes_the_cut_character_too_because_nothing_more_is_coming() {
        let dir = tempfile::tempdir().expect("a place to write a log");
        let path = dir.path().join("log");
        let whole = "ok \u{25b6}".as_bytes().to_vec();
        let partial = &whole[..whole.len() - 1];
        std::fs::write(&path, partial).expect("the partial write");

        let mut seen: Vec<u8> = Vec::new();
        let read = fresh_bytes(&path, 0, true, &mut |fresh| seen.extend_from_slice(fresh));
        assert_eq!(read as usize, partial.len(), "everything that is there");
        assert_eq!(seen, partial, "the cut character included");
    }

    /// `ephor job log` answers with what the job wrote, whatever it wrote. A
    /// log holding one byte that is not text used to read as the empty string
    /// for the whole file, so the command and its `--json` reported nothing
    /// where `--follow` printed the log fine — two answers to one question,
    /// decided by a byte (§FS-005-dispatch.17, §REQ-002-parity.3).
    #[test]
    fn a_log_with_a_byte_that_is_not_text_still_reads_as_what_the_job_wrote() {
        let dir = tempfile::tempdir().expect("a place to write a log");
        let path = dir.path().join("log");
        std::fs::write(&path, [b'b', b'u', b'i', b'l', b't', 0xff, b'\n']).expect("the write");
        let text = log_text(&path);
        assert!(text.starts_with("built"), "{text:?}");
        assert!(text.ends_with('\n'), "{text:?}");
        // And a log that is not there is empty rather than an error: a job
        // that has not written yet has written nothing.
        assert_eq!(log_text(&dir.path().join("nothing")), "");
    }

    /// A byte that is simply not text does not stall the follow. It will never
    /// become valid, so waiting for it is waiting forever — it goes out with
    /// the rest and the cursor moves past it.
    #[test]
    fn a_byte_that_is_not_text_goes_out_rather_than_stalling() {
        let dir = tempfile::tempdir().expect("a place to write a log");
        let path = dir.path().join("log");
        std::fs::write(&path, [b'a', 0xff, b'b']).expect("the write");
        let mut seen: Vec<u8> = Vec::new();
        let read = fresh_bytes(&path, 0, false, &mut |fresh| seen.extend_from_slice(fresh));
        assert_eq!(read, 3);
        assert_eq!(seen, vec![b'a', 0xff, b'b']);
    }

    /// Nothing new is not an error, and moves nothing.
    #[test]
    fn an_unmoved_log_reads_nothing_and_stays_where_it_was() {
        let dir = tempfile::tempdir().expect("a place to write a log");
        let path = dir.path().join("log");
        std::fs::write(&path, "done\n").expect("the write");
        let mut seen = Vec::new();
        assert_eq!(
            fresh_bytes(&path, 5, false, &mut |fresh| seen.extend_from_slice(fresh)),
            0
        );
        assert!(seen.is_empty());
        // And a log that is not there yet is not an error either: the
        // supervisor may still be creating it.
        assert_eq!(
            fresh_bytes(
                &dir.path().join("nothing"),
                0,
                false,
                &mut |_| unreachable!()
            ),
            0
        );
    }
}
