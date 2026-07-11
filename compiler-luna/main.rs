mod errors;
mod ast;
mod lexer;
mod parser;
mod interpreter;
mod checker;
mod codegen;

use std::{env, fs, process};

use checker::SemanticChecker;
use codegen::LlvmIrGenerator;
use errors::format_error_with_context;
use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        process::exit(1);
    }

    match args[1].as_str() {
        "run" => {
            let path = args
                .get(2)
                .map(String::as_str)
                .unwrap_or_else(|| {
                    eprintln!("error: expected a file path after `run`");
                    process::exit(1);
                });

            run_file(path, false);
        }

        "check" => {
            let path = args
                .get(2)
                .map(String::as_str)
                .unwrap_or_else(|| {
                    eprintln!("error: expected a file path after `check`");
                    process::exit(1);
                });

            check_file(path);
        }

        "emit-ir" => {
            let path = args
                .get(2)
                .map(String::as_str)
                .unwrap_or_else(|| {
                    eprintln!("error: expected a file path after `emit-ir`");
                    process::exit(1);
                });

            emit_ir(path);
        }

        "--version" | "-v" | "version" => {
            println!("lunasc 0.2.0 — Luna Script Compiler");
        }

        "--help" | "-h" | "help" => {
            print_usage(&args[0]);
        }

        other => {
            if other.ends_with(".luna") || other.ends_with(".ln") {
                run_file(other, false);
            } else {
                eprintln!("error: unknown command `{other}`");
                print_usage(&args[0]);
                process::exit(1);
            }
        }
    }
}

fn print_usage(bin: &str) {
    println!("lunasc 0.2.0 — Luna Script Compiler");
    println!();

    println!("USAGE:");
    println!("  {bin} run <file>       Interpret a Luna source file");
    println!("  {bin} check <file>     Type-check without running");
    println!("  {bin} emit-ir <file>   Emit LLVM IR to stdout");
    println!("  {bin} --version        Print version");
    println!("  {bin} --help           Print this help");
    println!();

    println!("EXAMPLE:");
    println!("  {bin} run hello.luna");
}

fn read_source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: cannot read `{path}`: {e}");
        process::exit(1);
    })
}

fn lex_and_parse(source: &str, path: &str) -> crate::ast::nodes::Program {
    let mut lexer = Lexer::new(source);

    let tokens = lexer.tokenize().unwrap_or_else(|e| {
        eprintln!("{}", format_error_with_context(&e, source));
        process::exit(1);
    });

    let mut parser = Parser::new(tokens, path);

    parser.parse().unwrap_or_else(|e| {
        eprintln!("{}", format_error_with_context(&e, source));
        process::exit(1);
    })
}

fn run_file(path: &str, _verbose: bool) {
    let source = read_source(path);
    let program = lex_and_parse(&source, path);

    let mut interp = Interpreter::new();

    if let Err(e) = interp.run(&program) {
        eprintln!("{}", format_error_with_context(&e, &source));
        process::exit(1);
    }
}

fn check_file(path: &str) {
    let source = read_source(path);
    let program = lex_and_parse(&source, path);

    let mut checker = SemanticChecker::new();

    match checker.check(&program) {
        Ok(()) => {
            println!("✓ {path}: no errors found");
        }
        Err(e) => {
            eprintln!("{}", format_error_with_context(&e, &source));
            process::exit(1);
        }
    }
}

fn emit_ir(path: &str) {
    let source = read_source(path);
    let program = lex_and_parse(&source, path);

    let mut generator = LlvmIrGenerator::new();

    match generator.generate(&program) {
        Ok(ir) => {
            print!("{ir}");
        }
        Err(e) => {
            eprintln!("{}", format_error_with_context(&e, &source));
            process::exit(1);
        }
    }
}
