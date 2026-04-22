use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::parser::{BrowserSliceSpec, CodeBlock, DirConfig, ExecConfig, Frontmatter};
use crate::sidecar::{PauseInfo, RestoreArgs, Sidecar, SliceCallbackContext};

#[derive(Debug)]
pub struct BlockResult {
    pub block_index: usize,
    pub language: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
    /// Machine-readable failure category. `None` for a successful block.
    /// Agents branch on this instead of regex-parsing stderr. Stable tokens:
    ///   `spawn_not_found`  — ENOENT launching the runtime binary
    ///   `spawn_failed`     — other OS-level spawn failure
    ///   `nonzero_exit`     — block ran but exited non-zero
    ///   `signal_killed`    — killed by a signal (exit code ≥ 128 on Unix)
    ///   `timeout`          — reached the block-level timeout
    ///   `sandbox_failed`   — `requires:` sandbox setup or build failed
    ///   `read_error`       — workbook file unreadable (run_single_collect path)
    pub error_type: Option<String>,
    /// Captured stdout may be truncated — block was killed by a timeout or
    /// signal before the runtime emitted its completion sentinel. The buffer
    /// still contains everything written up to the kill, which is often
    /// enough to diagnose a hung block.
    pub stdout_partial: bool,
    /// Same as `stdout_partial` but for stderr.
    pub stderr_partial: bool,
}

impl BlockResult {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// Post-hoc classification: if the block failed and no specific
    /// `error_type` has been set (e.g. by a spawn-error path), infer one
    /// from the exit code. A partial-output flag pins it to `timeout` since
    /// that's the only reason wb truncates its own collection. Idempotent.
    pub fn auto_classify(&mut self) {
        if self.error_type.is_some() {
            return;
        }
        if self.stdout_partial || self.stderr_partial {
            self.error_type = Some("timeout".to_string());
            return;
        }
        if self.exit_code != 0 {
            let kind = classify_exit(self.exit_code);
            if !kind.is_empty() {
                self.error_type = Some(kind.to_string());
            }
        }
    }
}

/// Categorize a non-zero exit into a stable error-type token.
pub fn classify_exit(exit_code: i32) -> &'static str {
    if exit_code == 0 {
        return "";
    }
    // Unix convention: values ≥ 128 encode a signal (e.g. 137 = SIGKILL, 143 = SIGTERM).
    #[cfg(unix)]
    if exit_code >= 128 && exit_code < 128 + 64 {
        return "signal_killed";
    }
    "nonzero_exit"
}

pub struct ExecutionContext {
    pub env: HashMap<String, String>,
    pub working_dir: String,
    pub venv: Option<String>,
    pub default_runtime: Option<String>,
    pub exec_config: Option<ExecConfig>,
    pub dir_config: Option<DirConfig>,
    pub quiet: bool,
    pub vars: HashMap<String, String>,
    pub redact_values: Vec<String>,
    /// Default timeout for one execution of a block. Per-block overrides live
    /// in `Session::block_timeouts` (keyed by block index) and the session
    /// applies them before calling `execute_block` by mutating this field.
    pub block_timeout: Duration,
}

impl ExecutionContext {
    pub fn from_frontmatter(frontmatter: &Frontmatter, file_path: &str) -> Self {
        let working_dir = Path::new(file_path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        let mut env = HashMap::new();
        if let Some(ref fm_env) = frontmatter.env {
            env.extend(fm_env.clone());
        }

        ExecutionContext {
            env,
            working_dir,
            venv: frontmatter.venv.clone(),
            default_runtime: frontmatter.runtime.clone(),
            exec_config: frontmatter.exec.clone(),
            dir_config: frontmatter.working_dir.clone(),
            quiet: false,
            vars: frontmatter.vars.clone().unwrap_or_default(),
            redact_values: Vec::new(),
            block_timeout: DEFAULT_BLOCK_TIMEOUT,
        }
    }
}

pub fn substitute_vars(code: &str, vars: &HashMap<String, String>) -> String {
    if vars.is_empty() {
        return code.to_string();
    }
    let mut result = code.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

pub fn redact_output(text: &str, values: &[String]) -> String {
    if values.is_empty() {
        return text.to_string();
    }
    let mut result = text.to_string();
    for val in values {
        if !val.is_empty() {
            result = result.replace(val, "***");
        }
    }
    result
}

/// Resolve exec for a given normalized language.
/// - Global: prefix (prepended before the runtime command)
/// - Per-language: replacement (replaces the runtime program entirely)
enum ExecMode {
    /// Prefix args before the original program: docker exec ctr python3 ...
    Prefix(Vec<String>),
    /// Replace the program entirely: uv run python ...
    Replace(Vec<String>),
}

fn resolve_exec(config: &Option<ExecConfig>, lang: &str) -> Option<ExecMode> {
    config.as_ref().and_then(|c| match c {
        ExecConfig::Global(s) => {
            let parts: Vec<String> = s.split_whitespace().map(|w| w.to_string()).collect();
            if parts.is_empty() { None } else { Some(ExecMode::Prefix(parts)) }
        }
        ExecConfig::PerLanguage(map) => {
            map.get(lang).map(|s| {
                let parts: Vec<String> = s.split_whitespace().map(|w| w.to_string()).collect();
                ExecMode::Replace(parts)
            })
        }
    })
}

/// Resolve working directory for a given language.
/// Per-language overrides the default; paths are resolved relative to the base working_dir.
fn resolve_working_dir(config: &Option<DirConfig>, lang: &str, base: &str) -> String {
    let override_dir = config.as_ref().and_then(|c| match c {
        DirConfig::Global(s) => Some(s.as_str()),
        DirConfig::PerLanguage(map) => map.get(lang).map(|s| s.as_str()),
    });
    match override_dir {
        Some(d) if Path::new(d).is_absolute() => d.to_string(),
        Some(d) => Path::new(base).join(d).to_string_lossy().to_string(),
        None => base.to_string(),
    }
}

/// Build a Command from exec resolution + original program
fn build_command(exec: Option<&ExecMode>, program: &str) -> Command {
    match exec {
        Some(ExecMode::Prefix(parts)) => {
            let mut cmd = Command::new(&parts[0]);
            for p in &parts[1..] { cmd.arg(p); }
            cmd.arg(program);
            cmd
        }
        Some(ExecMode::Replace(parts)) => {
            let mut cmd = Command::new(&parts[0]);
            for p in &parts[1..] { cmd.arg(p); }
            cmd
        }
        None => Command::new(program),
    }
}

// ─── Persistent session ───────────────────────────────────────────────

const SENTINEL_PREFIX: &str = "__WB_DONE_";
const SENTINEL_SUFFIX: &str = "__";
const CODE_END_MARKER: &str = "__WB_CODE_END__";
pub const DEFAULT_BLOCK_TIMEOUT: Duration = Duration::from_secs(300);

const PYTHON_HARNESS: &str = r#"import sys, traceback
_wb_g = {'__builtins__': __builtins__, '__name__': '__main__'}
while True:
    _wb_lines = []
    while True:
        _wb_line = sys.stdin.readline()
        if not _wb_line:
            sys.exit(0)
        if _wb_line.rstrip('\n') == '__WB_CODE_END__':
            break
        _wb_lines.append(_wb_line)
    _wb_code = ''.join(_wb_lines)
    _wb_rc = 0
    try:
        exec(compile(_wb_code, '<wb>', 'exec'), _wb_g)
    except SystemExit as _wb_e:
        _wb_rc = _wb_e.code if isinstance(_wb_e.code, int) else 1
    except:
        traceback.print_exc()
        _wb_rc = 1
    sys.stdout.flush()
    sys.stdout.write('\n__WB_DONE_' + str(_wb_rc) + '__\n')
    sys.stdout.flush()
    sys.stderr.flush()
    sys.stderr.write('\n__WB_DONE_' + str(_wb_rc) + '__\n')
    sys.stderr.flush()
"#;

const NODE_HARNESS: &str = r#"const vm = require('vm');
const ctx = vm.createContext(Object.assign(Object.create(globalThis), {
  console, require, process, Buffer,
  setTimeout, setInterval, clearTimeout, clearInterval
}));
const rl = require('readline').createInterface({ input: process.stdin, terminal: false });
let lines = [];
rl.on('line', (l) => {
  if (l === '__WB_CODE_END__') {
    const code = lines.join('\n');
    lines = [];
    let rc = 0;
    try { vm.runInContext(code, ctx, { filename: '<wb>' }); }
    catch (e) { console.error(e); rc = 1; }
    process.stdout.write('\n__WB_DONE_' + rc + '__\n');
    process.stderr.write('\n__WB_DONE_' + rc + '__\n');
  } else {
    lines.push(l);
  }
});
"#;

const RUBY_HARNESS: &str = r##"$__wb_binding = binding
loop do
  lines = []
  while (line = $stdin.gets)
    break if line.chomp == '__WB_CODE_END__'
    lines << line
  end
  break if lines.empty?
  code = lines.join
  rc = 0
  begin
    eval(code, $__wb_binding, '<wb>')
  rescue SystemExit => e
    rc = e.status
  rescue => e
    $stderr.puts "#{e.class}: #{e.message}"
    e.backtrace.each { |l| $stderr.puts "  #{l}" }
    rc = 1
  end
  $stdout.write("\n__WB_DONE_#{rc}__\n")
  $stdout.flush
  $stderr.write("\n__WB_DONE_#{rc}__\n")
  $stderr.flush
end
"##;

struct PersistentProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout_rx: Receiver<String>,
    stderr_rx: Receiver<String>,
}

impl Drop for PersistentProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Session {
    processes: HashMap<String, PersistentProcess>,
    ctx: ExecutionContext,
    browser_sidecar: Option<Sidecar>,
}

impl Session {
    pub fn new(ctx: ExecutionContext) -> Self {
        Session {
            processes: HashMap::new(),
            ctx,
            browser_sidecar: None,
        }
    }

    pub fn set_quiet(&mut self, quiet: bool) {
        self.ctx.quiet = quiet;
    }

    /// Override the per-block timeout for the *next* `execute_block` call.
    /// The caller is expected to reset it back to the default after the
    /// block finishes, since the session object is long-lived.
    pub fn set_block_timeout(&mut self, timeout: Duration) {
        self.ctx.block_timeout = timeout;
    }

    pub fn set_env(&mut self, key: String, value: String) {
        self.ctx.env.insert(key, value);
    }

    pub fn remove_env(&mut self, key: &str) {
        self.ctx.env.remove(key);
    }

    /// Snapshot of the env that will be injected into the next block. Used
    /// by the run loop to evaluate `when=` / `skip_if=` expressions against
    /// the same state a block would otherwise see.
    pub fn env(&self) -> &HashMap<String, String> {
        &self.ctx.env
    }

    /// Inject a small code snippet into all running persistent sessions to unset an env var.
    /// This ensures the already-spawned processes reflect the removal.
    pub fn unset_env_in_sessions(&mut self, key: &str) {
        let shells = ["bash", "sh", "zsh"];
        for lang in &shells {
            if self.processes.contains_key(*lang) {
                let block = CodeBlock {
                    language: lang.to_string(),
                    code: format!("unset {}", key),
                    line_number: 0,
                    skip_execution: false,
                    silent: false,
                    when: None,
                    skip_if: None,
                };
                let saved_quiet = self.ctx.quiet;
                self.ctx.quiet = true;
                self.execute_block(&block, 0);
                self.ctx.quiet = saved_quiet;
            }
        }
        if self.processes.contains_key("python") {
            let block = CodeBlock {
                language: "python".to_string(),
                code: format!("import os; os.environ.pop('{}', None)", key),
                line_number: 0,
                skip_execution: false,
                silent: false,
                when: None,
                skip_if: None,
            };
            let saved_quiet = self.ctx.quiet;
            self.ctx.quiet = true;
            self.execute_block(&block, 0);
            self.ctx.quiet = saved_quiet;
        }
        if self.processes.contains_key("node") {
            let block = CodeBlock {
                language: "node".to_string(),
                code: format!("delete process.env['{}']", key),
                line_number: 0,
                skip_execution: false,
                silent: false,
                when: None,
                skip_if: None,
            };
            let saved_quiet = self.ctx.quiet;
            self.ctx.quiet = true;
            self.execute_block(&block, 0);
            self.ctx.quiet = saved_quiet;
        }
        if self.processes.contains_key("ruby") {
            let block = CodeBlock {
                language: "ruby".to_string(),
                code: format!("ENV.delete('{}')", key),
                line_number: 0,
                skip_execution: false,
                silent: false,
                when: None,
                skip_if: None,
            };
            let saved_quiet = self.ctx.quiet;
            self.ctx.quiet = true;
            self.execute_block(&block, 0);
            self.ctx.quiet = saved_quiet;
        }
    }

    pub fn execute_block(&mut self, block: &CodeBlock, index: usize) -> BlockResult {
        let start = Instant::now();
        let lang = normalize_language(&block.language, &self.ctx.default_runtime);

        // Fall back to one-shot for unsupported languages
        if !supports_session(&lang) {
            let mut r = execute_block_oneshot(block, index, &self.ctx);
            r.auto_classify();
            return r;
        }

        // Ensure persistent process exists
        if !self.processes.contains_key(&lang) {
            let work_dir = resolve_working_dir(&self.ctx.dir_config, &lang, &self.ctx.working_dir);
            match spawn_persistent(&lang, &self.ctx.env, &work_dir, &self.ctx.venv, &self.ctx.exec_config) {
                Ok(proc) => {
                    self.processes.insert(lang.clone(), proc);
                }
                Err(e) => {
                    // spawn_persistent now returns spawn_error_message text
                    // for ENOENT; propagate the type so agents branch on it.
                    let kind = if e.starts_with('`') && e.contains("not found on PATH") {
                        "spawn_not_found"
                    } else {
                        "spawn_failed"
                    };
                    return BlockResult {
                        block_index: index,
                        language: block.language.clone(),
                        stdout: String::new(),
                        stderr: e,
                        exit_code: 127,
                        duration: start.elapsed(),
                        error_type: Some(kind.to_string()),
                        stdout_partial: false,
                        stderr_partial: false,
                    };
                }
            }
        }

        // Send code and collect output (scoped to release process borrow)
        let quiet = self.ctx.quiet;
        let code = substitute_vars(&block.code, &self.ctx.vars);
        let timeout = self.ctx.block_timeout;
        let (ok, out) = {
            let process = self.processes.get_mut(&lang).unwrap();
            match send_code(process, &lang, &code) {
                Ok(()) => {
                    let collected = collect_output(process, quiet, timeout);
                    (true, collected)
                }
                Err(e) => (
                    false,
                    CollectedOutput {
                        stdout: String::new(),
                        stderr: format!("Failed to send code: {}", e),
                        exit_code: 1,
                        stdout_partial: false,
                        stderr_partial: false,
                    },
                ),
            }
        };

        // Remove dead processes — a timed-out child still owns the session stdin,
        // so subsequent blocks would deadlock. Drop triggers kill() via Drop impl.
        if !ok || out.exit_code == -1 || out.stdout_partial || out.stderr_partial {
            self.processes.remove(&lang);
        }

        let mut result = BlockResult {
            block_index: index,
            language: block.language.clone(),
            stdout: redact_output(&out.stdout, &self.ctx.redact_values),
            stderr: redact_output(&out.stderr, &self.ctx.redact_values),
            exit_code: out.exit_code,
            duration: start.elapsed(),
            error_type: None,
            stdout_partial: out.stdout_partial,
            stderr_partial: out.stderr_partial,
        };
        result.auto_classify();
        result
    }

    /// Dispatch a browser slice to the long-lived sidecar. Spawns the sidecar
    /// lazily on the first call and reuses it for the rest of the run.
    /// Returns `(BlockResult, Some(PauseInfo))` when the slice paused for a
    /// human; main is expected to persist a pending descriptor and exit 42.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_browser_slice(
        &mut self,
        spec: &BrowserSliceSpec,
        index: usize,
        ctx: &SliceCallbackContext,
        restore: Option<&RestoreArgs>,
    ) -> (BlockResult, Option<PauseInfo>) {
        let start = Instant::now();

        if self.browser_sidecar.is_none() {
            // Resolve vendor: runbook's `vars.browser_service` declares which
            // sidecar provider this workbook is designed to drive. We project
            // it into WB_BROWSER_VENDOR before spawning so the sidecar's
            // boot-time provider selection lands on the right vendor without
            // requiring the operator to also set the env var. Conflicts
            // (env says X, runbook says Y) fail fast with a clear message —
            // silent precedence rules cause more confusion than enforcement.
            let mut spawn_env = self.ctx.env.clone();
            if let Some(declared) = self.ctx.vars.get("browser_service") {
                match self.ctx.env.get("WB_BROWSER_VENDOR") {
                    Some(env_v) if env_v != declared => {
                        let err = format!(
                            "browser vendor mismatch: runbook declares browser_service=\"{}\" but env WB_BROWSER_VENDOR=\"{}\". Unset one or align them.",
                            declared, env_v
                        );
                        if !self.ctx.quiet {
                            crate::output::print_stderr_dim(&err);
                        }
                        return (
                            BlockResult {
                                block_index: index,
                                language: "browser".to_string(),
                                stdout: String::new(),
                                stderr: err,
                                exit_code: 1,
                                duration: start.elapsed(),
                                error_type: Some("vendor_mismatch".to_string()),
                                stdout_partial: false,
                                stderr_partial: false,
                            },
                            None,
                        );
                    }
                    _ => {
                        spawn_env.insert(
                            "WB_BROWSER_VENDOR".to_string(),
                            declared.clone(),
                        );
                    }
                }
            }
            match Sidecar::spawn(&spawn_env, &self.ctx.working_dir) {
                Ok(sc) => self.browser_sidecar = Some(sc),
                Err(e) => {
                    if !self.ctx.quiet {
                        crate::output::print_stderr_dim(&e);
                    }
                    return (
                        BlockResult {
                            block_index: index,
                            language: "browser".to_string(),
                            stdout: String::new(),
                            stderr: e,
                            exit_code: 127,
                            duration: start.elapsed(),
                            error_type: Some("spawn_not_found".to_string()),
                            stdout_partial: false,
                            stderr_partial: false,
                        },
                        None,
                    );
                }
            }
        }

        // Run `{{var}}` substitution on slice fields that are session-scoped
        // (set on the slice envelope, not inside verbs). Verbs themselves are
        // forwarded opaquely — the JS sidecar already does its own
        // `{{ env.X }}` / `{{ artifacts.X }}` expansion against verb args
        // so it can pick up values written by intervening cells. Session
        // fields like `profile` are bound at slice dispatch and don't
        // change mid-slice, so we resolve them here against ctx.vars
        // (frontmatter `vars:` + any signal-bound vars merged in).
        let mut effective_spec;
        let resolved_spec = if spec.profile.is_some() && !self.ctx.vars.is_empty() {
            effective_spec = spec.clone();
            if let Some(p) = effective_spec.profile.as_ref() {
                effective_spec.profile = Some(substitute_vars(p, &self.ctx.vars));
            }
            &effective_spec
        } else {
            spec
        };

        let outcome = self
            .browser_sidecar
            .as_mut()
            .unwrap()
            .run_slice(resolved_spec, self.ctx.quiet, ctx, restore);

        let mut block = BlockResult {
            block_index: index,
            language: "browser".to_string(),
            stdout: redact_output(&outcome.stdout, &self.ctx.redact_values),
            stderr: redact_output(&outcome.stderr, &self.ctx.redact_values),
            exit_code: outcome.exit_code,
            duration: start.elapsed(),
            error_type: None,
            stdout_partial: false,
            stderr_partial: false,
        };
        block.auto_classify();
        (block, outcome.pause)
    }
}

fn normalize_language(lang: &str, default_runtime: &Option<String>) -> String {
    match lang.to_lowercase().as_str() {
        "python" | "python3" | "py" => "python".to_string(),
        "bash" | "shell" => "bash".to_string(),
        "sh" => "sh".to_string(),
        "zsh" => "zsh".to_string(),
        "node" | "javascript" | "js" => "node".to_string(),
        "ruby" | "rb" => "ruby".to_string(),
        other => {
            if let Some(ref default) = default_runtime {
                normalize_language(default, &None)
            } else {
                other.to_string()
            }
        }
    }
}

fn supports_session(lang: &str) -> bool {
    matches!(lang, "python" | "bash" | "sh" | "zsh" | "node" | "ruby")
}

fn spawn_persistent(
    lang: &str,
    env: &HashMap<String, String>,
    working_dir: &str,
    venv: &Option<String>,
    exec_config: &Option<ExecConfig>,
) -> Result<PersistentProcess, String> {
    let mut cmd_env = env.clone();

    let (program, args): (&str, Vec<String>) = match lang {
        "python" => {
            if let Some(ref venv_path) = venv {
                let venv_abs = if Path::new(venv_path).is_absolute() {
                    venv_path.clone()
                } else {
                    Path::new(working_dir)
                        .join(venv_path)
                        .to_string_lossy()
                        .to_string()
                };
                let bin_dir = Path::new(&venv_abs).join("bin");
                let current_path = std::env::var("PATH").unwrap_or_default();
                cmd_env.insert(
                    "PATH".to_string(),
                    format!("{}:{}", bin_dir.display(), current_path),
                );
                cmd_env.insert("VIRTUAL_ENV".to_string(), venv_abs);
            }
            (
                "python3",
                vec![
                    "-u".to_string(),
                    "-c".to_string(),
                    PYTHON_HARNESS.to_string(),
                ],
            )
        }
        "bash" => ("bash", vec![]),
        "sh" => ("sh", vec![]),
        "zsh" => ("zsh", vec![]),
        "node" => (
            "node",
            vec!["-e".to_string(), NODE_HARNESS.to_string()],
        ),
        "ruby" => (
            "ruby",
            vec!["-e".to_string(), RUBY_HARNESS.to_string()],
        ),
        _ => return Err(format!("No session support for {}", lang)),
    };

    // exec: "docker exec mycontainer"     → docker exec mycontainer python3 -u -c <harness>
    // exec: { python: "uv run python" }  → uv run python -u -c <harness>
    let exec = resolve_exec(exec_config, lang);
    let mut cmd = build_command(exec.as_ref(), program);
    let effective_program: String = match exec.as_ref() {
        Some(ExecMode::Prefix(parts)) | Some(ExecMode::Replace(parts)) => parts[0].clone(),
        None => program.to_string(),
    };
    for arg in &args {
        cmd.arg(arg);
    }
    cmd.current_dir(working_dir);
    for (k, v) in &cmd_env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| spawn_error_message(&effective_program, lang, &e))?;

    let stdin = BufWriter::new(child.stdin.take().unwrap());
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            if stdout_tx.send(line).is_err() {
                break;
            }
        }
    });

    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
            if stderr_tx.send(line).is_err() {
                break;
            }
        }
    });

    Ok(PersistentProcess {
        child,
        stdin,
        stdout_rx,
        stderr_rx,
    })
}

fn send_code(
    process: &mut PersistentProcess,
    lang: &str,
    code: &str,
) -> Result<(), std::io::Error> {
    match lang {
        "bash" | "sh" | "zsh" => {
            // Run user code in a brace group (current shell) so that
            // `export`, `cd`, function defs, etc. persist across blocks.
            // Tradeoff: `exit` or `set -e` in user code will end the session.
            writeln!(process.stdin, "{{")?;
            writeln!(process.stdin, "{}", code)?;
            writeln!(process.stdin, "}}")?;
            writeln!(process.stdin, "__wb_rc=$?")?;
            writeln!(process.stdin, "echo")?;
            writeln!(
                process.stdin,
                "echo \"{}${{__wb_rc}}{}\"",
                SENTINEL_PREFIX, SENTINEL_SUFFIX
            )?;
            writeln!(process.stdin, "echo >&2")?;
            writeln!(
                process.stdin,
                "echo \"{}${{__wb_rc}}{}\" >&2",
                SENTINEL_PREFIX, SENTINEL_SUFFIX
            )?;
            process.stdin.flush()?;
        }
        _ => {
            // Harness-based: send code lines then end marker
            for line in code.lines() {
                writeln!(process.stdin, "{}", line)?;
            }
            writeln!(process.stdin, "{}", CODE_END_MARKER)?;
            process.stdin.flush()?;
        }
    }
    Ok(())
}

/// Collected streams plus flags indicating whether each was truncated by a
/// timeout rather than a clean sentinel. Both partial flags imply the child
/// is probably still running and will be killed when the session drops.
struct CollectedOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
    stdout_partial: bool,
    stderr_partial: bool,
}

fn collect_output(process: &PersistentProcess, quiet: bool, timeout: Duration) -> CollectedOutput {
    // Read stdout first, then stderr. Safe because the child writes
    // stdout sentinel before stderr sentinel. If stderr fills during
    // stdout reading, the child blocks AFTER writing stdout sentinel.
    // Once we drain stderr, the child unblocks and writes stderr sentinel.
    let (stdout, stdout_code, stdout_timed_out) =
        collect_until_sentinel(&process.stdout_rx, quiet, false, timeout);
    // If stdout timed out, there's no point waiting the full timeout again
    // for the stderr sentinel — the child is already considered dead. Fall
    // through with a near-zero window so we drain anything already buffered.
    let stderr_timeout = if stdout_timed_out {
        Duration::from_millis(100)
    } else {
        timeout
    };
    let (stderr, stderr_code, stderr_timed_out) =
        collect_until_sentinel(&process.stderr_rx, quiet, true, stderr_timeout);

    let exit_code = stdout_code.or(stderr_code).unwrap_or(0);
    CollectedOutput {
        stdout,
        stderr,
        exit_code,
        stdout_partial: stdout_timed_out,
        stderr_partial: stderr_timed_out,
    }
}

/// Returns (buffered_lines, exit_code_if_sentinel_seen, timed_out).
/// `timed_out` is true iff we gave up waiting for a line without ever seeing
/// the sentinel — i.e. the buffer is partial, not the terminal state of the
/// block.
fn collect_until_sentinel(
    rx: &Receiver<String>,
    quiet: bool,
    is_stderr: bool,
    timeout: Duration,
) -> (String, Option<i32>, bool) {
    let mut lines = Vec::new();
    let exit_code;
    let mut timed_out = false;

    loop {
        match rx.recv_timeout(timeout) {
            Ok(line) => {
                if let Some(code) = parse_sentinel(&line) {
                    exit_code = Some(code);
                    break;
                }
                if !quiet {
                    if is_stderr {
                        crate::output::print_stderr_dim(&line);
                    } else {
                        println!("{}", line);
                    }
                }
                lines.push(line);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                lines.push("[wb: block timed out]".to_string());
                exit_code = Some(-1);
                timed_out = true;
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                exit_code = Some(-1);
                break;
            }
        }
    }

    // Trim trailing empty lines (artifact of sentinel newline prefix)
    while lines.last().map_or(false, |l| l.is_empty()) {
        lines.pop();
    }

    (lines.join("\n"), exit_code, timed_out)
}

fn parse_sentinel(line: &str) -> Option<i32> {
    let trimmed = line.trim();
    if trimmed.starts_with(SENTINEL_PREFIX) && trimmed.ends_with(SENTINEL_SUFFIX) {
        let inner = &trimmed[SENTINEL_PREFIX.len()..trimmed.len() - SENTINEL_SUFFIX.len()];
        inner.parse().ok()
    } else {
        None
    }
}

// ─── One-shot execution (fallback for languages without sessions) ─────

pub fn execute_block_oneshot(
    block: &CodeBlock,
    index: usize,
    ctx: &ExecutionContext,
) -> BlockResult {
    let start = Instant::now();

    let substituted = substitute_vars(&block.code, &ctx.vars);
    let (program, args, code) = resolve_runtime(&block.language, &substituted, ctx);

    let lang = normalize_language(&block.language, &ctx.default_runtime);
    let exec = resolve_exec(&ctx.exec_config, &lang);
    let mut cmd = build_command(exec.as_ref(), &program);
    // Effective program name — the binary the OS will actually try to exec.
    // When `exec:` is set this is the wrapper (e.g. `docker`, `uv`), not the
    // language runtime. Preserves agent-actionable errors on ENOENT.
    let effective_program: String = match exec.as_ref() {
        Some(ExecMode::Prefix(parts)) | Some(ExecMode::Replace(parts)) => parts[0].clone(),
        None => program.clone(),
    };
    for arg in &args {
        cmd.arg(arg);
    }

    let work_dir = resolve_working_dir(&ctx.dir_config, &lang, &ctx.working_dir);
    cmd.current_dir(&work_dir);

    for (key, val) in &ctx.env {
        cmd.env(key, val);
    }

    if let Some(ref venv) = ctx.venv {
        if is_python_language(&block.language) {
            let venv_path = if Path::new(venv).is_absolute() {
                venv.clone()
            } else {
                Path::new(&ctx.working_dir)
                    .join(venv)
                    .to_string_lossy()
                    .to_string()
            };
            let bin_dir = Path::new(&venv_path).join("bin");
            let current_path = std::env::var("PATH").unwrap_or_default();
            cmd.env(
                "PATH",
                format!("{}:{}", bin_dir.display(), current_path),
            );
            cmd.env("VIRTUAL_ENV", &venv_path);
        }
    }

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let result = cmd.spawn();
    match result {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(code.as_bytes());
                drop(stdin);
            }

            // Read stdout and stderr concurrently to avoid deadlock when
            // the child fills the OS pipe buffer on one stream.
            let quiet = ctx.quiet;
            let stdout_thread = child.stdout.take().map(|out| {
                thread::spawn(move || {
                    let reader = BufReader::new(out);
                    let mut buf = String::new();
                    for line in reader.lines().flatten() {
                        if !quiet {
                            println!("{}", line);
                        }
                        buf.push_str(&line);
                        buf.push('\n');
                    }
                    buf
                })
            });
            let stderr_thread = child.stderr.take().map(|err| {
                thread::spawn(move || {
                    let reader = BufReader::new(err);
                    let mut buf = String::new();
                    for line in reader.lines().flatten() {
                        if !quiet {
                            crate::output::print_stderr_dim(&line);
                        }
                        buf.push_str(&line);
                        buf.push('\n');
                    }
                    buf
                })
            });

            let stdout_buf = stdout_thread
                .map(|t| t.join().unwrap_or_default())
                .unwrap_or_default();
            let stderr_buf = stderr_thread
                .map(|t| t.join().unwrap_or_default())
                .unwrap_or_default();

            let status = child.wait();
            let exit_code = status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);

            let mut r = BlockResult {
                block_index: index,
                language: block.language.clone(),
                stdout: redact_output(stdout_buf.trim_end(), &ctx.redact_values),
                stderr: redact_output(stderr_buf.trim_end(), &ctx.redact_values),
                exit_code,
                duration: start.elapsed(),
                error_type: None,
                stdout_partial: false,
                stderr_partial: false,
            };
            r.auto_classify();
            r
        }
        Err(e) => {
            let kind = if e.kind() == std::io::ErrorKind::NotFound {
                "spawn_not_found"
            } else {
                "spawn_failed"
            };
            BlockResult {
                block_index: index,
                language: block.language.clone(),
                stdout: String::new(),
                stderr: spawn_error_message(&effective_program, &block.language, &e),
                exit_code: 127,
                duration: start.elapsed(),
                error_type: Some(kind.to_string()),
                stdout_partial: false,
                stderr_partial: false,
            }
        }
    }
}

/// Expand a spawn failure into an actionable message for agents. ENOENT is
/// by far the most common and the most recoverable — surface it explicitly
/// with install hints and the `exec:` escape hatch.
fn spawn_error_message(program: &str, language: &str, err: &std::io::Error) -> String {
    if err.kind() == std::io::ErrorKind::NotFound {
        let install_hint = match language.to_lowercase().as_str() {
            "python" | "python3" | "py" => "install Python (e.g. `brew install python` or `apt install python3`)",
            "node" | "javascript" | "js" => "install Node.js (e.g. `brew install node` or via `nvm`)",
            "ruby" | "rb" => "install Ruby (e.g. `brew install ruby` or via `rbenv`)",
            "go" => "install Go (e.g. `brew install go`)",
            "r" => "install R (e.g. `brew install r`)",
            "swift" => "install Swift (macOS: Xcode command-line tools)",
            "php" => "install PHP (e.g. `brew install php`)",
            "lua" => "install Lua (e.g. `brew install lua`)",
            "perl" => "Perl is usually preinstalled; check your PATH",
            _ => "install the runtime or override with frontmatter `exec:` (e.g. `exec: docker exec my-container`)",
        };
        format!(
            "`{}` not found on PATH — {}. Workbook block language: `{}`. \
             To run inside a container instead, set frontmatter `exec:` — see `wb inspect` for resolution details.",
            program, install_hint, language
        )
    } else {
        format!("Failed to spawn {}: {}", program, err)
    }
}

fn resolve_runtime(
    language: &str,
    code: &str,
    ctx: &ExecutionContext,
) -> (String, Vec<String>, String) {
    let lang = language.to_lowercase();
    match lang.as_str() {
        "python" | "python3" | "py" => (
            "python3".to_string(),
            vec!["-".to_string()],
            code.to_string(),
        ),
        "bash" | "shell" => (
            "bash".to_string(),
            vec!["-s".to_string()],
            code.to_string(),
        ),
        "sh" => ("sh".to_string(), vec!["-s".to_string()], code.to_string()),
        "zsh" => (
            "zsh".to_string(),
            vec!["-s".to_string()],
            code.to_string(),
        ),
        "node" | "javascript" | "js" => (
            "node".to_string(),
            vec!["-e".to_string(), code.to_string()],
            String::new(),
        ),
        "ruby" | "rb" => (
            "ruby".to_string(),
            vec!["-e".to_string(), code.to_string()],
            String::new(),
        ),
        "perl" => (
            "perl".to_string(),
            vec!["-e".to_string(), code.to_string()],
            String::new(),
        ),
        "php" => (
            "php".to_string(),
            vec!["-r".to_string(), code.to_string()],
            String::new(),
        ),
        "lua" => (
            "lua".to_string(),
            vec!["-e".to_string(), code.to_string()],
            String::new(),
        ),
        "r" => (
            "Rscript".to_string(),
            vec!["-e".to_string(), code.to_string()],
            String::new(),
        ),
        "swift" => (
            "swift".to_string(),
            vec!["-e".to_string(), code.to_string()],
            String::new(),
        ),
        "go" => {
            // go run requires a filename, not stdin
            let tmp = std::env::temp_dir().join("wb_block.go");
            let _ = std::fs::write(&tmp, code);
            (
                "go".to_string(),
                vec!["run".to_string(), tmp.to_string_lossy().to_string()],
                String::new(),
            )
        }
        _other => {
            if let Some(ref default) = ctx.default_runtime {
                resolve_runtime(
                    default,
                    code,
                    &ExecutionContext {
                        default_runtime: None,
                        env: ctx.env.clone(),
                        working_dir: ctx.working_dir.clone(),
                        venv: ctx.venv.clone(),
                        exec_config: ctx.exec_config.clone(),
                        dir_config: ctx.dir_config.clone(),
                        quiet: ctx.quiet,
                        vars: ctx.vars.clone(),
                        redact_values: ctx.redact_values.clone(),
                        block_timeout: ctx.block_timeout,
                    },
                )
            } else {
                ("bash".to_string(), vec!["-s".to_string()], code.to_string())
            }
        }
    }
}

fn is_python_language(lang: &str) -> bool {
    matches!(lang.to_lowercase().as_str(), "python" | "python3" | "py")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_vars_basic() {
        let mut vars = HashMap::new();
        vars.insert("cluster".to_string(), "prod".to_string());
        vars.insert("region".to_string(), "us-east-1".to_string());
        assert_eq!(
            substitute_vars("echo {{cluster}} in {{region}}", &vars),
            "echo prod in us-east-1"
        );
    }

    #[test]
    fn test_substitute_vars_empty() {
        let vars = HashMap::new();
        assert_eq!(substitute_vars("echo {{cluster}}", &vars), "echo {{cluster}}");
    }

    #[test]
    fn test_substitute_vars_repeated() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "42".to_string());
        assert_eq!(substitute_vars("{{x}} and {{x}}", &vars), "42 and 42");
    }

    #[test]
    fn test_redact_output_basic() {
        let values = vec!["secret123".to_string(), "password456".to_string()];
        assert_eq!(
            redact_output("token: secret123, pass: password456", &values),
            "token: ***, pass: ***"
        );
    }

    #[test]
    fn test_redact_output_empty() {
        let values: Vec<String> = vec![];
        assert_eq!(redact_output("token: secret123", &values), "token: secret123");
    }

    #[test]
    fn test_redact_skips_empty_values() {
        let values = vec!["".to_string(), "real".to_string()];
        assert_eq!(redact_output("real value", &values), "*** value");
    }

    fn bash_ctx() -> ExecutionContext {
        ExecutionContext {
            env: HashMap::new(),
            working_dir: ".".to_string(),
            venv: None,
            default_runtime: Some("bash".to_string()),
            exec_config: None,
            dir_config: None,
            quiet: true,
            vars: HashMap::new(),
            redact_values: Vec::new(),
            block_timeout: DEFAULT_BLOCK_TIMEOUT,
        }
    }

    fn code_block(code: &str) -> CodeBlock {
        CodeBlock {
            language: "bash".to_string(),
            code: code.to_string(),
            line_number: 0,
            skip_execution: false,
            silent: false,
            when: None,
            skip_if: None,
        }
    }

    #[test]
    fn test_bash_session_persists_exports() {
        // Regression: exports in one bash block must be visible in the next.
        // Previously blocks ran in a subshell `(...)`, which isolated env.
        let mut session = Session::new(bash_ctx());
        let r1 = session.execute_block(&code_block("export WB_TEST_VAR=persisted"), 0);
        assert_eq!(r1.exit_code, 0);
        let r2 = session.execute_block(&code_block("echo \"$WB_TEST_VAR\""), 1);
        assert_eq!(r2.exit_code, 0);
        assert_eq!(r2.stdout.trim(), "persisted");
    }

    #[test]
    fn test_bash_session_persists_cd() {
        // Regression: `cd` in one block must persist to the next.
        let mut session = Session::new(bash_ctx());
        let r1 = session.execute_block(&code_block("cd /tmp"), 0);
        assert_eq!(r1.exit_code, 0);
        let r2 = session.execute_block(&code_block("pwd"), 1);
        assert_eq!(r2.exit_code, 0);
        // macOS symlinks /tmp -> /private/tmp; accept either
        let pwd = r2.stdout.trim();
        assert!(pwd == "/tmp" || pwd == "/private/tmp", "got: {}", pwd);
    }

    #[test]
    fn test_bash_session_survives_nonzero_exit() {
        // A failing command should not kill the persistent session.
        let mut session = Session::new(bash_ctx());
        let r1 = session.execute_block(&code_block("false"), 0);
        assert_eq!(r1.exit_code, 1);
        let r2 = session.execute_block(&code_block("echo alive"), 1);
        assert_eq!(r2.exit_code, 0);
        assert_eq!(r2.stdout.trim(), "alive");
    }

    #[test]
    fn test_partial_output_on_timeout() {
        // Block prints a line, flushes, then sleeps past the timeout. We
        // should see "before-sleep" preserved in stdout, both partial flags
        // set (timeout aborts both streams), and error_type=timeout.
        let mut ctx = bash_ctx();
        ctx.block_timeout = Duration::from_millis(500);
        let mut session = Session::new(ctx);
        let code = "echo before-sleep; sleep 5; echo after-sleep";
        let r = session.execute_block(&code_block(code), 0);
        assert!(
            r.stdout.contains("before-sleep"),
            "partial stdout should retain output emitted before the timeout, got: {:?}",
            r.stdout
        );
        assert!(
            !r.stdout.contains("after-sleep"),
            "stdout after the timeout must not appear, got: {:?}",
            r.stdout
        );
        assert!(r.stdout_partial, "stdout_partial should be true on timeout");
        assert!(r.stderr_partial, "stderr_partial should be true on timeout");
        assert_eq!(r.error_type.as_deref(), Some("timeout"));
    }

    #[test]
    fn test_auto_classify_partial_sets_timeout_over_exit_code() {
        // A block that timed out (partial) + nonzero exit should be classified
        // as `timeout`, not `nonzero_exit`. Partial is the stronger signal.
        let mut r = BlockResult {
            block_index: 0,
            language: "bash".to_string(),
            stdout: "partial".to_string(),
            stderr: String::new(),
            exit_code: 1,
            duration: Duration::from_millis(10),
            error_type: None,
            stdout_partial: true,
            stderr_partial: false,
        };
        r.auto_classify();
        assert_eq!(r.error_type.as_deref(), Some("timeout"));
    }
}
