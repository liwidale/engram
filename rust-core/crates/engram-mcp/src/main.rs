mod server;
mod tools;

use engram_core::context::{build_context, ContextRequest};
use engram_core::health::{audit, Severity};
use engram_core::Store;
use server::Server;
use std::path::PathBuf;
use std::process::ExitCode;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const USAGE: &str = "\
engram-mcp - Model Context Protocol server for .engram project memory

USAGE:
    engram-mcp --file <project.engram>             Serve MCP over stdio (default command)
    engram-mcp init <project.engram> [--name <n>]  Create a new .engram file
    engram-mcp audit --file <f> [--repo <dir>] [--json]
                                                   Run Context Health (exit 1 on errors)
    engram-mcp context --file <f> --task <text> [--max-tokens <n>]
                                                   Print a context bundle as XML
    engram-mcp --version | --help

The file can also be given with the ENGRAM_FILE environment variable.";

struct Args {
    command: String,
    positional: Vec<String>,
    file: Option<PathBuf>,
    name: Option<String>,
    repo: Option<String>,
    task: Option<String>,
    max_tokens: Option<usize>,
    json: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        command: "serve".into(),
        positional: vec![],
        file: None,
        name: None,
        repo: None,
        task: None,
        max_tokens: None,
        json: false,
    };
    let mut first = true;
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => a.command = "help".into(),
            "-V" | "--version" => a.command = "version".into(),
            "-f" | "--file" => a.file = Some(PathBuf::from(value(&arg)?)),
            "--name" => a.name = Some(value(&arg)?),
            "--repo" => a.repo = Some(value(&arg)?),
            "--task" => a.task = Some(value(&arg)?),
            "--max-tokens" => a.max_tokens = Some(value(&arg)?.parse().map_err(|_| "--max-tokens must be a number")?),
            "--json" => a.json = true,
            "serve" | "init" | "audit" | "context" if first => a.command = arg,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => a.positional.push(arg),
        }
        first = false;
    }
    if a.file.is_none() {
        a.file = a
            .positional
            .first()
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("ENGRAM_FILE").map(PathBuf::from));
    }
    Ok(a)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("engram-mcp: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match args.command.as_str() {
        "help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        "version" => {
            println!("engram-mcp {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let Some(file) = args.file.clone() else {
        eprintln!("engram-mcp: no .engram file given (use --file or ENGRAM_FILE)\n\n{USAGE}");
        return ExitCode::from(2);
    };
    let store = Store::new(file);
    let result = match args.command.as_str() {
        "init" => init(&store, &args),
        "audit" => run_audit(&store, &args),
        "context" => run_context(&store, &args),
        _ => serve(store),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("engram-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}

type CliResult = Result<ExitCode, Box<dyn std::error::Error>>;

fn init(store: &Store, args: &Args) -> CliResult {
    let name = args.name.clone().unwrap_or_else(|| {
        store
            .path()
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Project".into())
    });
    store.create(&name)?;
    println!("Created {} ({name})", store.path().display());
    Ok(ExitCode::SUCCESS)
}

fn run_audit(store: &Store, args: &Args) -> CliResult {
    let doc = store.load()?.document;
    let report = audit(&doc, &store.base_dir(), args.repo.as_deref());
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Context health {}/100 ({})", report.score, report.grade);
        for i in &report.issues {
            println!("  {:<7} {}", format!("{:?}", i.severity).to_lowercase(), i.message);
        }
    }
    let failed = report.issues.iter().any(|i| i.severity == Severity::Error);
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

fn run_context(store: &Store, args: &Args) -> CliResult {
    let doc = store.load()?.document;
    let req = ContextRequest::for_task(args.task.clone().unwrap_or_default(), args.max_tokens.unwrap_or(4000));
    let bundle = build_context(&doc, &req);
    print!("{}", bundle.xml);
    eprintln!(
        "{} tokens ({}), budget {}",
        bundle.tokens, bundle.tokenizer, bundle.max_tokens
    );
    Ok(ExitCode::SUCCESS)
}

fn serve(store: Store) -> CliResult {
    let snap = store.load()?;
    eprintln!(
        "[engram-mcp] serving {} (project '{}', revision {})",
        store.path().display(),
        snap.document.project.name,
        snap.document.revision
    );
    let rt = tokio::runtime::Builder::new_current_thread().enable_io().build()?;
    rt.block_on(async move {
        let mut server = Server::new(store);
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mut out = tokio::io::stdout();
        while let Some(line) = lines.next_line().await? {
            if let Some(response) = server.handle_line(&line) {
                out.write_all(response.as_bytes()).await?;
                out.write_all(b"\n").await?;
                out.flush().await?;
            }
        }
        Ok::<_, std::io::Error>(())
    })?;
    Ok(ExitCode::SUCCESS)
}
