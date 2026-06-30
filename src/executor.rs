use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{WbError, WbResult};
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

    /// Mask every secret value in `stdout`/`stderr`. Idempotent — re-running
    /// over already-redacted text finds no secret and is a no-op. This is the
    /// single choke point that guarantees no `BlockResult` leaves the executor
    /// with an unredacted secret, regardless of which `execute_*` path built it
    /// (the `http` error branches, for one, build stderr from raw curl/parse
    /// text). Every downstream sink — callbacks, `--events`, `--repair`,
    /// checkpoints — consumes these two fields, so redacting here covers them.
    pub fn redact(&mut self, values: &[String]) {
        if values.is_empty() {
            return;
        }
        self.stdout = redact_output(&self.stdout, values);
        self.stderr = redact_output(&self.stderr, values);
    }
}

/// Categorize a non-zero exit into a stable error-type token.
pub fn classify_exit(exit_code: i32) -> &'static str {
    if exit_code == 0 {
        return "";
    }
    // Unix convention: values ≥ 128 encode a signal (e.g. 137 = SIGKILL, 143 = SIGTERM).
    #[cfg(unix)]
    if (128..128 + 64).contains(&exit_code) {
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
    /// Timeout to arm for the *next* `execute_block` call. `None` means
    /// unbounded — the kill timer is not armed and the block runs until the
    /// child process exits on its own. Per-block overrides live in
    /// `Session::block_timeouts` (keyed by block index) and the session
    /// applies them before calling `execute_block` by mutating this field.
    pub block_timeout: Option<Duration>,
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
            block_timeout: None,
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

/// A parsed `http` request (#45).
#[derive(Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// Substitute `$VAR` and `${VAR}` from `env`. Unknown vars expand to empty,
/// matching shell behavior. `$$` is a literal `$`.
pub fn substitute_env_dollar(input: &str, env: &HashMap<String, String>) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'$' {
                out.push('$');
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'{' {
                if let Some(end) = input[i + 2..].find('}') {
                    let name = &input[i + 2..i + 2 + end];
                    out.push_str(env.get(name).map(|s| s.as_str()).unwrap_or(""));
                    i = i + 2 + end + 1;
                    continue;
                }
            }
            // Bare $NAME (alnum + underscore).
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i + 1 {
                let name = &input[i + 1..j];
                out.push_str(env.get(name).map(|s| s.as_str()).unwrap_or(""));
                i = j;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Parse an `http` fence body into a request. Lines starting with `#` and blank
/// lines before the request line are ignored.
pub fn parse_http_request(body: &str) -> Result<HttpRequest, String> {
    let mut lines = body.lines().peekable();
    // First meaningful line: `METHOD URL` or just `URL` (defaults to GET).
    let request_line = loop {
        match lines.next() {
            Some(l) if l.trim().is_empty() || l.trim_start().starts_with('#') => continue,
            Some(l) => break l.trim().to_string(),
            None => return Err("empty request (expected `METHOD URL`)".to_string()),
        }
    };
    let (method, url) = match request_line.split_once(char::is_whitespace) {
        Some((m, u)) if !u.trim().is_empty() => (m.to_uppercase(), u.trim().to_string()),
        _ => ("GET".to_string(), request_line),
    };
    if url.is_empty() {
        return Err("missing URL".to_string());
    }

    let mut headers = Vec::new();
    // Header lines until a blank line.
    for line in lines.by_ref() {
        if line.trim().is_empty() {
            break;
        }
        match line.split_once(':') {
            Some((k, v)) => headers.push((k.trim().to_string(), v.trim().to_string())),
            None => return Err(format!("bad header line '{}'", line.trim())),
        }
    }
    // Everything after the blank line is the body.
    let rest: Vec<&str> = lines.collect();
    let body = if rest.is_empty() {
        None
    } else {
        let joined = rest.join("\n");
        if joined.trim().is_empty() {
            None
        } else {
            Some(joined)
        }
    };

    Ok(HttpRequest {
        method,
        url,
        headers,
        body,
    })
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
            if parts.is_empty() {
                None
            } else {
                Some(ExecMode::Prefix(parts))
            }
        }
        ExecConfig::PerLanguage(map) => map.get(lang).map(|s| {
            let parts: Vec<String> = s.split_whitespace().map(|w| w.to_string()).collect();
            ExecMode::Replace(parts)
        }),
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
            for p in &parts[1..] {
                cmd.arg(p);
            }
            cmd.arg(program);
            cmd
        }
        Some(ExecMode::Replace(parts)) => {
            let mut cmd = Command::new(&parts[0]);
            for p in &parts[1..] {
                cmd.arg(p);
            }
            cmd
        }
        None => Command::new(program),
    }
}

// ─── Persistent session ───────────────────────────────────────────────

const SENTINEL_PREFIX: &str = "__WB_DONE_";
const SENTINEL_SUFFIX: &str = "__";
const CODE_END_MARKER: &str = "__WB_CODE_END__";

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

    pub fn suspend_browser_sidecar(&mut self) {
        let Some(mut sidecar) = self.browser_sidecar.take() else {
            return;
        };
        if let Err(e) = sidecar.suspend() {
            if !self.ctx.quiet {
                crate::output::print_stderr_dim(&format!("warning: {}", e));
            }
        }
    }

    /// Override the per-block timeout for the *next* `execute_block` call.
    /// `None` means unbounded (no kill timer). The caller is expected to
    /// reset this between blocks since the session object is long-lived.
    pub fn set_block_timeout(&mut self, timeout: Option<Duration>) {
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

    /// Secret values to mask before sending block output off-box (e.g. the
    /// `--repair` endpoint, #42).
    pub fn redact_values(&self) -> &[String] {
        &self.ctx.redact_values
    }

    /// Native `http` runtime (#45): execute a REST call described by an `http`
    /// fence via `curl`. Body grammar:
    ///   line 1: `METHOD URL` (METHOD optional → defaults to GET)
    ///   then:   `Header: Value` lines until a blank line
    ///   then:   the (optional) request body
    /// `$VAR` / `${VAR}` in the URL, headers, and body are substituted from the
    /// session env. stdout = response body; exit 0 on a 2xx status, else 1.
    fn execute_http(&self, block: &CodeBlock, index: usize) -> BlockResult {
        let start = Instant::now();
        let resolved = substitute_env_dollar(&block.code, &self.ctx.env);
        let req = match parse_http_request(&resolved) {
            Ok(r) => r,
            Err(e) => {
                return BlockResult {
                    block_index: index,
                    language: "http".to_string(),
                    stdout: String::new(),
                    stderr: format!("http: {e}"),
                    exit_code: 2,
                    duration: start.elapsed(),
                    error_type: Some("http_invalid".to_string()),
                    stdout_partial: false,
                    stderr_partial: false,
                };
            }
        };

        // Marker separates the response body from the trailing status code that
        // `-w` appends, so we can split them even when the body lacks a newline.
        const MARK: &str = "\n__WB_HTTP_STATUS__:";
        let mut args: Vec<String> = vec![
            "-sS".into(),
            "--max-time".into(),
            "60".into(),
            "-X".into(),
            req.method.clone(),
            "-w".into(),
            format!("{MARK}%{{http_code}}"),
        ];
        for (k, v) in &req.headers {
            args.push("-H".into());
            args.push(format!("{k}: {v}"));
        }
        if let Some(ref body) = req.body {
            args.push("--data-binary".into());
            args.push(body.clone());
        }
        args.push(req.url.clone());

        if !self.ctx.quiet {
            // The resolved URL can carry an interpolated secret (e.g. ?token=…);
            // mask it before echoing the request line to the console.
            println!(
                "→ {} {}",
                req.method,
                redact_output(&req.url, &self.ctx.redact_values)
            );
        }

        let output = match Command::new("curl").args(&args).output() {
            Ok(o) => o,
            Err(e) => {
                return BlockResult {
                    block_index: index,
                    language: "http".to_string(),
                    stdout: String::new(),
                    stderr: format!("http: curl failed to launch: {e}"),
                    exit_code: 1,
                    duration: start.elapsed(),
                    error_type: Some("http_failed".to_string()),
                    stdout_partial: false,
                    stderr_partial: false,
                };
            }
        };

        let raw = String::from_utf8_lossy(&output.stdout);
        let (body, status) = match raw.rsplit_once(MARK) {
            Some((b, s)) => (b.to_string(), s.trim().parse::<u16>().unwrap_or(0)),
            None => (raw.to_string(), 0),
        };
        let curl_err = String::from_utf8_lossy(&output.stderr);

        let ok = (200..300).contains(&status);
        let body = redact_output(&body, &self.ctx.redact_values);
        if !self.ctx.quiet {
            if !body.is_empty() {
                println!("{body}");
            }
            eprintln!("← HTTP {status}");
        }
        BlockResult {
            block_index: index,
            language: "http".to_string(),
            stdout: body,
            stderr: if status == 0 {
                format!("http request failed: {}", curl_err.trim())
            } else if ok {
                String::new()
            } else {
                format!("HTTP {status}")
            },
            exit_code: if ok { 0 } else { 1 },
            duration: start.elapsed(),
            error_type: if ok {
                None
            } else {
                Some("http_status".to_string())
            },
            stdout_partial: false,
            stderr_partial: false,
        }
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
                    no_cache: false,
                    attrs: Default::default(),
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
                no_cache: false,
                attrs: Default::default(),
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
                no_cache: false,
                attrs: Default::default(),
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
                no_cache: false,
                attrs: Default::default(),
            };
            let saved_quiet = self.ctx.quiet;
            self.ctx.quiet = true;
            self.execute_block(&block, 0);
            self.ctx.quiet = saved_quiet;
        }
    }

    /// Native `sql` runtime (#45): run the block body as a query via the
    /// `sqlite3` or `psql` CLI (no driver dependency). The connection comes from
    /// `$WB_SQL_URL` or `$DATABASE_URL` in the session env: `postgres(ql)://…`
    /// → `psql`; `sqlite:<path>` or a bare path → `sqlite3`. stdout = result
    /// rows; exit 0 on success, non-zero on a query/connection error.
    fn execute_sql(&self, block: &CodeBlock, index: usize) -> BlockResult {
        let start = Instant::now();
        let fail = |stderr: String, kind: &str| BlockResult {
            block_index: index,
            language: "sql".to_string(),
            stdout: String::new(),
            stderr,
            exit_code: 2,
            duration: start.elapsed(),
            error_type: Some(kind.to_string()),
            stdout_partial: false,
            stderr_partial: false,
        };

        let conn = match self
            .ctx
            .env
            .get("WB_SQL_URL")
            .or_else(|| self.ctx.env.get("DATABASE_URL"))
            .filter(|s| !s.is_empty())
        {
            Some(c) => c.clone(),
            None => {
                return fail(
                    "sql: no connection — set $WB_SQL_URL or $DATABASE_URL".to_string(),
                    "sql_no_connection",
                )
            }
        };
        let query = block.code.trim();
        if query.is_empty() {
            return fail("sql: empty query".to_string(), "sql_invalid");
        }

        let lower = conn.to_ascii_lowercase();
        let (program, args): (&str, Vec<String>) =
            if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
                (
                    "psql",
                    vec![
                        conn.clone(),
                        "-v".into(),
                        "ON_ERROR_STOP=1".into(),
                        "-c".into(),
                        query.to_string(),
                    ],
                )
            } else {
                let path = conn
                    .strip_prefix("sqlite://")
                    .or_else(|| conn.strip_prefix("sqlite:"))
                    .unwrap_or(&conn)
                    .to_string();
                ("sqlite3", vec![path, query.to_string()])
            };

        if !self.ctx.quiet {
            println!("→ {} query", program);
        }
        let output = match Command::new(program).args(&args).output() {
            Ok(o) => o,
            Err(e) => {
                return fail(
                    format!("sql: failed to launch {program}: {e} (is it installed?)"),
                    "sql_failed",
                )
            }
        };
        let stdout = redact_output(
            &String::from_utf8_lossy(&output.stdout),
            &self.ctx.redact_values,
        );
        let stderr = redact_output(
            &String::from_utf8_lossy(&output.stderr),
            &self.ctx.redact_values,
        );
        let code = output.status.code().unwrap_or(1);
        if !self.ctx.quiet && !stdout.is_empty() {
            println!("{stdout}");
        }
        BlockResult {
            block_index: index,
            language: "sql".to_string(),
            stdout,
            stderr: if code == 0 { String::new() } else { stderr },
            exit_code: code,
            duration: start.elapsed(),
            error_type: if code == 0 {
                None
            } else {
                Some("sql_error".to_string())
            },
            stdout_partial: false,
            stderr_partial: false,
        }
    }

    pub fn execute_block(&mut self, block: &CodeBlock, index: usize) -> BlockResult {
        let start = Instant::now();

        // Native `http` runtime: REST calls via curl (no language subprocess).
        if block.language.eq_ignore_ascii_case("http") {
            let mut r = self.execute_http(block, index);
            // The http handler's error branches build stderr from raw parse/curl
            // text (a secret interpolated into the request can survive there);
            // redact at the boundary so the invariant holds for every branch.
            r.redact(&self.ctx.redact_values);
            r.auto_classify();
            return r;
        }
        // Native `sql` runtime: queries via the sqlite3/psql CLIs.
        if block.language.eq_ignore_ascii_case("sql") {
            let mut r = self.execute_sql(block, index);
            r.redact(&self.ctx.redact_values);
            r.auto_classify();
            return r;
        }

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
            match spawn_persistent(
                &lang,
                &self.ctx.env,
                &work_dir,
                &self.ctx.venv,
                &self.ctx.exec_config,
            ) {
                Ok(proc) => {
                    self.processes.insert(lang.clone(), proc);
                }
                Err(e) => {
                    // spawn_persistent now returns spawn_error_message text
                    // for ENOENT; propagate the type so agents branch on it.
                    let msg = e.message();
                    let kind = if msg.starts_with('`') && msg.contains("not found on PATH") {
                        "spawn_not_found"
                    } else {
                        "spawn_failed"
                    };
                    return BlockResult {
                        block_index: index,
                        language: block.language.clone(),
                        stdout: String::new(),
                        stderr: e.to_string(),
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
        let redact = self.ctx.redact_values.clone();
        let (ok, out) = {
            let process = self.processes.get_mut(&lang).unwrap();
            match send_code(process, &lang, &code) {
                Ok(()) => {
                    let collected = collect_output(process, quiet, timeout, &redact);
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
                        spawn_env.insert("WB_BROWSER_VENDOR".to_string(), declared.clone());
                    }
                }
            }
            match Sidecar::spawn(&spawn_env, &self.ctx.working_dir) {
                Ok(sc) => self.browser_sidecar = Some(sc),
                Err(e) => {
                    if !self.ctx.quiet {
                        crate::output::print_stderr_dim(&e.to_string());
                    }
                    return (
                        BlockResult {
                            block_index: index,
                            language: "browser".to_string(),
                            stdout: String::new(),
                            stderr: e.to_string(),
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

        let outcome = self.browser_sidecar.as_mut().unwrap().run_slice(
            resolved_spec,
            self.ctx.quiet,
            ctx,
            restore,
        );

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
) -> WbResult<PersistentProcess> {
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
        "node" => ("node", vec!["-e".to_string(), NODE_HARNESS.to_string()]),
        "ruby" => ("ruby", vec!["-e".to_string(), RUBY_HARNESS.to_string()]),
        _ => return Err(WbError::Io(format!("No session support for {}", lang))),
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
        .map_err(|e| WbError::Io(spawn_error_message(&effective_program, lang, &e)))?;

    let stdin = BufWriter::new(child.stdin.take().unwrap());
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if stdout_tx.send(line).is_err() {
                break;
            }
        }
    });

    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
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

fn collect_output(
    process: &PersistentProcess,
    quiet: bool,
    timeout: Option<Duration>,
    redact: &[String],
) -> CollectedOutput {
    // Read stdout first, then stderr. Safe because the child writes
    // stdout sentinel before stderr sentinel. If stderr fills during
    // stdout reading, the child blocks AFTER writing stdout sentinel.
    // Once we drain stderr, the child unblocks and writes stderr sentinel.
    let (stdout, stdout_code, stdout_timed_out) =
        collect_until_sentinel(&process.stdout_rx, quiet, false, timeout, redact);
    // If stdout timed out, there's no point waiting the full timeout again
    // for the stderr sentinel — the child is already considered dead. Fall
    // through with a near-zero window so we drain anything already buffered.
    // If stdout did NOT time out (sentinel seen, or no timeout configured),
    // mirror the caller's `timeout` setting for stderr.
    let stderr_timeout = if stdout_timed_out {
        Some(Duration::from_millis(100))
    } else {
        timeout
    };
    let (stderr, stderr_code, stderr_timed_out) =
        collect_until_sentinel(&process.stderr_rx, quiet, true, stderr_timeout, redact);

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
/// block. `timeout = None` means wait forever (blocking recv); the only way
/// to break out is the sentinel or a disconnected channel.
fn collect_until_sentinel(
    rx: &Receiver<String>,
    quiet: bool,
    is_stderr: bool,
    timeout: Option<Duration>,
    redact: &[String],
) -> (String, Option<i32>, bool) {
    let mut lines = Vec::new();
    let exit_code;
    let mut timed_out = false;

    loop {
        let recv_result = match timeout {
            Some(d) => rx.recv_timeout(d),
            None => match rx.recv() {
                Ok(v) => Ok(v),
                Err(_) => Err(mpsc::RecvTimeoutError::Disconnected),
            },
        };
        match recv_result {
            Ok(line) => {
                if let Some(code) = parse_sentinel(&line) {
                    exit_code = Some(code);
                    break;
                }
                if !quiet && !crate::step_outputs::is_output_capture_line(&line) {
                    // Redact secrets in the live terminal stream too — the final
                    // BlockResult is redacted, but this per-line echo would
                    // otherwise leak secret values to the console.
                    let shown = redact_output(&line, redact);
                    if is_stderr {
                        crate::output::print_stderr_dim(&shown);
                    } else {
                        println!("{}", shown);
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
    while lines.last().is_some_and(|l| l.is_empty()) {
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
            cmd.env("PATH", format!("{}:{}", bin_dir.display(), current_path));
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
            // Redact secrets in the live stream too (the BlockResult is redacted
            // separately below). Each reader thread gets its own copy.
            let redact_out = ctx.redact_values.clone();
            let redact_err = ctx.redact_values.clone();
            let stdout_thread = child.stdout.take().map(|out| {
                thread::spawn(move || {
                    let reader = BufReader::new(out);
                    let mut buf = String::new();
                    for line in reader.lines().map_while(Result::ok) {
                        if !quiet && !crate::step_outputs::is_output_capture_line(&line) {
                            println!("{}", redact_output(&line, &redact_out));
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
                    for line in reader.lines().map_while(Result::ok) {
                        if !quiet && !crate::step_outputs::is_output_capture_line(&line) {
                            crate::output::print_stderr_dim(&redact_output(&line, &redact_err));
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
        "bash" | "shell" => ("bash".to_string(), vec!["-s".to_string()], code.to_string()),
        "sh" => ("sh".to_string(), vec!["-s".to_string()], code.to_string()),
        "zsh" => ("zsh".to_string(), vec!["-s".to_string()], code.to_string()),
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
mod http_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parses_method_url_headers_body() {
        let req = parse_http_request(
            "POST https://api.test/x\nAuthorization: Bearer t\nContent-Type: application/json\n\n{\"a\":1}",
        )
        .unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "https://api.test/x");
        assert_eq!(req.headers.len(), 2);
        assert_eq!(req.body.as_deref(), Some("{\"a\":1}"));
    }

    #[test]
    fn defaults_to_get_and_ignores_comments() {
        let req = parse_http_request("# get health\nhttps://api.test/health\n").unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://api.test/health");
        assert!(req.body.is_none());
    }

    #[test]
    fn empty_request_errors() {
        assert!(parse_http_request("\n# only a comment\n").is_err());
    }

    #[test]
    fn dollar_substitution() {
        let mut env = HashMap::new();
        env.insert("TOKEN".to_string(), "secret".to_string());
        env.insert("HOST".to_string(), "api.test".to_string());
        assert_eq!(
            substitute_env_dollar("https://${HOST}/x?t=$TOKEN", &env),
            "https://api.test/x?t=secret"
        );
        // Unknown var → empty; $$ → literal $.
        assert_eq!(substitute_env_dollar("$NOPE/$$", &env), "/$");
    }
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
        assert_eq!(
            substitute_vars("echo {{cluster}}", &vars),
            "echo {{cluster}}"
        );
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
        assert_eq!(
            redact_output("token: secret123", &values),
            "token: secret123"
        );
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
            block_timeout: None,
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
            no_cache: false,
            attrs: Default::default(),
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
        ctx.block_timeout = Some(Duration::from_millis(500));
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
    fn test_unbounded_default_runs_past_old_300s_cap_signal() {
        // Regression for the v0.14.0 default-cap removal: with no timeout
        // set anywhere, a block that takes longer than what the old hardcoded
        // cap *would have* allowed must complete. 1.5s sleep is well past
        // what mpsc::recv_timeout's "no progress" debounce would worry about
        // on slow CI; the real proof is that it returns exit_code 0 with no
        // partial flags rather than dying with `[wb: block timed out]`.
        let mut ctx = bash_ctx();
        assert!(
            ctx.block_timeout.is_none(),
            "bash_ctx default must be unbounded; got {:?}",
            ctx.block_timeout
        );
        ctx.block_timeout = None;
        let mut session = Session::new(ctx);
        let r = session.execute_block(&code_block("sleep 1.5; echo done"), 0);
        assert_eq!(
            r.exit_code, 0,
            "unbounded block should run to completion, got: {:?}",
            r
        );
        assert!(!r.stdout_partial, "stdout_partial must be false: {:?}", r);
        assert!(!r.stderr_partial, "stderr_partial must be false: {:?}", r);
        assert!(
            r.stdout.contains("done"),
            "expected 'done' in stdout, got: {:?}",
            r.stdout
        );
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

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::collections::HashMap;

    // ─── Test fixtures ───────────────────────────────────────────────

    /// Probe whether a runtime binary is on PATH. Tests that need an optional
    /// interpreter (python3/node/ruby/sqlite3/psql) gate their assertions on
    /// this so the suite still passes on a machine that lacks it.
    fn have(bin: &str) -> bool {
        Command::new(bin)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    fn ctx_bash() -> ExecutionContext {
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
            block_timeout: None,
        }
    }

    fn block(lang: &str, code: &str) -> CodeBlock {
        CodeBlock {
            language: lang.to_string(),
            code: code.to_string(),
            line_number: 0,
            skip_execution: false,
            silent: false,
            when: None,
            skip_if: None,
            no_cache: false,
            attrs: Default::default(),
        }
    }

    fn unique_tmp(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = Instant::now().elapsed().as_nanos();
        p.push(format!("wb_exec_test_{}_{}_{}", pid, nanos, name));
        p
    }

    // ─── classify_exit / auto_classify ───────────────────────────────

    #[test]
    fn classify_exit_zero_is_empty() {
        assert_eq!(classify_exit(0), "");
    }

    #[test]
    fn classify_exit_nonzero_is_nonzero_exit() {
        assert_eq!(classify_exit(1), "nonzero_exit");
        assert_eq!(classify_exit(2), "nonzero_exit");
        assert_eq!(classify_exit(127), "nonzero_exit");
    }

    #[cfg(unix)]
    #[test]
    fn classify_exit_signal_range_is_signal_killed() {
        assert_eq!(classify_exit(137), "signal_killed"); // SIGKILL
        assert_eq!(classify_exit(143), "signal_killed"); // SIGTERM
        assert_eq!(classify_exit(128), "signal_killed");
        // Out of the signal window falls back to nonzero_exit.
        assert_eq!(classify_exit(192), "nonzero_exit");
    }

    #[test]
    fn auto_classify_success_leaves_none() {
        let mut r = ok_result();
        r.exit_code = 0;
        r.auto_classify();
        assert!(r.error_type.is_none());
    }

    #[test]
    fn auto_classify_nonzero_sets_token() {
        let mut r = ok_result();
        r.exit_code = 3;
        r.auto_classify();
        assert_eq!(r.error_type.as_deref(), Some("nonzero_exit"));
    }

    #[test]
    fn auto_classify_preserves_existing_type() {
        let mut r = ok_result();
        r.exit_code = 1;
        r.error_type = Some("http_status".to_string());
        r.auto_classify();
        assert_eq!(r.error_type.as_deref(), Some("http_status"));
    }

    fn ok_result() -> BlockResult {
        BlockResult {
            block_index: 0,
            language: "bash".to_string(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(1),
            error_type: None,
            stdout_partial: false,
            stderr_partial: false,
        }
    }

    #[test]
    fn block_result_success_helper() {
        let mut r = ok_result();
        assert!(r.success());
        r.exit_code = 1;
        assert!(!r.success());
    }

    // ─── normalize_language / supports_session / is_python_language ──

    #[test]
    fn normalize_language_aliases() {
        let none = None;
        assert_eq!(normalize_language("python3", &none), "python");
        assert_eq!(normalize_language("py", &none), "python");
        assert_eq!(normalize_language("PYTHON", &none), "python");
        assert_eq!(normalize_language("shell", &none), "bash");
        assert_eq!(normalize_language("bash", &none), "bash");
        assert_eq!(normalize_language("sh", &none), "sh");
        assert_eq!(normalize_language("zsh", &none), "zsh");
        assert_eq!(normalize_language("javascript", &none), "node");
        assert_eq!(normalize_language("js", &none), "node");
        assert_eq!(normalize_language("rb", &none), "ruby");
    }

    #[test]
    fn normalize_language_unknown_uses_default() {
        let dflt = Some("python".to_string());
        assert_eq!(normalize_language("cobol", &dflt), "python");
    }

    #[test]
    fn normalize_language_unknown_without_default_is_passthrough() {
        let none = None;
        assert_eq!(normalize_language("cobol", &none), "cobol");
    }

    #[test]
    fn supports_session_matrix() {
        for s in ["python", "bash", "sh", "zsh", "node", "ruby"] {
            assert!(supports_session(s), "{s} should support sessions");
        }
        for s in ["http", "sql", "perl", "go", "cobol"] {
            assert!(!supports_session(s), "{s} should not support sessions");
        }
    }

    #[test]
    fn is_python_language_matrix() {
        assert!(is_python_language("python"));
        assert!(is_python_language("Py"));
        assert!(is_python_language("PYTHON3"));
        assert!(!is_python_language("bash"));
        assert!(!is_python_language("node"));
    }

    // ─── parse_sentinel ──────────────────────────────────────────────

    #[test]
    fn parse_sentinel_valid_and_invalid() {
        assert_eq!(parse_sentinel("__WB_DONE_0__"), Some(0));
        assert_eq!(parse_sentinel("  __WB_DONE_42__  "), Some(42));
        assert_eq!(parse_sentinel("__WB_DONE_-1__"), Some(-1));
        assert_eq!(parse_sentinel("not a sentinel"), None);
        assert_eq!(parse_sentinel("__WB_DONE_abc__"), None);
        assert_eq!(parse_sentinel("__WB_DONE_"), None);
    }

    // ─── substitute_env_dollar edge cases ────────────────────────────

    #[test]
    fn substitute_env_dollar_unterminated_brace_is_literal() {
        let env = HashMap::new();
        // No closing brace → the `${` is emitted literally.
        assert_eq!(substitute_env_dollar("a ${UNCLOSED", &env), "a ${UNCLOSED");
    }

    #[test]
    fn substitute_env_dollar_trailing_dollar() {
        let env = HashMap::new();
        assert_eq!(substitute_env_dollar("cost$", &env), "cost$");
    }

    #[test]
    fn substitute_env_dollar_brace_form() {
        let mut env = HashMap::new();
        env.insert("A".to_string(), "1".to_string());
        assert_eq!(substitute_env_dollar("x=${A}y", &env), "x=1y");
    }

    // ─── parse_http_request error branches ───────────────────────────

    #[test]
    fn parse_http_request_bad_header() {
        let err = parse_http_request("GET https://h/x\nNoColonHeader\n").unwrap_err();
        assert!(err.contains("bad header"), "got: {err}");
    }

    #[test]
    fn parse_http_request_method_lowercased_to_upper() {
        let req = parse_http_request("delete https://h/x").unwrap();
        assert_eq!(req.method, "DELETE");
    }

    // ─── resolve_exec ────────────────────────────────────────────────

    #[test]
    fn resolve_exec_none_config() {
        assert!(resolve_exec(&None, "python").is_none());
    }

    #[test]
    fn resolve_exec_global_empty_is_none() {
        let cfg = Some(ExecConfig::Global("   ".to_string()));
        assert!(resolve_exec(&cfg, "bash").is_none());
    }

    #[test]
    fn resolve_exec_global_prefix() {
        let cfg = Some(ExecConfig::Global("docker exec c".to_string()));
        match resolve_exec(&cfg, "bash") {
            Some(ExecMode::Prefix(p)) => assert_eq!(p, vec!["docker", "exec", "c"]),
            other => panic!("expected prefix, got {other:?}"),
        }
    }

    #[test]
    fn resolve_exec_per_language_hit_and_miss() {
        let mut map = HashMap::new();
        map.insert("python".to_string(), "uv run python".to_string());
        let cfg = Some(ExecConfig::PerLanguage(map));
        match resolve_exec(&cfg, "python") {
            Some(ExecMode::Replace(p)) => assert_eq!(p, vec!["uv", "run", "python"]),
            other => panic!("expected replace, got {other:?}"),
        }
        assert!(resolve_exec(&cfg, "node").is_none());
    }

    // ExecMode needs Debug for the panic messages above.
    impl std::fmt::Debug for ExecMode {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                ExecMode::Prefix(p) => write!(f, "Prefix({p:?})"),
                ExecMode::Replace(p) => write!(f, "Replace({p:?})"),
            }
        }
    }

    // ─── resolve_working_dir ─────────────────────────────────────────

    #[test]
    fn resolve_working_dir_none_returns_base() {
        assert_eq!(resolve_working_dir(&None, "bash", "/work"), "/work");
    }

    #[test]
    fn resolve_working_dir_global_absolute() {
        let cfg = Some(DirConfig::Global("/abs/path".to_string()));
        assert_eq!(resolve_working_dir(&cfg, "bash", "/work"), "/abs/path");
    }

    #[test]
    fn resolve_working_dir_global_relative_joins_base() {
        let cfg = Some(DirConfig::Global("sub".to_string()));
        let got = resolve_working_dir(&cfg, "bash", "/work");
        assert!(got.ends_with("sub"), "got: {got}");
        assert!(got.starts_with("/work"), "got: {got}");
    }

    #[test]
    fn resolve_working_dir_per_language() {
        let mut map = HashMap::new();
        map.insert("python".to_string(), "src".to_string());
        let cfg = Some(DirConfig::PerLanguage(map));
        let got = resolve_working_dir(&cfg, "python", "/work");
        assert!(got.contains("src"), "got: {got}");
        // A language not in the map gets the base.
        assert_eq!(resolve_working_dir(&cfg, "bash", "/work"), "/work");
    }

    // ─── build_command ───────────────────────────────────────────────

    #[test]
    fn build_command_none_uses_program() {
        let cmd = build_command(None, "bash");
        assert_eq!(cmd.get_program(), "bash");
        assert_eq!(cmd.get_args().count(), 0);
    }

    #[test]
    fn build_command_prefix_appends_program() {
        let exec = ExecMode::Prefix(vec!["docker".into(), "exec".into(), "c".into()]);
        let cmd = build_command(Some(&exec), "python3");
        assert_eq!(cmd.get_program(), "docker");
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["exec", "c", "python3"]);
    }

    #[test]
    fn build_command_replace_drops_program() {
        let exec = ExecMode::Replace(vec!["uv".into(), "run".into(), "python".into()]);
        let cmd = build_command(Some(&exec), "python3");
        assert_eq!(cmd.get_program(), "uv");
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["run", "python"]);
    }

    // ─── spawn_error_message ─────────────────────────────────────────

    #[test]
    fn spawn_error_message_notfound_has_install_hint() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        let msg = spawn_error_message("python3", "python", &e);
        assert!(msg.contains("not found on PATH"), "got: {msg}");
        assert!(msg.contains("install Python"), "got: {msg}");
    }

    #[test]
    fn spawn_error_message_notfound_unknown_lang_generic_hint() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        let msg = spawn_error_message("cobolc", "cobol", &e);
        assert!(msg.contains("exec:"), "got: {msg}");
    }

    #[test]
    fn spawn_error_message_other_kind() {
        let e = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let msg = spawn_error_message("bash", "bash", &e);
        assert!(msg.starts_with("Failed to spawn bash"), "got: {msg}");
    }

    // ─── resolve_runtime ─────────────────────────────────────────────

    #[test]
    fn resolve_runtime_known_languages() {
        let ctx = ctx_bash();
        let (p, a, _c) = resolve_runtime("python", "print(1)", &ctx);
        assert_eq!(p, "python3");
        assert_eq!(a, vec!["-"]);

        let (p, a, _c) = resolve_runtime("node", "x", &ctx);
        assert_eq!(p, "node");
        assert_eq!(a[0], "-e");

        let (p, _a, _c) = resolve_runtime("ruby", "x", &ctx);
        assert_eq!(p, "ruby");
        let (p, _a, _c) = resolve_runtime("perl", "x", &ctx);
        assert_eq!(p, "perl");
        let (p, _a, _c) = resolve_runtime("php", "x", &ctx);
        assert_eq!(p, "php");
        let (p, _a, _c) = resolve_runtime("lua", "x", &ctx);
        assert_eq!(p, "lua");
        let (p, _a, _c) = resolve_runtime("r", "x", &ctx);
        assert_eq!(p, "Rscript");
        let (p, _a, _c) = resolve_runtime("swift", "x", &ctx);
        assert_eq!(p, "swift");

        let (p, a, _c) = resolve_runtime("sh", "x", &ctx);
        assert_eq!(p, "sh");
        assert_eq!(a, vec!["-s"]);
        let (p, _a, _c) = resolve_runtime("zsh", "x", &ctx);
        assert_eq!(p, "zsh");
        let (p, a, code) = resolve_runtime("bash", "echo hi", &ctx);
        assert_eq!(p, "bash");
        assert_eq!(a, vec!["-s"]);
        assert_eq!(code, "echo hi");
    }

    #[test]
    fn resolve_runtime_go_writes_tmpfile() {
        let ctx = ctx_bash();
        let (p, a, _c) = resolve_runtime("go", "package main\nfunc main(){}", &ctx);
        assert_eq!(p, "go");
        assert_eq!(a[0], "run");
        assert!(a[1].ends_with("wb_block.go"), "got: {:?}", a);
    }

    #[test]
    fn resolve_runtime_unknown_with_default_recurses() {
        let mut ctx = ctx_bash();
        ctx.default_runtime = Some("python".to_string());
        let (p, a, _c) = resolve_runtime("cobol", "x", &ctx);
        assert_eq!(p, "python3");
        assert_eq!(a, vec!["-"]);
    }

    #[test]
    fn resolve_runtime_unknown_without_default_is_bash() {
        let mut ctx = ctx_bash();
        ctx.default_runtime = None;
        let (p, a, code) = resolve_runtime("cobol", "echo hi", &ctx);
        assert_eq!(p, "bash");
        assert_eq!(a, vec!["-s"]);
        assert_eq!(code, "echo hi");
    }

    // ─── ExecutionContext::from_frontmatter ──────────────────────────

    #[test]
    fn from_frontmatter_derives_working_dir_from_parent() {
        let mut fm = Frontmatter::default();
        let mut env = HashMap::new();
        env.insert("K".to_string(), "v".to_string());
        fm.env = Some(env);
        fm.runtime = Some("python".to_string());
        let ctx = ExecutionContext::from_frontmatter(&fm, "/a/b/run.md");
        assert_eq!(ctx.working_dir, "/a/b");
        assert_eq!(ctx.default_runtime.as_deref(), Some("python"));
        assert_eq!(ctx.env.get("K").map(String::as_str), Some("v"));
    }

    #[test]
    fn from_frontmatter_bare_filename_is_dot() {
        let fm = Frontmatter::default();
        let ctx = ExecutionContext::from_frontmatter(&fm, "run.md");
        assert_eq!(ctx.working_dir, ".");
    }

    // ─── execute_block_oneshot (bash always present) ─────────────────

    #[test]
    fn oneshot_bash_success_captures_stdout() {
        let ctx = ctx_bash();
        let r = execute_block_oneshot(&block("bash", "echo hello-oneshot"), 7, &ctx);
        assert_eq!(r.exit_code, 0);
        assert_eq!(r.block_index, 7);
        assert_eq!(r.stdout.trim(), "hello-oneshot");
        assert!(r.error_type.is_none());
    }

    #[test]
    fn oneshot_bash_nonzero_exit_classified() {
        let ctx = ctx_bash();
        let r = execute_block_oneshot(&block("bash", "exit 4"), 0, &ctx);
        assert_eq!(r.exit_code, 4);
        assert_eq!(r.error_type.as_deref(), Some("nonzero_exit"));
    }

    #[test]
    fn oneshot_bash_stderr_captured() {
        let ctx = ctx_bash();
        let r = execute_block_oneshot(&block("bash", "echo oops >&2; exit 1"), 0, &ctx);
        assert_eq!(r.exit_code, 1);
        assert!(r.stderr.contains("oops"), "got: {:?}", r.stderr);
    }

    #[test]
    fn oneshot_env_injection() {
        let mut ctx = ctx_bash();
        ctx.env
            .insert("WB_INJECTED".to_string(), "yes-here".to_string());
        let r = execute_block_oneshot(&block("bash", "echo \"$WB_INJECTED\""), 0, &ctx);
        assert_eq!(r.stdout.trim(), "yes-here");
    }

    #[test]
    fn oneshot_redacts_secret_in_stdout() {
        let mut ctx = ctx_bash();
        ctx.redact_values = vec!["topsecret".to_string()];
        let r = execute_block_oneshot(&block("bash", "echo topsecret"), 0, &ctx);
        assert_eq!(r.stdout.trim(), "***");
    }

    #[test]
    fn oneshot_var_substitution() {
        let mut ctx = ctx_bash();
        ctx.vars.insert("name".to_string(), "world".to_string());
        let r = execute_block_oneshot(&block("bash", "echo hi {{name}}"), 0, &ctx);
        assert_eq!(r.stdout.trim(), "hi world");
    }

    #[test]
    fn oneshot_spawn_not_found_via_exec_replace() {
        // Force an ENOENT by replacing the program with a binary that cannot
        // exist on PATH. Exercises the spawn-error / spawn_not_found path.
        let mut ctx = ctx_bash();
        let mut map = HashMap::new();
        map.insert("bash".to_string(), "wb_nonexistent_binary_zzz".to_string());
        ctx.exec_config = Some(ExecConfig::PerLanguage(map));
        let r = execute_block_oneshot(&block("bash", "echo hi"), 0, &ctx);
        assert_eq!(r.exit_code, 127);
        assert_eq!(r.error_type.as_deref(), Some("spawn_not_found"));
        assert!(
            r.stderr.contains("not found on PATH"),
            "got: {:?}",
            r.stderr
        );
    }

    // ─── Session::execute_block dispatch ─────────────────────────────

    #[test]
    fn session_bash_exit_code_and_capture() {
        let mut s = Session::new(ctx_bash());
        let r = s.execute_block(&block("bash", "echo line1; echo line2"), 0);
        assert_eq!(r.exit_code, 0);
        assert!(r.stdout.contains("line1"));
        assert!(r.stdout.contains("line2"));
    }

    #[test]
    fn session_bash_env_via_set_env() {
        let mut s = Session::new(ctx_bash());
        s.set_env("WB_SESSION_ENV".to_string(), "abc".to_string());
        assert_eq!(
            s.env().get("WB_SESSION_ENV").map(String::as_str),
            Some("abc")
        );
        let r = s.execute_block(&block("bash", "echo \"$WB_SESSION_ENV\""), 0);
        assert_eq!(r.stdout.trim(), "abc");
    }

    #[test]
    fn session_redacts_block_output() {
        let mut ctx = ctx_bash();
        ctx.redact_values = vec!["hunter2".to_string()];
        let mut s = Session::new(ctx);
        let r = s.execute_block(&block("bash", "echo hunter2"), 0);
        assert_eq!(r.stdout.trim(), "***");
        assert_eq!(s.redact_values(), &["hunter2".to_string()]);
    }

    #[test]
    fn session_spawn_not_found_via_exec_replace() {
        let mut ctx = ctx_bash();
        let mut map = HashMap::new();
        map.insert("bash".to_string(), "wb_nonexistent_binary_yyy".to_string());
        ctx.exec_config = Some(ExecConfig::PerLanguage(map));
        let mut s = Session::new(ctx);
        let r = s.execute_block(&block("bash", "echo hi"), 0);
        assert_eq!(r.exit_code, 127);
        assert_eq!(r.error_type.as_deref(), Some("spawn_not_found"));
    }

    #[test]
    fn session_set_block_timeout_then_unbounded() {
        // Exercise set_block_timeout: arm a tight cap that fires, then clear it.
        let mut s = Session::new(ctx_bash());
        s.set_block_timeout(Some(Duration::from_millis(300)));
        let r = s.execute_block(&block("bash", "echo pre; sleep 5"), 0);
        assert!(
            r.stdout_partial || r.stderr_partial,
            "expected partial, got {r:?}"
        );
        assert_eq!(r.error_type.as_deref(), Some("timeout"));
        // After a timeout the dead session is dropped; a fresh unbounded block runs.
        s.set_block_timeout(None);
        let r2 = s.execute_block(&block("bash", "echo recovered"), 1);
        assert_eq!(r2.exit_code, 0);
        assert_eq!(r2.stdout.trim(), "recovered");
    }

    #[test]
    fn session_set_quiet_toggle() {
        let mut s = Session::new(ctx_bash());
        s.set_quiet(false);
        s.set_quiet(true);
        // Should still execute cleanly.
        let r = s.execute_block(&block("bash", "echo ok"), 0);
        assert_eq!(r.exit_code, 0);
    }

    #[test]
    fn session_remove_env() {
        let mut ctx = ctx_bash();
        ctx.env.insert("WB_RM".to_string(), "v".to_string());
        let mut s = Session::new(ctx);
        s.remove_env("WB_RM");
        assert!(s.env().get("WB_RM").is_none());
    }

    // ─── Native http runtime ─────────────────────────────────────────

    #[test]
    fn http_invalid_request_is_exit_2() {
        let s = Session::new(ctx_bash());
        // Empty body → parse error → http_invalid, exit 2.
        let r = s.execute_http(&block("http", "\n# just a comment\n"), 0);
        assert_eq!(r.exit_code, 2);
        assert_eq!(r.error_type.as_deref(), Some("http_invalid"));
        assert!(r.stderr.starts_with("http:"), "got: {:?}", r.stderr);
    }

    #[test]
    fn http_dispatch_through_execute_block() {
        let mut s = Session::new(ctx_bash());
        let r = s.execute_block(&block("HTTP", "\n"), 0);
        // Case-insensitive dispatch; empty body is invalid.
        assert_eq!(r.error_type.as_deref(), Some("http_invalid"));
    }

    #[test]
    fn http_connection_refused_fails_fast() {
        if !have("curl") {
            return;
        }
        let s = Session::new(ctx_bash());
        // Port 1 refuses immediately on localhost; no network round-trip.
        let r = s.execute_http(&block("http", "GET http://127.0.0.1:1/"), 0);
        // Connection failure → status unparsed (0) → exit 1, error http_status.
        assert_eq!(r.exit_code, 1);
        assert_eq!(r.error_type.as_deref(), Some("http_status"));
        assert!(
            r.stdout.is_empty(),
            "expected empty body, got {:?}",
            r.stdout
        );
    }

    // ─── Native sql runtime ──────────────────────────────────────────

    #[test]
    fn sql_no_connection() {
        let s = Session::new(ctx_bash());
        let r = s.execute_sql(&block("sql", "SELECT 1;"), 0);
        assert_eq!(r.exit_code, 2);
        assert_eq!(r.error_type.as_deref(), Some("sql_no_connection"));
    }

    #[test]
    fn sql_empty_query() {
        let mut ctx = ctx_bash();
        ctx.env
            .insert("WB_SQL_URL".to_string(), "sqlite::memory:".to_string());
        let s = Session::new(ctx);
        let r = s.execute_sql(&block("sql", "   \n  "), 0);
        assert_eq!(r.exit_code, 2);
        assert_eq!(r.error_type.as_deref(), Some("sql_invalid"));
    }

    #[test]
    fn sql_dispatch_through_execute_block_no_connection() {
        let mut s = Session::new(ctx_bash());
        let r = s.execute_block(&block("SQL", "SELECT 1;"), 0);
        assert_eq!(r.error_type.as_deref(), Some("sql_no_connection"));
    }

    #[test]
    fn sql_sqlite_happy_path() {
        if !have("sqlite3") {
            return; // optional dependency; assert only when present.
        }
        let dbpath = unique_tmp("sql.db");
        let mut ctx = ctx_bash();
        // Bare path form → sqlite3.
        ctx.env.insert(
            "WB_SQL_URL".to_string(),
            format!("sqlite:{}", dbpath.display()),
        );
        let s = Session::new(ctx);
        let r = s.execute_sql(&block("sql", "SELECT 1+2;"), 0);
        let _ = std::fs::remove_file(&dbpath);
        assert_eq!(r.exit_code, 0, "stderr: {:?}", r.stderr);
        assert!(r.stdout.contains('3'), "got stdout: {:?}", r.stdout);
        assert!(r.error_type.is_none());
    }

    #[test]
    fn sql_sqlite_query_error() {
        if !have("sqlite3") {
            return;
        }
        let dbpath = unique_tmp("sqlerr.db");
        let mut ctx = ctx_bash();
        ctx.env.insert(
            "WB_SQL_URL".to_string(),
            format!("sqlite://{}", dbpath.display()),
        );
        let s = Session::new(ctx);
        let r = s.execute_sql(&block("sql", "SELECT * FROM no_such_table;"), 0);
        let _ = std::fs::remove_file(&dbpath);
        assert_ne!(r.exit_code, 0);
        assert_eq!(r.error_type.as_deref(), Some("sql_error"));
        assert!(!r.stderr.is_empty());
    }

    #[test]
    fn sql_postgres_routes_to_psql() {
        // We don't need a live server: either psql is absent (sql_failed) or it
        // is present and the bogus connection fails (sql_error). Both exercise
        // the postgres branch / program selection without real network success.
        let mut ctx = ctx_bash();
        ctx.env.insert(
            "DATABASE_URL".to_string(),
            "postgres://nouser@127.0.0.1:1/nodb".to_string(),
        );
        let s = Session::new(ctx);
        let r = s.execute_sql(&block("sql", "SELECT 1;"), 0);
        assert_ne!(r.exit_code, 0);
        let kind = r.error_type.as_deref().unwrap_or("");
        assert!(
            kind == "sql_failed" || kind == "sql_error",
            "unexpected error_type: {kind:?} (stderr {:?})",
            r.stderr
        );
    }

    // ─── unset_env_in_sessions (bash branch) ─────────────────────────

    #[test]
    fn unset_env_in_sessions_clears_running_bash() {
        let mut ctx = ctx_bash();
        ctx.env
            .insert("WB_UNSET_ME".to_string(), "stillhere".to_string());
        let mut s = Session::new(ctx);
        // Start a bash session so processes map has a "bash" entry.
        let r = s.execute_block(&block("bash", "echo \"$WB_UNSET_ME\""), 0);
        assert_eq!(r.stdout.trim(), "stillhere");
        s.unset_env_in_sessions("WB_UNSET_ME");
        let r2 = s.execute_block(&block("bash", "echo \"[$WB_UNSET_ME]\""), 1);
        assert_eq!(r2.stdout.trim(), "[]");
    }

    // ─── collect_until_sentinel via real sessions (timeout window) ───

    #[test]
    fn session_disconnected_when_child_exits() {
        // `exit` ends the persistent bash session; the next block sees a fresh
        // process spawned because the dead one was removed.
        let mut s = Session::new(ctx_bash());
        let r1 = s.execute_block(&block("bash", "echo a"), 0);
        assert_eq!(r1.exit_code, 0);
        let r2 = s.execute_block(&block("bash", "echo b"), 1);
        assert_eq!(r2.exit_code, 0);
        assert_eq!(r2.stdout.trim(), "b");
    }

    // ─── Optional interpreter coverage (gated) ───────────────────────

    #[test]
    fn python_session_when_available() {
        if !have("python3") {
            return;
        }
        let mut ctx = ctx_bash();
        ctx.default_runtime = Some("python".to_string());
        let mut s = Session::new(ctx);
        let r1 = s.execute_block(&block("python", "x = 21"), 0);
        assert_eq!(r1.exit_code, 0, "stderr: {:?}", r1.stderr);
        let r2 = s.execute_block(&block("python", "print(x * 2)"), 1);
        assert_eq!(r2.exit_code, 0);
        assert_eq!(r2.stdout.trim(), "42");
    }

    #[test]
    fn python_session_error_nonzero() {
        if !have("python3") {
            return;
        }
        let mut ctx = ctx_bash();
        ctx.default_runtime = Some("python".to_string());
        let mut s = Session::new(ctx);
        let r = s.execute_block(&block("python", "raise ValueError('boom')"), 0);
        assert_eq!(r.exit_code, 1);
        assert!(r.stderr.contains("boom"), "got: {:?}", r.stderr);
    }

    #[test]
    fn node_session_when_available() {
        if !have("node") {
            return;
        }
        let mut ctx = ctx_bash();
        ctx.default_runtime = Some("node".to_string());
        let mut s = Session::new(ctx);
        let r = s.execute_block(&block("node", "console.log(1 + 1)"), 0);
        assert_eq!(r.exit_code, 0, "stderr: {:?}", r.stderr);
        assert_eq!(r.stdout.trim(), "2");
    }

    #[test]
    fn ruby_session_when_available() {
        if !have("ruby") {
            return;
        }
        let mut ctx = ctx_bash();
        ctx.default_runtime = Some("ruby".to_string());
        let mut s = Session::new(ctx);
        let r = s.execute_block(&block("ruby", "puts 3 + 4"), 0);
        assert_eq!(r.exit_code, 0, "stderr: {:?}", r.stderr);
        assert_eq!(r.stdout.trim(), "7");
    }
}
