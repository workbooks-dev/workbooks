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
}

impl BlockResult {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
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
const BLOCK_TIMEOUT: Duration = Duration::from_secs(300);

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

    pub fn set_env(&mut self, key: String, value: String) {
        self.ctx.env.insert(key, value);
    }

    pub fn remove_env(&mut self, key: &str) {
        self.ctx.env.remove(key);
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
            return execute_block_oneshot(block, index, &self.ctx);
        }

        // Ensure persistent process exists
        if !self.processes.contains_key(&lang) {
            let work_dir = resolve_working_dir(&self.ctx.dir_config, &lang, &self.ctx.working_dir);
            match spawn_persistent(&lang, &self.ctx.env, &work_dir, &self.ctx.venv, &self.ctx.exec_config) {
                Ok(proc) => {
                    self.processes.insert(lang.clone(), proc);
                }
                Err(e) => {
                    return BlockResult {
                        block_index: index,
                        language: block.language.clone(),
                        stdout: String::new(),
                        stderr: e,
                        exit_code: 127,
                        duration: start.elapsed(),
                    };
                }
            }
        }

        // Send code and collect output (scoped to release process borrow)
        let quiet = self.ctx.quiet;
        let code = substitute_vars(&block.code, &self.ctx.vars);
        let (ok, stdout, stderr, exit_code) = {
            let process = self.processes.get_mut(&lang).unwrap();
            match send_code(process, &lang, &code) {
                Ok(()) => {
                    let (stdout, stderr, exit_code) = collect_output(process, quiet);
                    (true, stdout, stderr, exit_code)
                }
                Err(e) => (false, String::new(), format!("Failed to send code: {}", e), 1),
            }
        };

        // Remove dead processes
        if !ok || exit_code == -1 {
            self.processes.remove(&lang);
        }

        BlockResult {
            block_index: index,
            language: block.language.clone(),
            stdout: redact_output(&stdout, &self.ctx.redact_values),
            stderr: redact_output(&stderr, &self.ctx.redact_values),
            exit_code,
            duration: start.elapsed(),
        }
    }

    /// Dispatch a browser slice to the long-lived sidecar.
    ///
    /// Gated behind `WB_EXPERIMENTAL_BROWSER=1`. Spawns the sidecar lazily on
    /// the first call and reuses it for the rest of the run. Returns
    /// `(BlockResult, Some(PauseInfo))` when the slice paused for a human;
    /// main is expected to persist a pending descriptor and exit 42.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_browser_slice(
        &mut self,
        spec: &BrowserSliceSpec,
        index: usize,
        ctx: &SliceCallbackContext,
        restore: Option<&RestoreArgs>,
    ) -> (BlockResult, Option<PauseInfo>) {
        let start = Instant::now();

        if std::env::var("WB_EXPERIMENTAL_BROWSER").ok().as_deref() != Some("1") {
            let msg = format!(
                "`browser` blocks are experimental. Set WB_EXPERIMENTAL_BROWSER=1 to enable. (L{})",
                spec.line_number
            );
            if !self.ctx.quiet {
                crate::output::print_stderr_dim(&msg);
            }
            return (
                BlockResult {
                    block_index: index,
                    language: "browser".to_string(),
                    stdout: String::new(),
                    stderr: msg,
                    exit_code: 1,
                    duration: start.elapsed(),
                },
                None,
            );
        }

        if self.browser_sidecar.is_none() {
            match Sidecar::spawn(&self.ctx.env, &self.ctx.working_dir) {
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
                        },
                        None,
                    );
                }
            }
        }

        let outcome = self
            .browser_sidecar
            .as_mut()
            .unwrap()
            .run_slice(spec, self.ctx.quiet, ctx, restore);

        let block = BlockResult {
            block_index: index,
            language: "browser".to_string(),
            stdout: redact_output(&outcome.stdout, &self.ctx.redact_values),
            stderr: redact_output(&outcome.stderr, &self.ctx.redact_values),
            exit_code: outcome.exit_code,
            duration: start.elapsed(),
        };
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
        .map_err(|e| format!("Failed to spawn {}: {}", program, e))?;

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

fn collect_output(process: &PersistentProcess, quiet: bool) -> (String, String, i32) {
    // Read stdout first, then stderr. Safe because the child writes
    // stdout sentinel before stderr sentinel. If stderr fills during
    // stdout reading, the child blocks AFTER writing stdout sentinel.
    // Once we drain stderr, the child unblocks and writes stderr sentinel.
    let (stdout, stdout_code) = collect_until_sentinel(&process.stdout_rx, quiet, false);
    let (stderr, stderr_code) = collect_until_sentinel(&process.stderr_rx, quiet, true);

    let exit_code = stdout_code.or(stderr_code).unwrap_or(0);
    (stdout, stderr, exit_code)
}

fn collect_until_sentinel(
    rx: &Receiver<String>,
    quiet: bool,
    is_stderr: bool,
) -> (String, Option<i32>) {
    let mut lines = Vec::new();
    let exit_code;

    loop {
        match rx.recv_timeout(BLOCK_TIMEOUT) {
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

    (lines.join("\n"), exit_code)
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

            BlockResult {
                block_index: index,
                language: block.language.clone(),
                stdout: redact_output(stdout_buf.trim_end(), &ctx.redact_values),
                stderr: redact_output(stderr_buf.trim_end(), &ctx.redact_values),
                exit_code,
                duration: start.elapsed(),
            }
        }
        Err(e) => BlockResult {
            block_index: index,
            language: block.language.clone(),
            stdout: String::new(),
            stderr: format!("Failed to spawn {}: {}", program, e),
            exit_code: 127,
            duration: start.elapsed(),
        },
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
        }
    }

    fn code_block(code: &str) -> CodeBlock {
        CodeBlock {
            language: "bash".to_string(),
            code: code.to_string(),
            line_number: 0,
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
}
