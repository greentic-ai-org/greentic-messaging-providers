use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use greentic_messaging_renderer::{
    capabilities_for, parse_presentation, render_plan_from_presentation,
};
use serde_json::Value;

fn usage() -> &'static str {
    "Usage:
  cargo run -p greentic-messaging-renderer --example render_presentation -- <provider> [presentation.json]

Examples:
  cargo run -p greentic-messaging-renderer --example render_presentation -- teams /tmp/presentation.json
  cat /tmp/presentation.json | cargo run -p greentic-messaging-renderer --example render_presentation -- slack"
}

fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let Some(provider) = args.first() else {
        eprintln!("{}", usage());
        return ExitCode::FAILURE;
    };

    let Some(capabilities) = capabilities_for(provider) else {
        eprintln!("unknown provider: {provider}");
        eprintln!("{}", usage());
        return ExitCode::FAILURE;
    };

    let input = match args.get(1) {
        Some(path) => match fs::read_to_string(path) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("failed to read {path}: {err}");
                return ExitCode::FAILURE;
            }
        },
        None => {
            let mut buffer = String::new();
            if let Err(err) = io::stdin().read_to_string(&mut buffer) {
                eprintln!("failed to read stdin: {err}");
                return ExitCode::FAILURE;
            }
            buffer
        }
    };

    let value = match serde_json::from_str::<Value>(&input) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("failed to parse input JSON: {err}");
            return ExitCode::FAILURE;
        }
    };

    let presentation = match parse_presentation(&value) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let plan = render_plan_from_presentation(&presentation, &capabilities);
    match serde_json::to_string_pretty(&plan) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("failed to serialize render plan: {err}");
            ExitCode::FAILURE
        }
    }
}
